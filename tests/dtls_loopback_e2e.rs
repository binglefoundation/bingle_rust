#![cfg(not(target_os = "ios"))]

use std::net::{SocketAddr, UdpSocket};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use rust_comms::dtls::{Dtls, DtlsOpenSsl};

static ECHOED: OnceLock<Vec<u8>> = OnceLock::new();

fn client_handler(_from: &SocketAddr, data: &[u8]) {
    let _ = ECHOED.set(data.to_vec());
}

#[test]
fn dtls_openssl_end_to_end_loopback_echo() {
    // Load test PEM materials (self-signed server cert as both CA and server cert).
    let cert_pem: Vec<u8> = include_bytes!("../dtls_test/server.crt").to_vec();
    let key_pem: Vec<u8> = include_bytes!("../dtls_test/server.key").to_vec();

    // Choose a free UDP port by binding to 127.0.0.1:0 and taking the assigned port.
    let probe = UdpSocket::bind(("127.0.0.1", 0)).expect("bind probe");
    let port = probe.local_addr().unwrap().port();
    drop(probe); // free the port for the server

    let addr: SocketAddr = SocketAddr::from(([127, 0, 0, 1], port));

    // Build and configure the server instance (no-op handler; echo occurs over DTLS stream).
    let mut server = DtlsOpenSsl::new()
        .as_server()
        .with_handle_message(|_, _| {})
        .with_server_signing_cert(cert_pem.clone())
        .with_server_signing_private_key(key_pem.clone())
        .with_ca_cert(cert_pem.clone());

    // Start the server; it will attempt DTLS first and fall back to UDP if necessary.
    server.start_server(addr).expect("start_server should succeed");

    // Give the background thread a moment to bind.
    thread::sleep(Duration::from_millis(50));

    // Client: build DtlsOpenSsl, send payload via DTLS, and capture echo via client handler.
    let client = DtlsOpenSsl::new()
        .as_client()
        .with_handle_message(client_handler)
        .with_client_cert(cert_pem.clone())
        .with_client_private_key(key_pem.clone())
        .with_ca_cert(cert_pem.clone());

    let payload = b"loopback-echo-payload";
    assert!(client.send(addr, payload).is_ok(), "client DTLS send failed");

    // Verify echoed data was captured by client handler.
    let echoed = ECHOED.get().expect("did not receive echo via client handler");
    assert_eq!(echoed.as_slice(), payload, "echoed payload mismatch");
}
