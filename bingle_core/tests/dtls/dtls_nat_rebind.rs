// A NAT that moves a client to a new outside port (issue #483). A client behind NAT keeps a direct
// DTLS session to a reachable peer; after a quiet spell its NAT drops the mapping and the next
// datagram leaves from a new port. The peer sees an address it has no session for, so the old
// session's records mean nothing to it. The client must start a new session rather than keep
// writing into the old one.
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::NetworkEndpoint;
use bingle_core::dtls::dtls_openssl::openssl_impl::is_client_hello;
use bingle_core::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};
use bingle_core::packet_transport::{DtlsReliablePacketTransport, PacketTransport};

use super::pki;

/// How long the client's session may go without hearing from the peer before a send starts a new
/// one. Short, so the test doesn't wait out the production limit.
const IDLE_LIMIT: Duration = Duration::from_millis(300);

fn accept_any_peer(_cert: &[u8], _ca: &[u8]) -> bingle_core::dtls::Result<String> {
    Ok("TEST-ISSUER".to_string())
}

/// A started DTLS node on loopback, recording the application payloads it receives.
struct Node {
    dtls: DtlsOpenSsl,
    addr: SocketAddr,
    received: Arc<Mutex<Vec<Vec<u8>>>>,
}

fn node(name: &str, idle_limit: Duration) -> Node {
    let received: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let dtls = unstarted(name, idle_limit, received.clone());
    let addr = start(&dtls);
    Node {
        dtls,
        addr,
        received,
    }
}

/// A DTLS node recording the payloads it receives in `received`, not yet started.
fn unstarted(name: &str, idle_limit: Duration, received: Arc<Mutex<Vec<Vec<u8>>>>) -> DtlsOpenSsl {
    let certs = pki::generate_ed25519_test_certs();
    DtlsOpenSsl::new(name.to_string())
        .with_null_encryption()
        .with_direct_session_idle_limit(idle_limit)
        .with_handle_message(Arc::new(
            move |_d: &dyn Dtls, _from, _issuer, data: &[u8]| {
                if let Ok(mut r) = received.lock() {
                    r.push(data.to_vec());
                }
            },
        ))
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(certs.ca_crt.clone())
        .with_handle_peer_certificate(accept_any_peer)
}

/// Starts `dtls` on a loopback socket and returns its address.
fn start(dtls: &dyn Dtls) -> SocketAddr {
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    let addr = mux.local_addr().expect("mux addr");
    mux.start().expect("mux start");
    dtls.start(mux).expect("dtls start");
    addr
}

fn wait_for_payload(node: &Node, payload: &[u8], timeout: Duration) -> bool {
    wait_for(&node.received, payload, timeout)
}

