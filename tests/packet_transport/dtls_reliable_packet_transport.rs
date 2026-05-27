use rust_comms::api::bingle_api::NetworkEndpoint;
use rust_comms::dtls::dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use rust_comms::dtls::UdpNetworkMux;
use rust_comms::packet_transport::{
    DtlsReliablePacketTransport,
    PacketTransport,
    PacketTransportHandleMessage,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct MockDtls {
    handle_message: Option<HandleMessage>,
    sent_packets: Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
    auto_ack_on_send: bool,
}

impl MockDtls {
    fn new() -> Self {
        Self::new_with_auto_ack(false)
    }

    fn new_with_auto_ack(auto_ack_on_send: bool) -> Self {
        Self {
            handle_message: None,
            sent_packets: Arc::new(Mutex::new(vec![])),
            auto_ack_on_send,
        }
    }
}

fn new_transport_with_sent_packets(
    mtu: usize,
) -> (
    DtlsReliablePacketTransport,
    Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
) {
    new_transport_with_sent_packets_and_ack(mtu, true)
}

fn new_transport_with_sent_packets_and_ack(
    mtu: usize,
    auto_ack_on_send: bool,
) -> (
    DtlsReliablePacketTransport,
    Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
) {
    let sent_packets: Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = Arc::new(Mutex::new(vec![]));
    let dtls = MockDtls {
        handle_message: None,
        sent_packets: sent_packets.clone(),
        auto_ack_on_send,
    };
    (
        DtlsReliablePacketTransport::new(Box::new(dtls), mtu),
        sent_packets,
    )
}

impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> DtlsResult<()> {
        Ok(())
    }

    fn stop(&mut self) -> DtlsResult<()> {
        Ok(())
    }

    fn send(&self, to: &NetworkEndpoint, data: &[u8]) -> DtlsResult<()> {
        let address = to
            .inet_socket_address()
            .expect("MockDtls::send requires inet socket address");
        self.sent_packets
            .lock()
            .expect("sent_packets lock should not be poisoned")
            .push((address, data.to_vec()));

        if self.auto_ack_on_send && data.len() >= 4 {
            let packet_type = data[0] & 0x0F;
            if packet_type == 0x01 {
                if let Some(handler) = self.handle_message.clone() {
                    let ack_complete_packet = vec![0x14, 0x00, data[2], data[3]];
                    handler(self, to, "mock-auto-ack", &ack_complete_packet);
                }
            }
        }

        Ok(())
    }

    fn get_handle_message(&self) -> Option<HandleMessage> {
        self.handle_message.clone()
    }

    fn set_handle_message(&mut self, handler: Option<HandleMessage>) {
        self.handle_message = handler;
    }

    fn set_handle_new_session(&mut self, _handler: Option<rust_comms::dtls::dtls_trait::HandleNewSession>) {}

    fn with_handle_message(mut self, handler: HandleMessage) -> Self
    where
        Self: Sized,
    {
        self.handle_message = Some(handler);
        self
    }

    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> {
        None
    }

    fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) {}

    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self
    where
        Self: Sized,
    {
        self
    }

    fn get_ca_cert(&self) -> Option<&[u8]> {
        None
    }

    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}

    fn with_ca_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }

    fn get_client_cert(&self) -> Option<&[u8]> {
        None
    }

    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}

    fn with_client_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }

    fn get_client_private_key(&self) -> Option<&[u8]> {
        None
    }

    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}

    fn with_client_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }

    fn get_server_signing_cert(&self) -> Option<&[u8]> {
        None
    }

    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}

    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }

    fn get_server_signing_private_key(&self) -> Option<&[u8]> {
        None
    }

    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}

    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }

    fn set_app_layer_only_verification(&mut self, _enabled: bool) {}

    fn with_app_layer_only_verification(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }

    fn set_dangerous_debug(&mut self, _enabled: bool) {}

    fn with_dangerous_debug(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }

    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self where Self: Sized { self }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn new_installs_dtls_handler_that_uses_transport_handler() {
    let mut transport = DtlsReliablePacketTransport::new(Box::new(MockDtls::new()), 1492);

    let calls: Arc<Mutex<Vec<(String, Vec<u8>)>>> = Arc::new(Mutex::new(vec![]));
    let calls_clone = calls.clone();
    let transport_handler: PacketTransportHandleMessage = Arc::new(move |_from, issuer, packet| {
        calls_clone
            .lock()
            .expect("calls lock should not be poisoned")
            .push((issuer.to_string(), packet.to_vec()));
        Ok(Some(packet.to_vec()))
    });
    transport.set_handle_message(Some(transport_handler));

    let dtls_handler = transport
        .dtls()
        .get_handle_message()
        .expect("DTLS handler should be installed by packet transport constructor");

    let from_addr: SocketAddr = "127.0.0.1:9090".parse().expect("valid socket address");
    let from = NetworkEndpoint::new_direct(from_addr);
    dtls_handler(transport.dtls(), &from, "peer-issuer", b"hello");

    let locked_calls = calls
        .lock()
        .expect("calls lock should not be poisoned after dtls callback invocation");
    assert_eq!(locked_calls.len(), 1);
    assert_eq!(locked_calls[0].0, "peer-issuer");
    assert_eq!(locked_calls[0].1, b"hello".to_vec());
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn with_handle_message_and_dispatch_handle_message_are_transport_instance_scoped() {
    let captured: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(vec![]));
    let captured_clone = captured.clone();
    let handler: PacketTransportHandleMessage = Arc::new(move |_from, _issuer, packet| {
        captured_clone
            .lock()
            .expect("captured lock should not be poisoned")
            .push(packet.to_vec());
        Ok(Some(vec![0xAA]))
    });

    let transport = DtlsReliablePacketTransport::new(Box::new(MockDtls::new()), 1492).with_handle_message(handler);
    assert!(transport.get_handle_message().is_some());

    let from_addr: SocketAddr = "127.0.0.1:9191".parse().expect("valid socket address");
    let from = NetworkEndpoint::new_direct(from_addr);
    let handled = transport
        .dispatch_handle_message(&from, "issuer-a", b"payload")
        .expect("transport handler should return success");
    assert!(handled.is_some());
    assert_eq!(handled.expect("handled payload should exist"), vec![0xAA]);

    let captured_packets = captured
        .lock()
        .expect("captured lock should not be poisoned after handle_message");
    assert_eq!(captured_packets.as_slice(), &[b"payload".to_vec()]);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn mtu_is_set_in_constructor_and_can_be_updated() {
    let mut transport = DtlsReliablePacketTransport::new(Box::new(MockDtls::new()), 1492);
    assert_eq!(transport.mtu(), 1492);

    transport.set_mtu(1200);
    assert_eq!(transport.mtu(), 1200);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn send_wraps_payload_as_data_single_and_increments_tx_id() {
    let (transport, sent_packets) = new_transport_with_sent_packets(1492);
    let to_addr: SocketAddr = "127.0.0.1:7001".parse().expect("valid socket address");
    let to = NetworkEndpoint::new_direct(to_addr);

    transport
        .send(&to, b"hello")
        .expect("first DATA_SINGLE send should succeed");
    transport
        .send(&to, b"world")
        .expect("second DATA_SINGLE send should succeed");

    let packets = sent_packets
        .lock()
        .expect("sent_packets lock should not be poisoned");
    assert_eq!(packets.len(), 2);
    assert_eq!(packets[0].0, to_addr);
    assert_eq!(packets[1].0, to_addr);

    assert_eq!(packets[0].1, vec![0x11, 0x00, 0x00, 0x00, b'h', b'e', b'l', b'l', b'o']);
    assert_eq!(packets[1].1, vec![0x11, 0x00, 0x00, 0x01, b'w', b'o', b'r', b'l', b'd']);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn send_rejects_payload_that_requires_fragmentation() {
    let (transport, _sent_packets) = new_transport_with_sent_packets(8);
    let to_addr: SocketAddr = "127.0.0.1:7002".parse().expect("valid socket address");
    let to = NetworkEndpoint::new_direct(to_addr);

    let err = transport
        .send(&to, b"12345")
        .expect_err("payload larger than mtu-4 should fail until fragmentation is implemented");
    assert!(err.contains("exceeds DATA_SINGLE capacity"));
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn send_waits_for_ack_complete_before_returning() {
    let (mut transport, sent_packets) = new_transport_with_sent_packets_and_ack(1492, false);
    transport.set_ack_wait_timeout(Duration::from_millis(300));

    let transport = Arc::new(transport);
    let to_addr: SocketAddr = "127.0.0.1:7005".parse().expect("valid socket address");
    let to = NetworkEndpoint::new_direct(to_addr);

    let transport_for_ack = transport.clone();
    let ack_thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        let from = NetworkEndpoint::new_direct(to_addr);
        let ack_complete_packet = vec![0x14, 0x00, 0x00, 0x00];
        let handled = transport_for_ack
            .dispatch_handle_message(&from, "issuer-ack", &ack_complete_packet)
            .expect("ACK_COMPLETE dispatch should succeed");
        assert!(handled.is_none());
    });

    let started = Instant::now();
    transport
        .send(&to, b"hello")
        .expect("send should succeed after ACK_COMPLETE arrives");
    let elapsed = started.elapsed();

    ack_thread
        .join()
        .expect("ack thread should complete without panic");

    assert!(
        elapsed >= Duration::from_millis(60),
        "send should wait for ACK_COMPLETE before returning; elapsed={elapsed:?}"
    );

    let packets = sent_packets
        .lock()
        .expect("sent_packets lock should not be poisoned");
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].0, to_addr);
    assert_eq!(packets[0].1, vec![0x11, 0x00, 0x00, 0x00, b'h', b'e', b'l', b'l', b'o']);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn send_handles_ack_complete_timeout_failure_path_by_waiting_then_continuing() {
    let (mut transport, sent_packets) = new_transport_with_sent_packets_and_ack(1492, false);
    transport.set_ack_wait_timeout(Duration::from_millis(25));

    let to_addr: SocketAddr = "127.0.0.1:7006".parse().expect("valid socket address");
    let to = NetworkEndpoint::new_direct(to_addr);

    let started = Instant::now();
    transport
        .send(&to, b"hello")
        .expect("send should continue after waiting for ACK_COMPLETE timeout");
    let elapsed = started.elapsed();

    assert!(
        elapsed >= Duration::from_millis(20),
        "send should wait approximately for timeout; elapsed={elapsed:?}"
    );

    let packets = sent_packets
        .lock()
        .expect("sent_packets lock should not be poisoned");
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].0, to_addr);
    assert_eq!(packets[0].1, vec![0x11, 0x00, 0x00, 0x00, b'h', b'e', b'l', b'l', b'o']);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn data_single_dispatch_acks_and_suppresses_duplicate_delivery() {
    let (mut transport, sent_packets) = new_transport_with_sent_packets(1492);

    let delivered_payloads: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(vec![]));
    let delivered_payloads_clone = delivered_payloads.clone();
    transport.set_handle_message(Some(Arc::new(move |_from, _issuer, packet| {
        delivered_payloads_clone
            .lock()
            .expect("delivered_payloads lock should not be poisoned")
            .push(packet.to_vec());
        Ok(Some(packet.to_vec()))
    })));

    let from_addr: SocketAddr = "127.0.0.1:7003".parse().expect("valid socket address");
    let from = NetworkEndpoint::new_direct(from_addr);
    let data_single_packet = vec![0x11, 0x00, 0x12, 0x34, b'd', b'a', b't', b'a'];

    let first = transport
        .dispatch_handle_message(&from, "issuer-a", &data_single_packet)
        .expect("first DATA_SINGLE dispatch should succeed");
    assert_eq!(first, Some(b"data".to_vec()));

    let second = transport
        .dispatch_handle_message(&from, "issuer-a", &data_single_packet)
        .expect("duplicate DATA_SINGLE dispatch should succeed");
    assert!(second.is_none());

    let delivered = delivered_payloads
        .lock()
        .expect("delivered_payloads lock should not be poisoned after dispatch");
    assert_eq!(delivered.as_slice(), &[b"data".to_vec()]);

    let sent = sent_packets
        .lock()
        .expect("sent_packets lock should not be poisoned after dispatch");
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].0, from_addr);
    assert_eq!(sent[1].0, from_addr);
    assert_eq!(sent[0].1, vec![0x14, 0x00, 0x12, 0x34]);
    assert_eq!(sent[1].1, vec![0x14, 0x00, 0x12, 0x34]);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ack_complete_is_consumed_and_not_forwarded_to_handler() {
    let (mut transport, sent_packets) = new_transport_with_sent_packets(1492);

    let calls: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(vec![]));
    let calls_clone = calls.clone();
    transport.set_handle_message(Some(Arc::new(move |_from, _issuer, packet| {
        calls_clone
            .lock()
            .expect("calls lock should not be poisoned")
            .push(packet.to_vec());
        Ok(Some(packet.to_vec()))
    })));

    let from_addr: SocketAddr = "127.0.0.1:7004".parse().expect("valid socket address");
    let from = NetworkEndpoint::new_direct(from_addr);
    let ack_complete_packet = vec![0x14, 0x00, 0x00, 0x07];

    let handled = transport
        .dispatch_handle_message(&from, "issuer-b", &ack_complete_packet)
        .expect("ACK_COMPLETE dispatch should succeed");
    assert!(handled.is_none());

    let forwarded_calls = calls
        .lock()
        .expect("calls lock should not be poisoned after ACK_COMPLETE");
    assert!(forwarded_calls.is_empty());

    let sent = sent_packets
        .lock()
        .expect("sent_packets lock should not be poisoned after ACK_COMPLETE");
    assert!(sent.is_empty());
}
