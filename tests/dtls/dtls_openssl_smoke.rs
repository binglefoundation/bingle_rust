#![cfg(not(target_os = "ios"))]

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rust_comms::dtls::{Dtls, DtlsOpenSsl};
mod pki;

static MESSAGE_SEEN: AtomicBool = AtomicBool::new(false);

fn handler(_server: &dyn Dtls, from: &SocketAddr, _issuer: &str, data: &[u8]) {
    // Record that the server received application data.
    if !data.is_empty() {
        let _ = from;
        MESSAGE_SEEN.store(true, Ordering::Relaxed);
    }
}

#[test]
fn dtls_openssl_udp_listener_invokes_handler() {
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

    // Build and configure the server instance.
    let mut server = DtlsOpenSsl::new()
        .with_handle_message(std::sync::Arc::new(handler))
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

    // Start the DTLS accept loop.
    server.start(addr, None).expect("start should succeed");

    // Give the background thread a moment to bind.
    thread::sleep(Duration::from_millis(50));

    // DTLS client: build and send a payload.
    let client = DtlsOpenSsl::new()
        .with_client_cert(client_cert_pem.clone())
        .with_client_private_key(client_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

    let payload = b"hello-dtls";
    assert!(client.send(addr, "", payload).is_ok(), "client DTLS send failed");

    // Spin-wait up to ~1 second for the handler to observe the message.
    let start = Instant::now();
    while !MESSAGE_SEEN.load(Ordering::Relaxed) {
        if start.elapsed() > Duration::from_secs(1) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        MESSAGE_SEEN.load(Ordering::Relaxed),
        "message handler was not invoked within the timeout"
    );
}
