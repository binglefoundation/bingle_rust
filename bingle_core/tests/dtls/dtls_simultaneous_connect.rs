// Simultaneous-connect tie-break (issue #288): two peers that dial each other at once must converge
// on one DTLS session instead of both failing with `unexpected message`. Covers the ordering rule
// itself, and two real DtlsOpenSsl nodes on loopback sending to each other at the same moment.
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::NetworkEndpoint;
use bingle_core::dtls::dtls_openssl::openssl_impl::is_designated_client;
use bingle_core::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};

use super::pki;

fn addr(s: &str) -> SocketAddr {
    s.parse().expect("socket address")
}

#[test]
pub fn the_lower_port_is_the_designated_client() {
    // The case from #288: relay :12121, NATed client :41002.
    let relay = addr("18.208.207.10:12121");
    let client = addr("150.228.155.203:41002");
    assert!(is_designated_client(relay, client));
    assert!(!is_designated_client(client, relay));
}

#[test]
pub fn equal_ports_are_ordered_by_ip() {
    let a = addr("10.0.0.1:5000");
    let b = addr("10.0.0.2:5000");
    assert!(is_designated_client(a, b));
    assert!(!is_designated_client(b, a));
}

#[test]
pub fn exactly_one_side_is_the_designated_client() {
    let endpoints = [
        addr("127.0.0.1:1000"),
        addr("127.0.0.1:2000"),
        addr("10.0.0.9:1000"),
        addr("192.168.1.1:65000"),
        addr("[::1]:1000"),
    ];
    for a in endpoints {
        for b in endpoints {
            if a != b {
                assert_ne!(
                    is_designated_client(a, b),
                    is_designated_client(b, a),
                    "{a} vs {b}: both sides must reach opposite answers"
                );
            }
        }
    }
}

fn accept_any_peer(_cert: &[u8], _ca: &[u8]) -> bingle_core::dtls::Result<String> {
    Ok("TEST-ISSUER".to_string())
}

/// A started DTLS node on loopback, recording application payloads it receives and counting the
/// outbound sessions it establishes.
struct Node {
    dtls: DtlsOpenSsl,
    addr: SocketAddr,
    received: Arc<Mutex<Vec<Vec<u8>>>>,
    outbound_sessions: Arc<AtomicUsize>,
}

fn node(name: &str) -> Node {
    let certs = pki::generate_ed25519_test_certs();
    let received: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = received.clone();
    let dtls = DtlsOpenSsl::new(name.to_string())
        .with_null_encryption()
        .with_handle_message(Arc::new(
            move |_d: &dyn Dtls, _from, _issuer, data: &[u8]| {
                // Ignore stray DTLS records; keep only application payloads.
                if data.first().is_some_and(|b| *b == 22 || *b == 23) {
                    return;
                }
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
    let outbound_sessions = Arc::new(AtomicUsize::new(0));
    let counter = outbound_sessions.clone();
    dtls.set_handle_new_outbound_session(Some(Arc::new(move |_to: &NetworkEndpoint| {
        counter.fetch_add(1, Ordering::SeqCst);
    })));
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    let addr = mux.local_addr().expect("mux addr");
    mux.start().expect("mux start");
    dtls.start(mux).expect("dtls start");
    // On loopback the bound address is the public one.
    dtls.set_public_endpoint(Some(addr));
    Node {
        dtls,
        addr,
        received,
        outbound_sessions,
    }
}

fn wait_for_payload(node: &Node, payload: &[u8], timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if node
            .received
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

/// Both nodes send to each other; `delay` holds back the second node's send. Returns each send's
/// result as (a_to_b, b_to_a).
fn send_both_ways(
    a: &Arc<Node>,
    b: &Arc<Node>,
    delay: Duration,
) -> (Result<(), String>, Result<(), String>) {
    let barrier = Arc::new(Barrier::new(2));
    let (a1, b1, bar1) = (a.clone(), b.clone(), barrier.clone());
    let from_a = thread::spawn(move || {
        bar1.wait();
        a1.dtls
            .send(&NetworkEndpoint::new_direct(b1.addr), b"from a")
    });
    let (a2, b2, bar2) = (a.clone(), b.clone(), barrier);
    let from_b = thread::spawn(move || {
        bar2.wait();
        thread::sleep(delay);
        b2.dtls
            .send(&NetworkEndpoint::new_direct(a2.addr), b"from b")
    });
    (
        from_a.join().expect("a sender"),
        from_b.join().expect("b sender"),
    )
}

fn assert_converges(delay: Duration) {
    let a = Arc::new(node("sim-a"));
    let b = Arc::new(node("sim-b"));

    let (a_to_b, b_to_a) = send_both_ways(&a, &b, delay);
    assert!(a_to_b.is_ok(), "a -> b send failed: {a_to_b:?}");
    assert!(b_to_a.is_ok(), "b -> a send failed: {b_to_a:?}");
    assert!(
        wait_for_payload(&b, b"from a", Duration::from_secs(5)),
        "b did not receive a's payload"
    );
    assert!(
        wait_for_payload(&a, b"from b", Duration::from_secs(5)),
        "a did not receive b's payload"
    );
    // One session between them, not two half-open ones. When the ClientHellos collide it is the
    // designated (lower) client's; when one arrives first, whichever side dialled first. Which of
    // those happens is timing, so assert the invariant; the direction rule is unit-tested above.
    assert_eq!(
        a.outbound_sessions.load(Ordering::SeqCst) + b.outbound_sessions.load(Ordering::SeqCst),
        1,
        "exactly one outbound session between the two nodes"
    );
    let _ = a.dtls.stop();
    let _ = b.dtls.stop();
}

#[ntest::timeout(60_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn simultaneous_sends_converge_on_one_session() {
    // Both ClientHellos cross in flight. Repeated, since which side registers its connect first
    // varies run to run.
    for _ in 0..3 {
        assert_converges(Duration::ZERO);
    }
}

#[ntest::timeout(60_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_inbound_client_hello_after_the_outbound_connect_is_registered_converges() {
    // The second node dials a little after the first has registered its outbound connect, so the
    // first node sees the inbound ClientHello mid-handshake.
    assert_converges(Duration::from_millis(3));
}

#[ntest::timeout(60_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_inbound_connect_after_the_outbound_one_failed_is_accepted() {
    // The shape seen on TestNet: one side's outbound connect has already failed and been cleared
    // when the other side's ClientHello arrives. The tie-break does not depend on the two overlapping:
    // with no connect in progress the inbound one is simply accepted.
    let a = node("late-a");
    // An address that answers nothing, so a's connect times out and its state is cleared.
    let silent = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind silent");
    let silent_addr = silent.local_addr().expect("silent addr");
    assert!(
        a.dtls
            .send(&NetworkEndpoint::new_direct(silent_addr), b"nobody home")
            .is_err()
    );
    drop(silent);

    let b = node("late-b");
    assert!(
        b.dtls
            .send(&NetworkEndpoint::new_direct(a.addr), b"after the failure")
            .is_ok()
    );
    assert!(wait_for_payload(
        &a,
        b"after the failure",
        Duration::from_secs(5)
    ));
    let _ = a.dtls.stop();
    let _ = b.dtls.stop();
}
