//! Multi-packet blocks through `DtlsReliablePacketTransport` (spec §5.3, §6.3–6.4, issue #518):
//! two transports linked through mock DTLS layers that can lose packets, and one real DTLS
//! loopback transfer.

use super::dtls_reliable_packet_transport::{MockDtls, SendHook};
use super::reassembly::test_block;
use crate::util::pki;
use crate::util::test_util::CapturedPackets;
use bingle_core::api::bingle_api::NetworkEndpoint;
use bingle_core::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};
use bingle_core::packet_transport::frame::{FragHeader, Frame, PACKET_MAX, build_data_frag, parse};
use bingle_core::packet_transport::reassembly::DEFAULT_PER_PEER_REASSEMBLY_BYTES;
use bingle_core::packet_transport::{DtlsReliablePacketTransport, PacketTransport};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

/// Decides whether the link loses a packet.
type DropRule = Arc<dyn Fn(&[u8]) -> bool + Send + Sync>;

fn lose_nothing() -> DropRule {
    Arc::new(|_| false)
}

/// Loses the first transmission of each listed `DATA_FRAG` index.
fn lose_first_transmission_of(indices: &[u16]) -> DropRule {
    let pending: Mutex<HashSet<u16>> = Mutex::new(indices.iter().copied().collect());
    Arc::new(move |packet| match parse(packet) {
        Some(Frame::DataFrag { header, .. }) => pending
            .lock()
            .map(|mut p| p.remove(&header.frag_index))
            .unwrap_or(false),
        _ => false,
    })
}

/// Loses the first `count` `ACK_WINDOW` packets.
fn lose_first_ack_windows(count: usize) -> DropRule {
    let remaining = Mutex::new(count);
    Arc::new(move |packet| match parse(packet) {
        Some(Frame::AckWindow { .. }) => match remaining.lock() {
            Ok(mut r) if *r > 0 => {
                *r -= 1;
                true
            }
            _ => false,
        },
        _ => false,
    })
}

type TransportSlot = Arc<Mutex<Weak<DtlsReliablePacketTransport>>>;

/// A hook that passes each packet not lost by `drop` to the transport in `peer`, as if from `from`.
fn forward_to(peer: TransportSlot, from: SocketAddr, drop: DropRule) -> SendHook {
    Arc::new(move |_to, data| {
        if drop(data) {
            return;
        }
        let target = peer.lock().ok().and_then(|w| w.upgrade());
        if let Some(target) = target {
            let _ =
                target.dispatch_handle_message(&NetworkEndpoint::new_direct(from), "peer", data);
        }
    })
}

/// A sender and a receiver transport joined by lossy mock DTLS layers.
struct Link {
    sender: Arc<DtlsReliablePacketTransport>,
    receiver: Arc<DtlsReliablePacketTransport>,
    sender_addr: SocketAddr,
    receiver_addr: SocketAddr,
    sent_by_sender: CapturedPackets,
    sent_by_receiver: CapturedPackets,
    forgotten_by_sender: Arc<Mutex<Vec<SocketAddr>>>,
    delivered: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Link {
    fn new(mtu: usize, sender_loses: DropRule, receiver_loses: DropRule) -> Self {
        let sender_addr: SocketAddr = "127.0.0.1:7201".parse().expect("valid socket address");
        let receiver_addr: SocketAddr = "127.0.0.1:7202".parse().expect("valid socket address");
        let sender_slot: TransportSlot = Arc::new(Mutex::new(Weak::new()));
        let receiver_slot: TransportSlot = Arc::new(Mutex::new(Weak::new()));
        let sent_by_sender: CapturedPackets = Arc::new(Mutex::new(vec![]));
        let sent_by_receiver: CapturedPackets = Arc::new(Mutex::new(vec![]));
        let forgotten_by_sender: Arc<Mutex<Vec<SocketAddr>>> = Arc::new(Mutex::new(vec![]));

        let sender = Arc::new(DtlsReliablePacketTransport::new(
            Box::new(MockDtls::with_send_hook(
                sent_by_sender.clone(),
                forgotten_by_sender.clone(),
                forward_to(receiver_slot.clone(), sender_addr, sender_loses),
            )),
            mtu,
        ));
        let receiver = Arc::new(DtlsReliablePacketTransport::new(
            Box::new(MockDtls::with_send_hook(
                sent_by_receiver.clone(),
                Arc::new(Mutex::new(vec![])),
                forward_to(sender_slot.clone(), receiver_addr, receiver_loses),
            )),
            mtu,
        ));
        *sender_slot.lock().expect("slot lock") = Arc::downgrade(&sender);
        *receiver_slot.lock().expect("slot lock") = Arc::downgrade(&receiver);

        let delivered: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(vec![]));
        let sink = delivered.clone();
        receiver.set_handle_message(Some(Arc::new(move |_d, _from, _issuer, block| {
            sink.lock().expect("delivered lock").push(block.to_vec());
            Ok(None)
        })));
        let fast = vec![
            Duration::from_millis(50),
            Duration::from_millis(100),
            Duration::from_millis(200),
        ];
        sender.set_retry_delays(fast.clone());
        receiver.set_retry_delays(fast);

        Self {
            sender,
            receiver,
            sender_addr,
            receiver_addr,
            sent_by_sender,
            sent_by_receiver,
            forgotten_by_sender,
            delivered,
        }
    }

