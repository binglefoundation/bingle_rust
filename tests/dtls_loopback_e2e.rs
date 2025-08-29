#![cfg(not(target_os = "ios"))]

use std::net::{SocketAddr, UdpSocket};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use rust_comms::dtls::{Dtls, DtlsOpenSsl};
mod pki;

static SERVER_ECHOED: OnceLock<Vec<u8>> = OnceLock::new();
static CLIENT_ECHOED: OnceLock<Vec<u8>> = OnceLock::new();

fn client_handler(_server: &dyn Dtls, _from: &SocketAddr, data: &[u8]) {
    let _ = CLIENT_ECHOED.set(data.to_vec());
}

#[test]
fn dtls_openssl_end_to_end_loopback_echo() {
    use std::time::Instant;
    // Generate Ed25519 CA, server, and client credentials dynamically
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let client_cert_pem: Vec<u8> = certs.client_crt.clone();
    let client_key_pem: Vec<u8> = certs.client_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Choose a free UDP port by binding to 127.0.0.1:0 and taking the assigned port.
    let probe = UdpSocket::bind(("127.0.0.1", 0)).expect("bind probe");
    let port = probe.local_addr().unwrap().port();
    drop(probe); // free the port for the server

    let addr: SocketAddr = SocketAddr::from(([127, 0, 0, 1], port));

    // Echo handler: save payload then send back to sender using the server instance.
    fn echo_handler(server: &dyn Dtls, from: &SocketAddr, data: &[u8]) {
        // Ignore DTLS record-layer datagrams (Handshake=22, Application=23) that may arrive
        // when the server is in plaintext fallback mode; we only want to record real app payload.
        if let Some(first) = data.first() {
            if *first == 22 || *first == 23 {
                return;
            }
        }
        let _ = SERVER_ECHOED.set(data.to_vec());
        let _ = server.send(*from, data);
    }

    // Build and configure the server instance with echo_handler that echoes via server.send.
    let mut server = DtlsOpenSsl::new()
        .as_server()
        .with_handle_message(echo_handler)
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

    server.start_server(addr).expect("start_server should succeed");

    // Give the background thread a moment to bind.
    thread::sleep(Duration::from_millis(200));

    // DTLS client: build and send payload to server.
    let client = DtlsOpenSsl::new()
        .as_client()
        .with_handle_message(client_handler)
        .with_client_cert(client_cert_pem.clone())
        .with_client_private_key(client_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

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
    assert_eq!(echoed.as_slice(), payload, "client captured echoed payload mismatch");

    // Also ensure the server echo handler recorded the original payload.
    let start = Instant::now();
    while SERVER_ECHOED.get().is_none() && start.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(10));
    }
    let server_echoed = SERVER_ECHOED.get().expect("server did not record payload within timeout");
    assert_eq!(server_echoed.as_slice(), payload, "server recorded payload mismatch");
}
