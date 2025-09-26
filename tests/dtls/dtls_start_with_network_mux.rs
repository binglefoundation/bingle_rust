#![cfg(not(target_os = "ios"))]

use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::{Duration, Instant};

use rust_comms::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};
mod pki;

static MESSAGE_SEEN: AtomicBool = AtomicBool::new(false);

fn handler(_server: &dyn Dtls, _from: &SocketAddr, _issuer: &str, data: &[u8]) {
    if !data.is_empty() {
        MESSAGE_SEEN.store(true, Ordering::Relaxed);
    }
}

#[test]
fn dtls_start_accepts_external_network_mux_udp() {
    // Generate test certs
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let client_cert_pem: Vec<u8> = certs.client_crt.clone();
    let client_key_pem: Vec<u8> = certs.client_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Choose a free UDP port for the DTLS server
    let probe = UdpSocket::bind(("127.0.0.1", 0)).expect("bind probe");
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let addr: SocketAddr = SocketAddr::from(([127, 0, 0, 1], port));

    // Create and bind a standalone NetworkMuxUdp (on its own ephemeral port) and pass it to start
    let mux = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    let mux: Arc<dyn rust_comms::dtls::NetworkMux + Send + Sync> = Arc::new(mux);

    // Build and configure the server
    let mut server = DtlsOpenSsl::new()
        .with_handle_message(std::sync::Arc::new(handler))
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

    // Start the server with the externally created mux
    server.start(addr, Some(mux)).expect("server start with external mux");
    thread::sleep(Duration::from_millis(100));

    // Build DTLS client and send a payload
    let client = DtlsOpenSsl::new()
        .with_client_cert(client_cert_pem.clone())
        .with_client_private_key(client_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

    let payload = b"hello-with-external-mux";
    let mut ok = false;
    for _ in 0..6 {
        if client.send(addr, payload).is_ok() { ok = true; break; }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(ok, "client DTLS send failed with external mux supplied");

    let start = Instant::now();
    while !MESSAGE_SEEN.load(Ordering::Relaxed) && start.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(MESSAGE_SEEN.load(Ordering::Relaxed), "server handler did not observe message");
}
