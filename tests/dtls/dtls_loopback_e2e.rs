#![cfg(not(target_os = "ios"))]

use std::net::SocketAddr;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use rust_comms::dtls::{Dtls, DtlsOpenSsl};
mod pki;

fn mock_peer_cert_handler(_cert: &[u8], _ca: &[u8]) -> rust_comms::dtls::Result<String> {
    Ok("MOCK-ISSUER".to_string())
}

static SERVER_ECHOED: OnceLock<Vec<u8>> = OnceLock::new();
static CLIENT_ECHOED: OnceLock<Vec<u8>> = OnceLock::new();

fn client_handler(_server: &dyn Dtls, _from: &SocketAddr, _issuer: &str, data: &[u8]) {
    let _ = CLIENT_ECHOED.set(data.to_vec());
}


#[ntest::timeout(30_000)]
#[test]
fn dtls_openssl_end_to_end_loopback_echo() {
    use std::time::Instant;
    // Generate Ed25519 CA, server, and client credentials dynamically
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Create and start a UDP mux for the server and determine its bound address.
    let mux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    let mux = std::sync::Arc::new(mux0);
    let addr: SocketAddr = mux.local_addr().expect("mux addr");

    // Echo handler: save payload then send back to sender using the server instance.
    fn echo_handler(server: &dyn Dtls, from: &SocketAddr, _issuer: &str, data: &[u8]) {
        // Ignore DTLS record-layer datagrams (Handshake=22, Application=23) that may arrive
        // when the server is in plaintext fallback mode; we only want to record real app payload.
        if let Some(first) = data.first() {
            if *first == 22 || *first == 23 {
                return;
            }
        }
        let _ = SERVER_ECHOED.set(data.to_vec());
        let mut echoed = b"ECHOED: ".to_vec();
        echoed.extend_from_slice(data);
        let _ = server.send(*from,  &echoed);
    }

    // Build and configure the server instance with echo_handler that echoes via server.send.
    let mut server = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_handle_message(std::sync::Arc::new(echo_handler))
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    // Start mux then the DTLS server with the mux.
    mux.start().expect("mux start");
    server.start(mux.clone()).expect("start should succeed");

    // Give the background thread a moment.
    thread::sleep(Duration::from_millis(200));

    // DTLS client: build and send payload to server. Provide server creds for its accept loop.
    let certs_b = pki::generate_ed25519_test_certs();
    let mut client = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_handle_message(std::sync::Arc::new(client_handler))
        .with_client_cert(certs_b.client_crt.clone())
        .with_client_private_key(certs_b.client_key.clone())
        .with_server_signing_cert(certs_b.server_crt.clone())
        .with_server_signing_private_key(certs_b.server_key.clone())
        .with_ca_cert(certs_b.ca_crt.clone())
                .with_handle_peer_certificate(mock_peer_cert_handler);

    // Start client mux and initialize client DTLS
    let cmux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux");
    let cmux = std::sync::Arc::new(cmux0);
    cmux.start().expect("client mux start");
    client.start(cmux.clone()).expect("client start");

    let payload = b"loopback-echo-payload";
    // Retry a few times in case of transient handshake timing.
    let mut ok = false;
    for _ in 0..5 {
        if client.send(addr, payload).is_ok() { ok = true; break; }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(ok, "client DTLS send failed");

    // Wait for the client handler to capture the echoed payload.
    let start = Instant::now();
    while CLIENT_ECHOED.get().is_none() && start.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(10));
    }
    let echoed = CLIENT_ECHOED.get().expect("client did not capture echoed payload within timeout");
    let mut expected = b"ECHOED: ".to_vec();
    expected.extend_from_slice(payload);
    assert_eq!(echoed.as_slice(), expected.as_slice(), "client captured echoed payload mismatch with prefix");

    // Also ensure the server echo handler recorded the original payload.
    let start = Instant::now();
    while SERVER_ECHOED.get().is_none() && start.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(10));
    }
    let server_echoed = SERVER_ECHOED.get().expect("server did not record payload within timeout");
    assert_eq!(server_echoed.as_slice(), payload, "server recorded payload mismatch");
}