fn wait_for(received: &Mutex<Vec<Vec<u8>>>, payload: &[u8], timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if received
            .lock()
            .map(|r| r.iter().any(|p| p == payload))
            .unwrap_or(false)
        {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

/// A one-client NAT in front of `server`. The client sends to `inside`; the NAT forwards from its
/// current outside socket, and passes the server's replies to that socket back. `rebind` moves the
/// client to a new outside port and drops the old mapping, as a NAT does after an idle timeout.
struct Nat {
    inside: Arc<UdpSocket>,
    server: SocketAddr,
    client: Arc<Mutex<Option<SocketAddr>>>,
    outside: Arc<Mutex<Arc<UdpSocket>>>,
    mapping: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
}

impl Nat {
    fn new(server: SocketAddr) -> Nat {
        let inside = Arc::new(UdpSocket::bind("127.0.0.1:0").expect("bind nat inside"));
        inside
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("inside timeout");
        let nat = Nat {
            inside,
            server,
            client: Arc::new(Mutex::new(None)),
            outside: Arc::new(Mutex::new(Arc::new(Self::outside_socket()))),
            mapping: Arc::new(AtomicU64::new(0)),
            running: Arc::new(AtomicBool::new(true)),
        };
        nat.forward_outbound();
        nat.forward_inbound(0);
        nat
    }

    fn outside_socket() -> UdpSocket {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("bind nat outside");
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("outside timeout");
        socket
    }

    fn inside_addr(&self) -> SocketAddr {
        self.inside.local_addr().expect("inside addr")
    }

    fn outside_addr(&self) -> SocketAddr {
        self.outside
            .lock()
            .unwrap()
            .local_addr()
            .expect("outside addr")
    }

    /// Client to server, from whichever outside socket is current.
    fn forward_outbound(&self) {
        let (inside, client, outside, server, running) = (
            self.inside.clone(),
            self.client.clone(),
            self.outside.clone(),
            self.server,
            self.running.clone(),
        );
        thread::spawn(move || {
            let mut buf = [0u8; 65536];
            while running.load(Ordering::SeqCst) {
                if let Ok((n, from)) = inside.recv_from(&mut buf) {
                    *client.lock().unwrap() = Some(from);
                    let socket = outside.lock().unwrap().clone();
                    let _ = socket.send_to(&buf[..n], server);
                }
            }
        });
    }

    /// Server to client, through the outside socket of mapping `mapping` while it is current.
    fn forward_inbound(&self, mapping: u64) {
        let socket = self.outside.lock().unwrap().clone();
        let (inside, client, current, running) = (
            self.inside.clone(),
            self.client.clone(),
            self.mapping.clone(),
            self.running.clone(),
        );
        thread::spawn(move || {
            let mut buf = [0u8; 65536];
            while running.load(Ordering::SeqCst) && current.load(Ordering::SeqCst) == mapping {
                if let Ok((n, _)) = socket.recv_from(&mut buf)
                    && current.load(Ordering::SeqCst) == mapping
                    && let Some(to) = *client.lock().unwrap()
                {
                    let _ = inside.send_to(&buf[..n], to);
                }
            }
        });
    }

    fn rebind(&self) {
        let mapping = self.mapping.fetch_add(1, Ordering::SeqCst) + 1;
        *self.outside.lock().unwrap() = Arc::new(Self::outside_socket());
        self.forward_inbound(mapping);
    }
}

impl Drop for Nat {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

#[ntest::timeout(20_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_send_after_the_nat_moves_the_client_reaches_the_peer() {
    let server = node("rebind-server", IDLE_LIMIT);
    let client = node("rebind-client", IDLE_LIMIT);
    let nat = Nat::new(server.addr);
    let via_nat = NetworkEndpoint::new_direct(nat.inside_addr());

    client.dtls.send(&via_nat, b"before").expect("first send");
    assert!(
        wait_for_payload(&server, b"before", Duration::from_secs(5)),
        "the server did not receive the first payload"
    );

    // The client goes quiet, and its NAT gives it a new outside port.
    let first_port = nat.outside_addr();
    nat.rebind();
    assert_ne!(nat.outside_addr(), first_port);
    thread::sleep(IDLE_LIMIT * 2);

    client.dtls.send(&via_nat, b"after").expect("second send");
    assert!(
        wait_for_payload(&server, b"after", Duration::from_secs(5)),
        "the server did not receive the payload sent after the NAT moved the client"
    );

    let _ = client.dtls.stop();
    let _ = server.dtls.stop();
}

#[ntest::timeout(20_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_session_in_use_is_kept() {
    // Within the idle limit the client keeps its session: a reply refreshes it, and the NAT
    // mapping it rides on is still open.
    let server = node("keep-server", Duration::from_secs(60));
    let client = node("keep-client", Duration::from_secs(60));
    let to_server = NetworkEndpoint::new_direct(server.addr);

    let sessions = Arc::new(AtomicU64::new(0));
    let counter = sessions.clone();
    client
        .dtls
        .set_handle_new_outbound_session(Some(Arc::new(move |_to: &NetworkEndpoint| {
            counter.fetch_add(1, Ordering::SeqCst);
        })));

    for payload in [&b"one"[..], b"two", b"three"] {
        client.dtls.send(&to_server, payload).expect("send");
        assert!(wait_for_payload(&server, payload, Duration::from_secs(5)));
    }
    assert_eq!(
        sessions.load(Ordering::SeqCst),
        1,
        "one session for all three sends"
    );

    let _ = client.dtls.stop();
    let _ = server.dtls.stop();
}

/// A DTLS 1.2 record header (13 bytes) and the first byte of its body.
fn record(content_type: u8, epoch: u16, first_body_byte: u8) -> Vec<u8> {
    let mut data = vec![content_type, 0xfe, 0xfd];
    data.extend_from_slice(&epoch.to_be_bytes());
    data.extend_from_slice(&[0, 0, 0, 0, 0, 1]); // sequence number
    data.extend_from_slice(&[0, 1]); // length
    data.push(first_body_byte);
    data
}

#[test]
pub fn only_an_epoch_zero_client_hello_opens_a_session() {
    assert!(is_client_hello(&record(22, 0, 1)));
    // Another handshake message, such as a Finished fragment.
    assert!(!is_client_hello(&record(22, 0, 20)));
    // An encrypted handshake record from an established session.
    assert!(!is_client_hello(&record(22, 1, 1)));
    // Application data and alerts from an established session.
    assert!(!is_client_hello(&record(23, 1, 1)));
    assert!(!is_client_hello(&record(21, 1, 1)));
    // Too short to hold a handshake header.
    assert!(!is_client_hello(&[22, 0xfe, 0xfd, 0, 0]));
}

/// A started DTLS node wrapped in the reliable packet transport, which acknowledges each block and
/// retries a send that gets no acknowledgement. Returns the blocks it receives. The transport is
/// installed before the node starts, as in the engine: the accept path keeps the handler it
/// started with.
fn reliable(
    name: &str,
    idle_limit: Duration,
) -> (
    DtlsReliablePacketTransport,
    SocketAddr,
    Arc<Mutex<Vec<Vec<u8>>>>,
) {
    let received: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let dtls = unstarted(name, idle_limit, received.clone());
    let transport = DtlsReliablePacketTransport::new(Box::new(dtls), 1200);
    let sink = received.clone();
    transport.set_handle_message(Some(Arc::new(move |_d, _from, _issuer, block| {
        if let Ok(mut r) = sink.lock() {
            r.push(block.to_vec());
        }
        Ok(None)
    })));
    transport.set_retry_delays(vec![
        Duration::from_millis(500),
        Duration::from_secs(3),
        Duration::from_secs(3),
    ]);
    let addr = start(transport.dtls());
    (transport, addr, received)
}

#[ntest::timeout(20_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_retry_after_the_nat_moves_the_client_starts_a_new_session() {
    // The NAT moves the client inside the idle limit, so the first attempt still goes out on the
    // old session and is lost. The retry must start a new session, which reaches the server.
    let limit = Duration::from_secs(60);
    let (server, server_addr, server_received) = reliable("retry-server", limit);
    let (client, _, _) = reliable("retry-client", limit);
    let nat = Nat::new(server_addr);
    let via_nat = NetworkEndpoint::new_direct(nat.inside_addr());

    client.send(&via_nat, b"before").expect("first send");
    assert!(wait_for(
        &server_received,
        b"before",
        Duration::from_secs(5)
    ));

    nat.rebind();
    client
        .send(&via_nat, b"after")
        .expect("the retry should be acknowledged over a new session");
    assert!(wait_for(&server_received, b"after", Duration::from_secs(5)));

    let _ = client.dtls().stop();
    let _ = server.dtls().stop();
}
