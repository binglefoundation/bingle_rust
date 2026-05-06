

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
#[path = "../test_util.rs"]
pub mod test_util;
pub mod pki;
use test_util::init_test_logging;
use rust_comms::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};

fn mock_peer_cert_handler(_cert: &[u8], _ca: &[u8]) -> rust_comms::dtls::Result<String> {
    Ok(test_util::ADDRESS_SPEND.to_string())
}

static MSG_COUNT: AtomicUsize = AtomicUsize::new(0);

fn reset_test_state() {
    MSG_COUNT.store(0, Ordering::Relaxed);
}

fn server_handler(_server: &dyn Dtls, _from: &rust_comms::api::bingle_api::NetworkEndpoint, _issuer: &str, data: &[u8]) {
    if !data.is_empty() {
        MSG_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

#[ntest::timeout(30_000)]
#[cfg_attr(not(target_os = "ios"), test)]
pub fn dtls_client_keeps_stream_open_across_sends() {
    init_test_logging();

    reset_test_state();
    #[allow(unused)]
    {
        rust_comms::util::printing::enable_immediate_prints();
    }

    // Generate credentials
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Start server DTLS with a live mux
    let mut server = DtlsOpenSsl::new("server".to_string())
        .with_handle_message(Arc::new(server_handler))
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    let mux0 = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind server mux");
    let mux = Arc::new(mux0);
    let server_addr = mux.local_addr().expect("server addr");
    mux.start().expect("server mux start");
    server.start(mux.clone()).expect("server start");

    // Start client DTLS with its own mux
    let certs_b = pki::generate_ed25519_test_certs();
    let mut client = DtlsOpenSsl::new("client".to_string())
        .with_client_cert(certs_b.client_crt.clone())
        .with_client_private_key(certs_b.client_key.clone())
        .with_server_signing_cert(certs_b.server_crt.clone())
        .with_server_signing_private_key(certs_b.server_key.clone())
        .with_ca_cert(certs_b.ca_crt.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    let cmux0 = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux");
    let cmux = Arc::new(cmux0);
    cmux.start().expect("client mux start");
    client.start(cmux.clone()).expect("client start");

    // First send (triggers handshake)
    let payload1 = b"first-message";
    assert!(client.send(&rust_comms::api::bingle_api::NetworkEndpoint::new_direct(server_addr), payload1).is_ok(), "first send failed");

    // Wait until server receives first message
    let start = Instant::now();
    while MSG_COUNT.load(Ordering::Relaxed) < 1 && start.elapsed() < Duration::from_secs(10) {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(MSG_COUNT.load(Ordering::Relaxed), 1, "server did not receive first message");
    tracing::info!("[Test] Server received first message, proceeding with second send");

    // Second send should reuse the same client DTLS stream; the stream must remain open across send()
    let payload2 = b"second-message";
    assert!(client.send(&rust_comms::api::bingle_api::NetworkEndpoint::new_direct(server_addr), payload2).is_ok(), "second send failed (stream may have been closed)");
    tracing::info!("[Test] Client sent second message");
    // Wait for second message
    let start2 = Instant::now();
    while MSG_COUNT.load(Ordering::Relaxed) < 2 && start2.elapsed() < Duration::from_secs(10) {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(MSG_COUNT.load(Ordering::Relaxed), 2, "server did not receive second message; client stream might not be kept open");
    tracing::info!("[Test] Server received second message");

    client.stop().expect("client stop");
    server.stop().expect("server stop");
    cmux.stop();
    mux.stop();
}