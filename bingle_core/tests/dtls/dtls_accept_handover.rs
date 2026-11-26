// An inbound ClientHello from an endpoint that already has an established session. It may be the
// peer restarting, but it may also be forged with the peer's address, or be a retransmission of a
// hello already being handled. The established session must survive anything except a client that
// proves, with a valid HelloVerifyRequest cookie, that it receives replies at that address.
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::NetworkEndpoint;
use bingle_core::dtls::{Dtls, DtlsOpenSsl, NetworkMux, UdpNetworkMux};

use super::pki;

fn accept_any_peer(_cert: &[u8], _ca: &[u8]) -> bingle_core::dtls::Result<String> {
    Ok("TEST-ISSUER".to_string())
}

/// A started DTLS node on loopback, recording the application payloads it receives.
struct Node {
    dtls: DtlsOpenSsl,
    mux: Arc<UdpNetworkMux>,
    addr: SocketAddr,
    received: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Node {
    fn start(name: &str, port: u16, accept_timeout: Duration) -> Node {
        let received: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = received.clone();
        let certs = pki::generate_ed25519_test_certs();
        let dtls = DtlsOpenSsl::new(name.to_string())
            .with_null_encryption()
            .with_accept_handshake_timeout(accept_timeout)
            .with_handle_message(Arc::new(
                move |_d: &dyn Dtls, _from, _issuer, data: &[u8]| {
                    if let Ok(mut r) = sink.lock() {
                        r.push(data.to_vec());
                    }
                },
            ))
            .with_client_cert(certs.client_crt.clone())
            .with_client_private_key(certs.client_key.clone())
            .with_server_signing_cert(certs.server_crt.clone())
            .with_server_signing_private_key(certs.server_key.clone())
            .with_ca_cert(certs.ca_crt.clone())
            .with_handle_peer_certificate(accept_any_peer);
        let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", port)).expect("bind mux"));
        let addr = mux.local_addr().expect("mux addr");
        mux.start().expect("mux start");
        dtls.start(mux.clone()).expect("dtls start");
        Node {
            dtls,
            mux,
            addr,
            received,
        }
    }

    fn got(&self, payload: &[u8], timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(r) = self.received.lock()
                && r.iter().any(|p| p == payload)
            {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn stop(&self) {
        let _ = self.dtls.stop();
        self.mux.stop();
    }
}

/// A minimal DTLS 1.2 ClientHello datagram, with `cookie` in its cookie field. It is well formed
/// enough for the accept path to treat it as a ClientHello.
fn client_hello(cookie: &[u8]) -> Vec<u8> {
    let mut body = vec![0xfe, 0xfd]; // client_version DTLS 1.2
    body.extend_from_slice(&[0x42; 32]); // random
    body.push(0); // session_id
    body.push(cookie.len() as u8);
    body.extend_from_slice(cookie);
    body.extend_from_slice(&[0x00, 0x02, 0xc0, 0x2b]); // one cipher suite
    body.extend_from_slice(&[0x01, 0x00]); // null compression
    let len = body.len() as u32;
    let mut handshake = vec![0x01]; // client_hello
    handshake.extend_from_slice(&len.to_be_bytes()[1..]);
    handshake.extend_from_slice(&[0x00, 0x00]); // message_seq
    handshake.extend_from_slice(&[0x00, 0x00, 0x00]); // fragment_offset
    handshake.extend_from_slice(&len.to_be_bytes()[1..]); // fragment_length
    handshake.extend_from_slice(&body);
    let mut record = vec![0x16, 0xfe, 0xfd, 0x00, 0x00]; // handshake, DTLS 1.2, epoch 0
    record.extend_from_slice(&[0, 0, 0, 0, 0, 7]); // sequence number
    record.extend_from_slice(&(handshake.len() as u16).to_be_bytes());
    record.extend_from_slice(&handshake);
    record
}

/// Sends `datagram` to `to` from `from`'s own socket, so it carries the address and port of
/// `from`'s session: exactly what a forger spoofing that endpoint would send.
fn forge_from(from: &Node, to: SocketAddr, datagram: &[u8]) {
    from.mux
        .write(&NetworkEndpoint::new_direct(to), datagram)
        .expect("forged write");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_client_hello_without_a_cookie_is_answered_with_a_hello_verify_request() {
    let server = Node::start("hvr-server", 0, Duration::from_secs(2));
    let probe = UdpSocket::bind("127.0.0.1:0").expect("bind probe");
    probe
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("probe timeout");
    probe
        .send_to(&client_hello(&[]), server.addr)
        .expect("send hello");
    let mut buf = [0u8; 2048];
    let (n, _) = probe
        .recv_from(&mut buf)
        .expect("a reply to the ClientHello");
    assert!(n > 14, "reply too short: {} bytes", n);
    assert_eq!(buf[0], 0x16, "a handshake record");
    assert_eq!(buf[13], 0x03, "a HelloVerifyRequest, not a ServerHello");
    server.stop();
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_forged_client_hello_does_not_disturb_an_established_session() {
    let server = Node::start("forge-server", 0, Duration::from_secs(1));
    let client = Node::start("forge-client", 0, Duration::from_secs(1));
    let to_server = NetworkEndpoint::new_direct(server.addr);
    let to_client = NetworkEndpoint::new_direct(client.addr);

    let sessions = Arc::new(AtomicU64::new(0));
    let counter = sessions.clone();
    client
        .dtls
        .set_handle_new_outbound_session(Some(Arc::new(move |_to: &NetworkEndpoint| {
            counter.fetch_add(1, Ordering::SeqCst);
        })));

    client.dtls.send(&to_server, b"before").expect("first send");
    assert!(server.got(b"before", Duration::from_secs(5)));

    // Forged hellos from the client's address: no cookie, and a cookie the server never issued.
    forge_from(&client, server.addr, &client_hello(&[]));
    forge_from(&client, server.addr, &client_hello(&[0x5a; 16]));
    thread::sleep(Duration::from_millis(200));

    // Both directions still use the established session.
    client
        .dtls
        .send(&to_server, b"during")
        .expect("send after forgery");
    assert!(
        server.got(b"during", Duration::from_secs(3)),
        "the server dropped the client's session on a forged ClientHello"
    );
    server.dtls.send(&to_client, b"reply").expect("server send");
    assert!(client.got(b"reply", Duration::from_secs(3)));

    // The stray handshake times out (1 s here); the session still works afterwards.
    thread::sleep(Duration::from_millis(1500));
    client
        .dtls
        .send(&to_server, b"after")
        .expect("send after timeout");
    assert!(server.got(b"after", Duration::from_secs(3)));
    assert_eq!(
        sessions.load(Ordering::SeqCst),
        1,
        "the client never had to start a new session"
    );

    client.stop();
    server.stop();
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_restarted_client_on_the_same_port_replaces_its_session() {
    let server = Node::start("restart-server", 0, Duration::from_secs(5));
    let first = Node::start("restart-client-1", 0, Duration::from_secs(5));
    let port = first.addr.port();
    let to_server = NetworkEndpoint::new_direct(server.addr);

    first.dtls.send(&to_server, b"one").expect("first send");
    assert!(server.got(b"one", Duration::from_secs(5)));
    first.stop();
    drop(first);

    // The same address and port, with no memory of the old session: a full new handshake, which
    // completes the cookie exchange and so replaces the server's session.
    let second = Node::start("restart-client-2", port, Duration::from_secs(5));
    second
        .dtls
        .send(&to_server, b"two")
        .expect("send after restart");
    assert!(
        server.got(b"two", Duration::from_secs(5)),
        "the restarted client's new session was not accepted"
    );
    server
        .dtls
        .send(&NetworkEndpoint::new_direct(second.addr), b"back")
        .expect("server send to restarted client");
    assert!(second.got(b"back", Duration::from_secs(5)));

    second.stop();
    server.stop();
}

/// A UDP relay between one client and `server` that delivers every client datagram twice, as a
/// network that duplicates packets (or a client that retransmits early) would.
struct Duplicator {
    inside: Arc<UdpSocket>,
    running: Arc<AtomicBool>,
}

impl Duplicator {
    fn new(server: SocketAddr) -> Duplicator {
        let inside = Arc::new(UdpSocket::bind("127.0.0.1:0").expect("bind inside"));
        let outside = Arc::new(UdpSocket::bind("127.0.0.1:0").expect("bind outside"));
        for s in [&inside, &outside] {
            s.set_read_timeout(Some(Duration::from_millis(50)))
                .expect("timeout");
        }
        let running = Arc::new(AtomicBool::new(true));
        let client: Arc<Mutex<Option<SocketAddr>>> = Arc::new(Mutex::new(None));
        {
            let (inside, outside, client, running) = (
                inside.clone(),
                outside.clone(),
                client.clone(),
                running.clone(),
            );
            thread::spawn(move || {
                let mut buf = [0u8; 65536];
                while running.load(Ordering::SeqCst) {
                    if let Ok((n, from)) = inside.recv_from(&mut buf) {
                        *client.lock().unwrap() = Some(from);
                        let _ = outside.send_to(&buf[..n], server);
                        let _ = outside.send_to(&buf[..n], server);
                    }
                }
            });
        }
        {
            let (inside, running) = (inside.clone(), running.clone());
            thread::spawn(move || {
                let mut buf = [0u8; 65536];
                while running.load(Ordering::SeqCst) {
                    if let Ok((n, _)) = outside.recv_from(&mut buf)
                        && let Some(to) = *client.lock().unwrap()
                    {
                        let _ = inside.send_to(&buf[..n], to);
                    }
                }
            });
        }
        Duplicator { inside, running }
    }

    fn addr(&self) -> SocketAddr {
        self.inside.local_addr().expect("inside addr")
    }
}

impl Drop for Duplicator {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn duplicated_client_hellos_do_not_restart_the_handshake() {
    let server = Node::start("dup-server", 0, Duration::from_secs(5));
    let client = Node::start("dup-client", 0, Duration::from_secs(5));
    let relay = Duplicator::new(server.addr);
    let via_relay = NetworkEndpoint::new_direct(relay.addr());

    client
        .dtls
        .send(&via_relay, b"one")
        .expect("send through duplicator");
    assert!(
        server.got(b"one", Duration::from_secs(5)),
        "a duplicated ClientHello broke the handshake"
    );
    client.dtls.send(&via_relay, b"two").expect("second send");
    assert!(server.got(b"two", Duration::from_secs(5)));

    client.stop();
    server.stop();
}

/// Runs `openssl s_client` against `server` with a small MTU and a long cipher list, so its
/// ClientHello is split across several datagrams. Returns its `-msg` output, or `None` when no
/// suitable OpenSSL CLI is installed.
fn s_client_with_fragmented_hello(server: SocketAddr) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let version = Command::new("openssl").arg("version").output().ok()?;
    let version = String::from_utf8_lossy(&version.stdout).into_owned();
    if version.starts_with("LibreSSL") || version.is_empty() {
        return None;
    }
    let mut child = Command::new("openssl")
        .args(["s_client", "-dtls1_2", "-mtu", "256", "-msg"])
        .args(["-cipher", "ALL:@SECLEVEL=0"])
        .args(["-connect", &server.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = child.stdout.take()?;
    let output = Arc::new(Mutex::new(String::new()));
    let sink = output.clone();
    thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(n) = out.read(&mut chunk) {
            if n == 0 {
                break;
            }
            if let Ok(mut o) = sink.lock() {
                o.push_str(&String::from_utf8_lossy(&chunk[..n]));
            }
        }
    });
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if output
            .lock()
            .map(|o| o.contains("ServerHello"))
            .unwrap_or(false)
        {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    thread::sleep(Duration::from_millis(100));
    output.lock().ok().map(|o| o.clone())
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_client_hello_split_across_datagrams_gets_a_server_hello() {
    let server = Node::start("frag-server", 0, Duration::from_secs(5));
    let Some(output) = s_client_with_fragmented_hello(server.addr) else {
        eprintln!("[skipped] no OpenSSL CLI on PATH");
        server.stop();
        return;
    };
    // The client's first flight is several records: its ClientHello was fragmented.
    let first_flight = output.split("<<<").next().unwrap_or("");
    // (The first flight's records carry record version DTLS 1.0, as RFC 6347 allows.)
    let records = first_flight
        .lines()
        .filter(|l| l.starts_with(">>> DTLS") && l.contains("RecordHeader"))
        .count();
    assert!(
        records >= 2,
        "the ClientHello was not fragmented ({} records):\n{}",
        records,
        output
    );
    assert!(
        output.contains("<<< DTLS 1.2, Handshake") && output.contains("ServerHello"),
        "the server never answered the fragmented ClientHello with a ServerHello:\n{}",
        output
    );
    server.stop();
}
