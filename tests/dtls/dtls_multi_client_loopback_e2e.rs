#![cfg(not(target_os = "ios"))]

use std::net::SocketAddr;
use std::sync::{OnceLock, Mutex};
use std::thread;
use std::time::Duration;

use rust_comms::dtls::{Dtls, DtlsOpenSsl};
#[path = "pki.rs"]
mod pki;

fn mock_peer_cert_handler(_cert: &[u8], _ca: &[u8]) -> rust_comms::dtls::Result<String> {
    Ok("MOCK-ISSUER".to_string())
}

static CLIENT1_ECHOED: OnceLock<Vec<u8>> = OnceLock::new();
static CLIENT2_ECHOED: OnceLock<Vec<u8>> = OnceLock::new();
static SERVER_SEEN: OnceLock<Mutex<Vec<Vec<u8>>>> = OnceLock::new();

fn client1_handler(_server: &dyn Dtls, _from: &SocketAddr, _issuer: &str, data: &[u8]) {
    let _ = CLIENT1_ECHOED.set(data.to_vec());
}

fn client2_handler(_server: &dyn Dtls, _from: &SocketAddr, _issuer: &str, data: &[u8]) {
    let _ = CLIENT2_ECHOED.set(data.to_vec());
}

#[ntest::timeout(30_000)]
#[test]
fn dtls_openssl_multi_client_loopback_echo() {
    use std::time::Instant;

    // Generate Ed25519 CA, server, and client credentials dynamically
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let _client_cert_pem: Vec<u8> = certs.client_crt.clone();
    let _client_key_pem: Vec<u8> = certs.client_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Create and start a UDP mux for the server and determine its bound address.
    let mux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    let mux = std::sync::Arc::new(mux0);
    let addr: SocketAddr = mux.local_addr().expect("mux addr");

    // Echo handler: save payload (when it looks like application data) then send back with prefix using the server instance.
    fn echo_handler(server: &dyn Dtls, from: &SocketAddr, _issuer: &str, data: &[u8]) {
        if let Some(first) = data.first() {
            // Ignore DTLS record-layer (Handshake=22, Application=23) ciphertext bytes
            if *first == 22 || *first == 23 {
                return;
            }
        }
        let store = SERVER_SEEN.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut v) = store.lock() { v.push(data.to_vec()); }
        let mut echoed = b"ECHOED: ".to_vec();
        echoed.extend_from_slice(data);
        let _ = server.send(&rust_comms::api::bingle_api::NetworkSourceKey::new_direct(*from), &echoed);
    }

    // Build and configure the server instance with echo_handler that echoes via server.send.
    let mut server = DtlsOpenSsl::new()
        .with_handle_message(std::sync::Arc::new(echo_handler))
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    // Start mux then the DTLS server with the mux
    mux.start().expect("mux start");
    server.start(mux.clone()).expect("start should succeed");

    // Give the background thread a moment.
    thread::sleep(Duration::from_millis(200));

    // DTLS clients: build and send payloads to server.
    let certs_b = pki::generate_ed25519_test_certs();
    let mut client1 = DtlsOpenSsl::new()
        .with_handle_message(std::sync::Arc::new(client1_handler))
        .with_client_cert(certs_b.client_crt.clone())
        .with_client_private_key(certs_b.client_key.clone())
        .with_server_signing_cert(certs_b.server_crt.clone())
        .with_server_signing_private_key(certs_b.server_key.clone())
        .with_ca_cert(certs_b.ca_crt.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    let certs_c = pki::generate_ed25519_test_certs();
    let mut client2 = DtlsOpenSsl::new()
        .with_handle_message(std::sync::Arc::new(client2_handler))
        .with_client_cert(certs_c.client_crt.clone())
        .with_client_private_key(certs_c.client_key.clone())
        .with_server_signing_cert(certs_c.server_crt.clone())
        .with_server_signing_private_key(certs_c.server_key.clone())
        .with_ca_cert(certs_c.ca_crt.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    // Start separate muxes for each client and initialize DTLS
    let cmux1_0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client1 mux");
    let cmux1 = std::sync::Arc::new(cmux1_0);
    cmux1.start().expect("client1 mux start");
    client1.start(cmux1.clone()).expect("client1 start");

    let cmux2_0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client2 mux");
    let cmux2 = std::sync::Arc::new(cmux2_0);
    cmux2.start().expect("client2 mux start");
    client2.start(cmux2.clone()).expect("client2 start");

    let payload1 = b"multi-client-msg-1";
    let payload2 = b"multi-client-msg-2";

    // Send from client1 with small retry loop.
    let mut ok1 = false;
    for _ in 0..8 {
        if client1.send(&rust_comms::api::bingle_api::NetworkSourceKey::new_direct(addr),  payload1).is_ok() { ok1 = true; break; }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(ok1, "client1 DTLS send failed");

    // Wait for client1 to capture its echoed payload before proceeding with client2.
    let start = Instant::now();
    while CLIENT1_ECHOED.get().is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }

    // Brief pause to allow the server accept loop to return to listening.
    thread::sleep(Duration::from_millis(400));

    // Now send from client2 with a larger retry window to allow server to cycle back.
    let mut ok2 = false;
    for _ in 0..20 {
        if client2.send(&rust_comms::api::bingle_api::NetworkSourceKey::new_direct(addr), payload2).is_ok() { ok2 = true; break; }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(ok2, "client2 DTLS send failed");

    // Wait for client2 to capture its echoed payload.
    let start = Instant::now();
    while CLIENT2_ECHOED.get().is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }

    // Validate client1 echo
    let echoed1 = CLIENT1_ECHOED.get().expect("client1 did not capture echoed payload within timeout");
    let mut expected1 = b"ECHOED: ".to_vec();
    expected1.extend_from_slice(payload1);
    assert_eq!(echoed1.as_slice(), expected1.as_slice(), "client1 echoed payload mismatch with prefix");

    // Validate client2 echo
    let echoed2 = CLIENT2_ECHOED.get().expect("client2 did not capture echoed payload within timeout");
    let mut expected2 = b"ECHOED: ".to_vec();
    expected2.extend_from_slice(payload2);
    assert_eq!(echoed2.as_slice(), expected2.as_slice(), "client2 echoed payload mismatch with prefix");

    // Optionally, validate server observed both original payloads.
    if let Some(mtx) = SERVER_SEEN.get() {
        if let Ok(list) = mtx.lock() {
            // The order is not guaranteed; check set membership.
            let have1 = list.iter().any(|v| v.as_slice() == payload1);
            let have2 = list.iter().any(|v| v.as_slice() == payload2);
            assert!(have1 && have2, "server did not observe both client payloads");
        }
    }
}
