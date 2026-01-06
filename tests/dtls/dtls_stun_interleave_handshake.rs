#![cfg(not(target_os = "ios"))]

use std::net::{SocketAddr, UdpSocket};
use std::sync::{atomic::{AtomicUsize, Ordering}, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use rust_comms::dtls::{Dtls, DtlsOpenSsl};
mod pki;

fn mock_peer_cert_handler(_cert: &[u8], _ca: &[u8]) -> rust_comms::dtls::Result<String> {
    Ok("MOCK-ISSUER".to_string())
}

// Record client-received echoes to verify ordering
static CLIENT_ECHO_COUNT: AtomicUsize = AtomicUsize::new(0);
static CLIENT_ECHOS: OnceLock<Mutex<Vec<Vec<u8>>>> = OnceLock::new();

fn client_handler(_client: &dyn Dtls, _from: &SocketAddr, _issuer: &str, data: &[u8]) {
    let m = CLIENT_ECHOS.get_or_init(|| Mutex::new(Vec::new()));
    m.lock().unwrap().push(data.to_vec());
    CLIENT_ECHO_COUNT.fetch_add(1, Ordering::Relaxed);
}

fn server_echo_handler(server: &dyn Dtls, from: &SocketAddr, _issuer: &str, data: &[u8]) {
    let _ = server.send(&rust_comms::api::bingle_api::NetworkSourceKey::new_direct(*from), data);
}

#[ntest::timeout(30_000)]
#[test]
fn stun_response_does_not_interfere_with_dtls_flow() {
    // Reset global state
    CLIENT_ECHO_COUNT.store(0, Ordering::Relaxed);
    if let Some(m) = CLIENT_ECHOS.get() { m.lock().unwrap().clear(); }

    // Generate test certs
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

    // Start the server
    let mut server = DtlsOpenSsl::new()
        .with_handle_message(std::sync::Arc::new(server_echo_handler))
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);
    mux.start().expect("mux start");
    server.start(mux.clone()).expect("server start");
    thread::sleep(Duration::from_millis(150));

    // Build two DTLS clients with a handler to record echoes
    let certs_b = pki::generate_ed25519_test_certs();
    let mut client1 = DtlsOpenSsl::new()
        .with_client_cert(certs_b.client_crt.clone())
        .with_client_private_key(certs_b.client_key.clone())
        .with_server_signing_cert(certs_b.server_crt.clone())
        .with_server_signing_private_key(certs_b.server_key.clone())
        .with_ca_cert(certs_b.ca_crt.clone())
        .with_handle_message(std::sync::Arc::new(client_handler))
        .with_handle_peer_certificate(mock_peer_cert_handler);

    let certs_c = pki::generate_ed25519_test_certs();
    let mut client2 = DtlsOpenSsl::new()
        .with_client_cert(certs_c.client_crt.clone())
        .with_client_private_key(certs_c.client_key.clone())
        .with_server_signing_cert(certs_c.server_crt.clone())
        .with_server_signing_private_key(certs_c.server_key.clone())
        .with_ca_cert(certs_c.ca_crt.clone())
        .with_handle_message(std::sync::Arc::new(client_handler))
        .with_handle_peer_certificate(mock_peer_cert_handler);

    // Start muxes and initialize both clients
    let cmux1_0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client1 mux");
    let cmux1 = std::sync::Arc::new(cmux1_0);
    cmux1.start().expect("client1 mux start");
    client1.start(cmux1.clone()).expect("client1 start");

    let cmux2_0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client2 mux");
    let cmux2 = std::sync::Arc::new(cmux2_0);
    cmux2.start().expect("client2 mux start");
    client2.start(cmux2.clone()).expect("client2 start");

    // 1) Send first DTLS message (this will perform handshake if needed)
    let payload1 = b"hello-dtls-1";
    let mut ok = false;
    for _ in 0..5 {
        if client1.send(&rust_comms::api::bingle_api::NetworkSourceKey::new_direct(addr), payload1).is_ok() { ok = true; break; }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(ok, "first DTLS send failed");

    // Wait for echo #1
    let start = Instant::now();
    while CLIENT_ECHO_COUNT.load(Ordering::Relaxed) < 1 && start.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(CLIENT_ECHO_COUNT.load(Ordering::Relaxed), 1, "did not receive first echo");

    // 2) Interleave a STUN response packet to the server (first byte in 0..=3 => STUN)
    let stun_socket = UdpSocket::bind(("127.0.0.1", 0)).expect("bind stun sender");
    // Minimal STUN-like payload: first byte 0, rest arbitrary
    let stun_payload = [0u8, 0x01, 0x02, 0x03];
    stun_socket.send_to(&stun_payload, addr).expect("send stun");

    // Small delay to ensure STUN packet arrives between the two DTLS messages
    thread::sleep(Duration::from_millis(20));

    // 3) Send second DTLS message using a fresh client (ensures a read of the echo)
    let payload2 = b"hello-dtls-2";
    let mut ok2 = false;
    for _ in 0..5 {
        if client2.send(&rust_comms::api::bingle_api::NetworkSourceKey::new_direct(addr), payload2).is_ok() { ok2 = true; break; }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(ok2, "second DTLS send failed");

    // Wait for echo #2
    let start2 = Instant::now();
    while CLIENT_ECHO_COUNT.load(Ordering::Relaxed) < 2 && start2.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(CLIENT_ECHO_COUNT.load(Ordering::Relaxed), 2, "did not receive second echo");

    // Verify echoes match the payloads in order
    let echos = CLIENT_ECHOS.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap().clone();
    assert_eq!(echos.len(), 2);
    assert_eq!(echos[0], payload1);
    assert_eq!(echos[1], payload2);
}
