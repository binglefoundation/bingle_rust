#![cfg(not(target_os = "ios"))]

use std::net::{SocketAddr, UdpSocket};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use rust_comms::dtls::{Dtls, DtlsOpenSsl};

static ECHOED: OnceLock<Vec<u8>> = OnceLock::new();
static SERVER_ECHOED: OnceLock<Vec<u8>> = OnceLock::new();

fn handle_peer_certificate(_cert: &[u8], _ca_cert: &[u8]) -> rust_comms::dtls::Result<String> {
    Ok("issuer".to_string())
}

fn client_handler(_from: &SocketAddr, data: &[u8]) {
    let _ = ECHOED.set(data.to_vec());
}

#[test]
fn dtls_openssl_end_to_end_loopback_echo() {
    use std::time::Instant;
    // Load server PEM materials for the server (CA = server cert in test env)
    let server_cert_pem: Vec<u8> = include_bytes!("../dtls_test/server.crt").to_vec();
    let server_key_pem: Vec<u8> = include_bytes!("../dtls_test/server.key").to_vec();
    // Load separate client PEM materials for the client
    let client_cert_pem: Vec<u8> = include_bytes!("../dtls_test/client.crt").to_vec();
    let client_key_pem: Vec<u8> = include_bytes!("../dtls_test/client.key").to_vec();

    // Choose a free UDP port by binding to 127.0.0.1:0 and taking the assigned port.
    let probe = UdpSocket::bind(("127.0.0.1", 0)).expect("bind probe");
    let port = probe.local_addr().unwrap().port();
    drop(probe); // free the port for the server

    let addr: SocketAddr = SocketAddr::from(([127, 0, 0, 1], port));

    // Echo handler: record the payload observed by the server for validation.
    fn echo_handler(_from: &SocketAddr, data: &[u8]) {
        // TODO: need to send the data back to from using the calling `server` instance.
        let _ = SERVER_ECHOED.set(data.to_vec());
    }

    // Build and configure the server instance (no-op handler; DTLS echo occurs in accept loop).
    let mut server = DtlsOpenSsl::new()
        .as_server()
        .with_handle_message(echo_handler)
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(server_cert_pem.clone());

    // Start the server; it will attempt DTLS first and fall back to UDP if necessary.
    server.start_server(addr).expect("start_server should succeed");

    // Give the background thread a moment to bind.
    thread::sleep(Duration::from_millis(50));

    // Client: build DtlsOpenSsl, send payload via DTLS, and capture echo via client handler.
    let client = DtlsOpenSsl::new()
        .as_client()
        .with_handle_message(client_handler)
        .with_handle_peer_certificate(handle_peer_certificate)
        .with_client_cert(client_cert_pem.clone())
        .with_client_private_key(client_key_pem.clone())
        .with_ca_cert(client_cert_pem.clone());

    let payload = b"loopback-echo-payload";
    assert!(client.send(addr, payload).is_ok(), "client DTLS send failed");

    // Wait for server to record the payload via echo_handler.
    let start = Instant::now();
    while SERVER_ECHOED.get().is_none() && start.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(10));
    }
    let echoed = SERVER_ECHOED.get().expect("server did not record payload within timeout");
    assert_eq!(echoed.as_slice(), payload, "server recorded payload mismatch");

}
