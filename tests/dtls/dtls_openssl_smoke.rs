#![cfg(not(target_os = "ios"))]

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rust_comms::dtls::{Dtls, DtlsOpenSsl};
mod pki;
#[path = "../test_util.rs"]
mod test_util;

fn mock_peer_cert_handler(_cert: &[u8], _ca: &[u8]) -> rust_comms::dtls::Result<String> {
    Ok(test_util::ADDRESS_SPEND.to_string())
}

static MESSAGE_SEEN: AtomicBool = AtomicBool::new(false);

fn handler(_server: &dyn Dtls, from: &SocketAddr, _issuer: &str, data: &[u8]) {
    // Record that the server received application data.
    if !data.is_empty() {
        let _ = from;
        MESSAGE_SEEN.store(true, Ordering::Relaxed);
    }
}

#[ntest::timeout(30_000)]
#[test]
fn dtls_openssl_udp_listener_invokes_handler() {
    #[allow(unused)]
    {
        rust_comms::util::printing::enable_immediate_prints();
    }

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
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    // Create and start a UDP mux for the server, and start the DTLS accept loop with it.
    let mux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    let mux = std::sync::Arc::new(mux0);
    let addr: SocketAddr = mux.local_addr().expect("mux addr");
    mux.start().expect("mux start");
    server.start(mux.clone()).expect("start should succeed");

    // Give the background thread a moment.
    thread::sleep(Duration::from_millis(50));

    // DTLS client: build and send a payload. Provide server creds as this instance starts an accept loop too.
    let certs_b = pki::generate_ed25519_test_certs();
    let mut client = DtlsOpenSsl::new()
        .with_client_cert(certs_b.client_crt.clone())
        .with_client_private_key(certs_b.client_key.clone())
        .with_server_signing_cert(certs_b.server_crt.clone())
        .with_server_signing_private_key(certs_b.server_key.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler)
        .with_ca_cert(certs_b.ca_crt.clone());

    // Start a client-side mux and initialize DTLS before sending
    let cmux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux");
    let cmux = std::sync::Arc::new(cmux0);
    cmux.start().expect("client mux start");
    client.start(cmux.clone()).expect("client start");

    let payload = b"hello-dtls";
    assert!(client.send(addr,  payload).is_ok(), "client DTLS send failed");

    // Spin-wait up to ~1 second for the handler to observe the message.
    let start = Instant::now();
    while !MESSAGE_SEEN.load(Ordering::Relaxed) {
        if start.elapsed() > Duration::from_secs(10) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        MESSAGE_SEEN.load(Ordering::Relaxed),
        "message handler was not invoked within the timeout"
    );
}
