use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub mod pki;
pub mod test_handlers;
#[path = "../test_util.rs"]
pub mod test_util;

use rust_comms::api::bingle_api::NetworkEndpoint;
use rust_comms::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};
use test_util::init_test_logging;

fn make_counting_server_handler(
    hello_count: Arc<Mutex<u32>>,
) -> impl Fn(&dyn Dtls, &NetworkEndpoint, &str, &[u8]) + Send + Sync + 'static {
    move |_server, _from, _issuer, data| {
        if data == b"Hello" {
            let mut g = hello_count.lock().unwrap();
            *g += 1;
            tracing::info!("[test server] received Hello #{}", *g);
        }
    }
}

/// Verifies that after a network outage (client mux stopped and restarted on the same port),
/// a new `send()` successfully reconnects and the server receives the message.
///
/// This exercises the fix where a stale peer worker channel is detected and cleared,
/// allowing a fresh DTLS handshake to be performed.
///
/// Scenario:
/// 1. Client sends "Hello" — first handshake, peer worker created, server receives Hello #1.
/// 2. Client is fully stopped (simulating network outage).
/// 3. A new client is started on the same port — simulating reconnect after IP/port change.
/// 4. New client sends "Hello" — fresh handshake, server receives Hello #2.
#[ntest::timeout(60_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_reconnect_after_worker_channel_close() {
    init_test_logging();

    let certs = pki::generate_ed25519_test_certs();
    let ca_pem = certs.ca_crt.clone();

    // --- server ---
    let mux_srv = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind server mux"));
    let addr_srv: SocketAddr = mux_srv.local_addr().expect("server addr");

    let hello_count = Arc::new(Mutex::new(0u32));

    let mut server = DtlsOpenSsl::new("srv".to_string())
        .with_handle_message(Arc::new(make_counting_server_handler(Arc::clone(
            &hello_count,
        ))))
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(test_handlers::mock_peer_cert_handler);

    mux_srv.start().expect("server mux start");
    server.start(mux_srv.clone()).expect("server start");

    thread::sleep(Duration::from_millis(300));

    // --- client 1: first connection ---
    let client_port = test_util::find_unused_loopback_port();
    let client_addr = SocketAddr::new("127.0.0.1".parse().unwrap(), client_port);

    let mux_cli1 = Arc::new(UdpNetworkMux::bind(client_addr).expect("bind client mux 1"));
    let mut client1 = DtlsOpenSsl::new("cli1".to_string())
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(test_handlers::mock_peer_cert_handler);

    mux_cli1.start().expect("client mux 1 start");
    client1.start(mux_cli1.clone()).expect("client 1 start");

    thread::sleep(Duration::from_millis(200));

    let endpoint = NetworkEndpoint::new_direct(addr_srv);
    client1
        .send(&endpoint, b"Hello")
        .expect("first send failed");

    let start = Instant::now();
    while *hello_count.lock().unwrap() < 1 && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        *hello_count.lock().unwrap(),
        1,
        "server did not receive first Hello"
    );

    // --- simulate network outage: stop client 1 entirely ---
    tracing::info!("[test] stopping client 1 to simulate network outage");
    client1.stop().expect("client 1 stop");
    mux_cli1.stop();

    // Wait for port to be freed and server to detect the disconnection.
    thread::sleep(Duration::from_millis(500));

    // --- client 2: reconnect on same port (simulates changed outbound IP/port mapping) ---
    let mux_cli2 = Arc::new(UdpNetworkMux::bind(client_addr).expect("bind client mux 2"));
    let mut client2 = DtlsOpenSsl::new("cli2".to_string())
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(test_handlers::mock_peer_cert_handler);

    mux_cli2.start().expect("client mux 2 start");
    client2.start(mux_cli2.clone()).expect("client 2 start");

    thread::sleep(Duration::from_millis(200));

    tracing::info!("[test] sending second Hello after reconnect");
    client2
        .send(&endpoint, b"Hello")
        .expect("second send failed");

    let start = Instant::now();
    while *hello_count.lock().unwrap() < 2 && start.elapsed() < Duration::from_secs(10) {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        *hello_count.lock().unwrap(),
        2,
        "server did not receive second Hello after reconnect"
    );

    client2.stop().expect("client2 stop");
    mux_cli2.stop();
    server.stop().expect("server stop");
    mux_srv.stop();
}