    fn send(&self, block: &[u8]) -> Result<(), String> {
        self.sender
            .send(&NetworkEndpoint::new_direct(self.receiver_addr), block)
    }

    fn delivered(&self) -> Vec<Vec<u8>> {
        self.delivered.lock().expect("delivered lock").clone()
    }

    fn sender_packets(&self) -> Vec<Vec<u8>> {
        self.sent_by_sender
            .lock()
            .expect("sent lock")
            .iter()
            .map(|(_, p)| p.clone())
            .collect()
    }
}

fn data_frag_count(packets: &[Vec<u8>]) -> usize {
    packets
        .iter()
        .filter(|p| matches!(parse(p), Some(Frame::DataFrag { .. })))
        .count()
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_block_that_fits_one_packet_still_goes_as_data_single() {
    let link = Link::new(1492, lose_nothing(), lose_nothing());
    let fits = test_block(1488);
    link.send(&fits).expect("single-packet send");
    let packets = link.sender_packets();
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0][0], 0x11, "DATA_SINGLE");
    assert_eq!(packets[0].len(), 1492);

    let one_more = test_block(1489);
    link.send(&one_more).expect("fragmented send");
    let packets = link.sender_packets();
    assert_eq!(data_frag_count(&packets[1..]), 2);
    assert!(packets.iter().all(|p| p.len() <= 1492));
    assert_eq!(link.delivered(), vec![fits, one_more]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_200_fragment_block_crosses_the_link_exactly_once() {
    let link = Link::new(1200, lose_nothing(), lose_nothing());
    let block = test_block(200 * 1188 - 100);
    link.send(&block).expect("fragmented send");
    let packets = link.sender_packets();
    assert_eq!(
        data_frag_count(&packets),
        200,
        "no retransmission on a clean link"
    );
    assert!(packets.iter().all(|p| p.len() <= 1200));
    assert_eq!(link.delivered(), vec![block]);
    assert_eq!(link.receiver.reassembly_usage().expect("usage"), (0, 0));
    assert_eq!(link.sender.largest_block_sent(), 200 * 1188 - 100);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn lost_fragments_and_acknowledgements_are_retransmitted() {
    let link = Link::new(
        1200,
        lose_first_transmission_of(&[0, 3, 64, 130, 199]),
        lose_first_ack_windows(3),
    );
    let block = test_block(200 * 1188 - 100);
    link.send(&block)
        .expect("send completes after retransmission");
    assert_eq!(link.delivered(), vec![block]);
    assert!(data_frag_count(&link.sender_packets()) > 200);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_fragment_of_a_delivered_block_is_acknowledged_again_but_not_redelivered() {
    let link = Link::new(1200, lose_nothing(), lose_nothing());
    let block = test_block(3 * 1188);
    link.send(&block).expect("fragmented send");
    let packets = link.sender_packets();
    let last_fragment = packets.last().expect("a fragment was sent").clone();
    let tx_id = match parse(&last_fragment) {
        Some(Frame::DataFrag { header, .. }) => header.tx_id,
        other => panic!("expected DATA_FRAG, got {other:?}"),
    };

    let handled = link
        .receiver
        .dispatch_handle_message(
            &NetworkEndpoint::new_direct(link.sender_addr),
            "peer",
            &last_fragment,
        )
        .expect("duplicate dispatch");
    assert!(handled.is_none());
    assert_eq!(link.delivered(), vec![block]);
    let receiver_sent = link.sent_by_receiver.lock().expect("sent lock");
    let tx = tx_id.to_be_bytes();
    assert_eq!(
        receiver_sent.last().map(|(_, p)| p.clone()),
        Some(vec![0x14, 0x00, tx[0], tx[1]])
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_fragmented_send_with_no_acknowledgement_fails_after_the_third_timeout() {
    let link = Link::new(1200, Arc::new(|_| true), lose_nothing());
    link.sender.set_retry_delays(vec![
        Duration::from_millis(20),
        Duration::from_millis(20),
        Duration::from_millis(20),
    ]);
    let err = link
        .send(&test_block(200 * 1188))
        .expect_err("nothing reaches the receiver");
    assert!(err.contains("no ACK_COMPLETE"), "{err}");
    // The first window, then two retransmissions of it, each on a fresh session.
    assert_eq!(data_frag_count(&link.sender_packets()), 3 * 64);
    assert_eq!(
        link.forgotten_by_sender
            .lock()
            .expect("forgotten lock")
            .len(),
        3,
        "fresh session for each retransmission, and forgotten after the last"
    );
    assert!(link.delivered().is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_block_over_packet_max_is_refused_before_sending() {
    let link = Link::new(1492, lose_nothing(), lose_nothing());
    let err = link
        .send(&vec![0u8; PACKET_MAX + 1])
        .expect_err("over PACKET_MAX");
    assert!(err.contains("PACKET_MAX"), "{err}");
    assert!(link.sender_packets().is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_fragment_announcing_a_large_block_holds_only_that_fragment() {
    let link = Link::new(1200, lose_nothing(), lose_nothing());
    let block_len = DEFAULT_PER_PEER_REASSEMBLY_BYTES as u32;
    let packet = build_data_frag(
        &FragHeader {
            tx_id: 9,
            frag_index: 0,
            frag_count: (block_len as usize).div_ceil(1188) as u16,
            block_len,
        },
        &[7u8; 1188],
    );
    let from = NetworkEndpoint::new_direct(link.sender_addr);
    link.receiver
        .dispatch_handle_message(&from, "peer", &packet)
        .expect("dispatch");
    assert_eq!(link.receiver.reassembly_usage().expect("usage"), (1, 1188));
    // The first fragment is acknowledged at once.
    let receiver_sent = link.sent_by_receiver.lock().expect("sent lock").clone();
    assert!(matches!(
        receiver_sent.last().and_then(|(_, p)| parse(p)),
        Some(Frame::AckWindow {
            tx_id: 9,
            base_index: 0,
            ack_bits: 1
        })
    ));
    drop(receiver_sent);

    // A new DTLS session from the peer releases its partial transfers.
    link.receiver.on_new_session(&from).expect("new session");
    assert_eq!(link.receiver.reassembly_usage().expect("usage"), (0, 0));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_fragment_claiming_more_than_packet_max_is_dropped_without_reply() {
    let link = Link::new(1200, lose_nothing(), lose_nothing());
    let packet = build_data_frag(
        &FragHeader {
            tx_id: 1,
            frag_index: 0,
            frag_count: 65_535,
            block_len: u32::MAX,
        },
        &[0u8; 1188],
    );
    let handled = link
        .receiver
        .dispatch_handle_message(
            &NetworkEndpoint::new_direct(link.sender_addr),
            "peer",
            &packet,
        )
        .expect("dispatch does not fail");
    assert!(handled.is_none());
    assert_eq!(link.receiver.reassembly_usage().expect("usage"), (0, 0));
    assert!(link.sent_by_receiver.lock().expect("sent lock").is_empty());
    assert!(link.delivered().is_empty());
}

fn accept_any_peer(_cert: &[u8], _ca: &[u8]) -> bingle_core::dtls::Result<String> {
    Ok("TEST-ISSUER".to_string())
}

/// A started reliable transport over real DTLS on loopback, recording the blocks it receives.
fn loopback_node(
    name: &str,
    mtu: usize,
) -> (
    DtlsReliablePacketTransport,
    SocketAddr,
    Arc<Mutex<Vec<Vec<u8>>>>,
) {
    let certs = pki::generate_ed25519_test_certs();
    let dtls = DtlsOpenSsl::new(name.to_string())
        .with_null_encryption()
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(certs.ca_crt.clone())
        .with_handle_peer_certificate(accept_any_peer);
    let transport = DtlsReliablePacketTransport::new(Box::new(dtls), mtu);
    let received: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = received.clone();
    transport.set_handle_message(Some(Arc::new(move |_d, _from, _issuer, block| {
        if let Ok(mut r) = sink.lock() {
            r.push(block.to_vec());
        }
        Ok(None)
    })));
    transport.set_retry_delays(vec![
        Duration::from_millis(500),
        Duration::from_secs(1),
        Duration::from_secs(2),
    ]);
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    let addr = mux.local_addr().expect("mux addr");
    mux.start().expect("mux start");
    transport.dtls().start(mux).expect("dtls start");
    (transport, addr, received)
}

#[ntest::timeout(120_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_8_mib_block_crosses_real_dtls_loopback_exactly() {
    let (receiver, receiver_addr, received) = loopback_node("frag-receiver", 1492);
    let (sender, _, _) = loopback_node("frag-sender", 1492);
    let to = NetworkEndpoint::new_direct(receiver_addr);

    // A small block first establishes the session.
    sender
        .send(&to, b"hello")
        .expect("handshake and first send");

    let block = test_block(8 * 1024 * 1024);
    let started = Instant::now();
    sender.send(&to, &block).expect("8 MiB fragmented send");
    let elapsed = started.elapsed();
    println!("8 MiB over DTLS loopback in {elapsed:?}");

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && received.lock().map(|r| r.len() < 2).unwrap_or(true) {
        thread::sleep(Duration::from_millis(10));
    }
    let got = received.lock().expect("received lock").clone();
    assert_eq!(got.len(), 2, "both blocks delivered exactly once");
    assert_eq!(got[0], b"hello".to_vec());
    assert!(got[1] == block, "the 8 MiB block reassembled exactly");
    assert_eq!(receiver.reassembly_usage().expect("usage"), (0, 0));

    let _ = sender.dtls().stop();
    let _ = receiver.dtls().stop();
}
