#![cfg(not(target_os = "ios"))]

use std::net::SocketAddr;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use rust_comms::dtls::{Dtls, DtlsOpenSsl, Result as DtlsResult};
mod pki;

// Storage for captured peer certificates and CA bytes
static SERVER_CERTS_SEEN: OnceLock<Mutex<Vec<Vec<u8>>>> = OnceLock::new();
static CLIENT_CERTS_SEEN: OnceLock<Mutex<Vec<Vec<u8>>>> = OnceLock::new();
static SERVER_CA_SEEN: OnceLock<Vec<u8>> = OnceLock::new();
static CLIENT_CA_SEEN: OnceLock<Vec<u8>> = OnceLock::new();
static CLIENT_ECHOED: OnceLock<Vec<u8>> = OnceLock::new();

fn normalize_pem_body(pem_bytes: &[u8]) -> String {
    // Extract only base64 body between BEGIN/END lines and strip all whitespace
    let s = String::from_utf8_lossy(pem_bytes);
    let mut in_body = false;
    let mut body = String::new();
    for line in s.lines() {
        if line.starts_with("-----BEGIN ") { in_body = true; continue; }
        if line.starts_with("-----END ") { break; }
        if in_body {
            for ch in line.chars() {
                if !ch.is_whitespace() { body.push(ch); }
            }
        }
    }
    body
}

fn server_peer_cert_handler(cert_pem: &[u8], ca_pem: &[u8]) -> DtlsResult<String> {
    let store = SERVER_CERTS_SEEN.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut v) = store.lock() {
        v.push(cert_pem.to_vec());
    }
    let _ = SERVER_CA_SEEN.set(ca_pem.to_vec());
    Ok("server-verified".to_string())
}

fn client_peer_cert_handler(cert_pem: &[u8], ca_pem: &[u8]) -> DtlsResult<String> {
    let store = CLIENT_CERTS_SEEN.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut v) = store.lock() {
        v.push(cert_pem.to_vec());
    }
    let _ = CLIENT_CA_SEEN.set(ca_pem.to_vec());
    Ok("client-verified".to_string())
}

fn echo_handler(server: &dyn Dtls, from: &rust_comms::api::bingle_api::NetworkEndpoint, _issuer: &str, data: &[u8]) {
    // Ignore DTLS record-layer types if any (not expected for application handler)
    if let Some(first) = data.first() { if *first == 22 || *first == 23 { return; } }
    let mut echoed = b"ECHOED: ".to_vec();
    echoed.extend_from_slice(data);
    let _ = server.send(from, &echoed);
}

fn client_handler(_server: &dyn Dtls, _from: &rust_comms::api::bingle_api::NetworkEndpoint, _issuer: &str, data: &[u8]) {
    let _ = CLIENT_ECHOED.set(data.to_vec());
}

#[ntest::timeout(30_000)]
#[test]
fn dtls_openssl_peer_certificate_handlers_are_invoked() {
    // Generate test certificates
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let _client_cert_pem: Vec<u8> = certs.client_crt.clone();
    let _client_key_pem: Vec<u8> = certs.client_key.clone();
    let server_ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Create and start a UDP mux for the server and determine its bound address.
    let mux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    let mux = std::sync::Arc::new(mux0);
    let addr: SocketAddr = mux.local_addr().expect("mux addr");

    // Start server with peer certificate handler and echo message handler
    let mut server = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_handle_peer_certificate(server_peer_cert_handler)
        .with_handle_message(std::sync::Arc::new(echo_handler))
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(server_ca_pem.clone());

    mux.start().expect("mux start");
    server.start(mux.clone()).expect("server start");
    thread::sleep(Duration::from_millis(200));

    // Build client with peer certificate handler; also provide server credentials for its accept loop
    let certs_b = pki::generate_ed25519_test_certs();
    let client_ca_pem = certs_b.ca_crt.clone();
    let mut client = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_handle_peer_certificate(client_peer_cert_handler)
        .with_handle_message(std::sync::Arc::new(client_handler))
        .with_client_cert(certs_b.client_crt.clone())
        .with_client_private_key(certs_b.client_key.clone())
        .with_server_signing_cert(certs_b.server_crt.clone())
        .with_server_signing_private_key(certs_b.server_key.clone())
        .with_ca_cert(certs_b.ca_crt.clone());

    // Start client mux and DTLS
    let cmux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux");
    let cmux = std::sync::Arc::new(cmux0);
    cmux.start().expect("client mux start");
    client.start(cmux.clone()).expect("client start");

    // Send a payload to drive handshake and application data
    let payload = b"peer-cert-test";
    let mut ok = false;
    for _ in 0..8 {
        if client.send(&rust_comms::api::bingle_api::NetworkEndpoint::new_direct(addr), payload).is_ok() { ok = true; break; }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(ok, "client DTLS send failed");

    // Wait for client to receive the echoed payload (ensures handshake completed)
    let start = Instant::now();
    while CLIENT_ECHOED.get().is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    let echoed = CLIENT_ECHOED.get().expect("client did not capture echoed payload");
    let mut expected = b"ECHOED: ".to_vec();
    expected.extend_from_slice(payload);
    assert_eq!(echoed.as_slice(), expected.as_slice(), "echo mismatch");

    // Validate that server saw the client's certificate at least once.
    // Compare against the actual client certificate used by the client instance
    // (certs_b.client_crt), not the unrelated client_cert_pem generated earlier.
    let client_norm = normalize_pem_body(&certs_b.client_crt);
    let server_saw_client = SERVER_CERTS_SEEN
        .get()
        .and_then(|m| m.lock().ok())
        .map(|v| v.iter().any(|pem| normalize_pem_body(pem) == client_norm))
        .unwrap_or(false);
    assert!(server_saw_client, "server did not observe client's certificate in handler");

    // Validate that client saw the server's certificate at least once
    let server_norm = normalize_pem_body(&server_cert_pem);
    let client_saw_server = CLIENT_CERTS_SEEN
        .get()
        .and_then(|m| m.lock().ok())
        .map(|v| v.iter().any(|pem| normalize_pem_body(pem) == server_norm))
        .unwrap_or(false);
    assert!(client_saw_server, "client did not observe server's certificate in handler");

    // Validate CA bytes passed to handlers match the configured CA PEM (normalize to account for formatting)
    if let Some(ca) = SERVER_CA_SEEN.get() {
        assert_eq!(normalize_pem_body(ca.as_slice()), normalize_pem_body(&client_ca_pem), "server CA bytes mismatch");
    }
    if let Some(ca) = CLIENT_CA_SEEN.get() {
        assert_eq!(normalize_pem_body(ca.as_slice()), normalize_pem_body(&server_ca_pem), "client CA bytes mismatch");
    }
}
