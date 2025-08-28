#![cfg(not(target_os = "ios"))]

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rust_comms::dtls::{Dtls, DtlsOpenSsl};

static MESSAGE_SEEN: AtomicBool = AtomicBool::new(false);

fn handler(from: &SocketAddr, data: &[u8]) {
    // Basic sanity assertions in the handler context are risky; just record receipt.
    if !data.is_empty() {
        let _ = from; // suppress unused warning
        MESSAGE_SEEN.store(true, Ordering::Relaxed);
    }
}

#[test]
fn dtls_openssl_udp_listener_invokes_handler() {
    // Load test PEM materials (self-signed server cert as both CA and server cert).
    let server_cert_pem: Vec<u8> = include_bytes!("../dtls_test/server.crt").to_vec();
    let server_key_pem: Vec<u8> = include_bytes!("../dtls_test/server.key").to_vec();

    // Choose a free UDP port by binding to 127.0.0.1:0 and taking the assigned port.
    let probe = UdpSocket::bind(("127.0.0.1", 0)).expect("bind probe");
    let port = probe.local_addr().unwrap().port();
    drop(probe); // free the port for the server

    let addr: SocketAddr = SocketAddr::from(([127, 0, 0, 1], port));

    // Build and configure the server instance.
    let mut server = DtlsOpenSsl::new()
        .as_server()
        .with_handle_message(handler)
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(server_cert_pem.clone());

    // Start the temporary UDP listener (scaffold) in the implementation.
    server.start_server(addr).expect("start_server should succeed");

    // Give the background thread a moment to bind.
    thread::sleep(Duration::from_millis(50));

    // Send a UDP datagram to the server address.
    let client = UdpSocket::bind(("127.0.0.1", 0)).expect("bind client");
    let payload = b"hello-dtls";
    client
        .send_to(payload, addr)
        .expect("send_to should succeed");

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
