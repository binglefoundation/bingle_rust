
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[path = "../test_util.rs"]
pub mod test_util;
pub mod pki;
pub mod test_handlers;

use test_util::init_test_logging;
use rust_comms::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};
use test_handlers::*;

// Clearable storage for validation
static SERVER_HELLO_2: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static SERVER_CLIENT_ECHOED_2: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static CLIENT_PING_SEEN_2: Mutex<Option<Vec<u8>>> = Mutex::new(None);

fn server_handler_2(server: &dyn Dtls, from: &rust_comms::api::bingle_api::NetworkEndpoint, _issuer: &str, data: &[u8]) {
    tracing::info!("server_handler_2: {:?}", data);
    if data == b"Hello" {
        let mut g = SERVER_HELLO_2.lock().unwrap();
        *g = Some(data.to_vec());
        let _ = server.send(from, b"Ping");
        return;
    }
    if data.starts_with(b"CLIENT ECHOED: ") {
        let mut g = SERVER_CLIENT_ECHOED_2.lock().unwrap();
        *g = Some(data.to_vec());
    }
}

fn client_handler_2(server: &dyn Dtls, from: &rust_comms::api::bingle_api::NetworkEndpoint, _issuer: &str, data: &[u8]) {
    tracing::info!("client_handler_2: {:?}", data);
    if data == b"Ping" {
        let mut g = CLIENT_PING_SEEN_2.lock().unwrap();
        *g = Some(data.to_vec());
    }
    let mut echoed = b"CLIENT ECHOED: ".to_vec();
    echoed.extend_from_slice(data);
    let _ = server.send(from, &echoed);
}

#[ntest::timeout(60_000)]
#[cfg_attr(not(target_os = "ios"), test)]
pub fn dtls_client_reconnect() {
    init_test_logging();

    // Generate credentials
    let certs = pki::generate_ed25519_test_certs();
    let ca_pem = certs.ca_crt.clone();

    // 1. Setup Server
    let mux_srv = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind server mux"));
    let addr_srv = mux_srv.local_addr().expect("server addr");

    let mut server = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_handle_message(Arc::new(server_handler_2))
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    mux_srv.start().expect("server mux start");
    server.start(mux_srv.clone()).expect("server start");

    thread::sleep(Duration::from_millis(400));

    // 2. Setup Client 1
    let client_mux_port = test_util::find_unused_loopback_port();
    let client_addr = SocketAddr::new("127.0.0.1".parse().unwrap(), client_mux_port);

    tracing::info!("Starting client 1 on addr:port {} server on {}", client_addr, mux_srv.local_addr().unwrap());

    let mux_cli1 = Arc::new(UdpNetworkMux::bind(client_addr).expect("bind client 1 mux"));
    let mut client1 = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_handle_message(Arc::new(client_handler_2))
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    mux_cli1.start().expect("client 1 mux start");
    client1.start(mux_cli1.clone()).expect("client 1 start");

    thread::sleep(Duration::from_millis(250));

    // Roundtrip 1
    do_roundtrip(&client1, addr_srv, "Round 1");

    // 3. Stop Client 1
    tracing::info!("Stopping client 1...");
    mux_cli1.stop();
    // Wait for port to be freed
    thread::sleep(Duration::from_millis(500));

    // 4. Build another DTLS client and mux on same address and port
    tracing::info!("Starting client 2 on same addr:port {}...", client_addr);
    
    // Clear validation storage for round 2
    {
        *SERVER_HELLO_2.lock().unwrap() = None;
        *SERVER_CLIENT_ECHOED_2.lock().unwrap() = None;
        *CLIENT_PING_SEEN_2.lock().unwrap() = None;
    }

    let mux_cli2 = Arc::new(UdpNetworkMux::bind(client_addr).expect("bind client 2 mux"));
    let mut client2 = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_handle_message(Arc::new(client_handler_2))
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    mux_cli2.start().expect("client 2 mux start");
    client2.start(mux_cli2.clone()).expect("client 2 start");

    thread::sleep(Duration::from_millis(250));

    // Roundtrip 2
    do_roundtrip(&client2, addr_srv, "Round 2");
}

fn do_roundtrip(client: &dyn Dtls, server_addr: SocketAddr, label: &str) {
    let endpoint = rust_comms::api::bingle_api::NetworkEndpoint::new_direct(server_addr);
    let mut ok = false;
    for _ in 0..1 {
        if client.send(&endpoint, b"Hello").is_ok() { ok = true; break; }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(ok, "{} client DTLS send of 'Hello' failed", label);

    // Validate server received Hello
    let start = Instant::now();
    while SERVER_HELLO_2.lock().unwrap().is_none() && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(SERVER_HELLO_2.lock().unwrap().is_some(), "{} server did not receive 'Hello'", label);

    // Validate client received Ping
    let start = Instant::now();
    while CLIENT_PING_SEEN_2.lock().unwrap().is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(CLIENT_PING_SEEN_2.lock().unwrap().is_some(), "{} client did not receive 'Ping'", label);

    // Validate server received client's echoed message
    let start = Instant::now();
    while SERVER_CLIENT_ECHOED_2.lock().unwrap().is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    let echoed = SERVER_CLIENT_ECHOED_2.lock().unwrap();
    let echoed_vec = echoed.as_ref().expect(&format!("{} server did not receive client's echo", label));
    assert_eq!(echoed_vec.as_slice(), b"CLIENT ECHOED: Ping", "{} server received wrong echo", label);
}
