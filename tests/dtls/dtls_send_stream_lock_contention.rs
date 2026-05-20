use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[path = "../test_util.rs"]
pub mod test_util;
#[path = "pki.rs"]
pub mod pki;

use rust_comms::api::bingle_api::NetworkEndpoint;
use rust_comms::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};
use test_util::init_test_logging;

fn mock_peer_cert_handler(_cert: &[u8], _ca: &[u8]) -> rust_comms::dtls::Result<String> {
    Ok(test_util::ADDRESS_SPEND.to_string())
}

#[ntest::timeout(30_000)]
#[cfg_attr(not(target_os = "ios"), test)]
fn second_send_should_queue_without_waiting_for_stream_lock() {
    init_test_logging();

    let certs = pki::generate_ed25519_test_certs();
    let mut client = DtlsOpenSsl::new("client".to_string())
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(certs.ca_crt.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux"));
    mux.start().expect("client mux start");
    client.start(mux.clone()).expect("client start");

    // Reserve a local UDP port and immediately release it so there is no DTLS peer listening.
    // The first send enters handshake retry/timeout behavior.
    let socket = std::net::UdpSocket::bind(("127.0.0.1", 0)).expect("bind probe socket");
    let unreachable_addr = socket.local_addr().expect("probe socket local addr");
    drop(socket);

    let endpoint = NetworkEndpoint::new_direct(unreachable_addr);
    let first_started = Arc::new(AtomicBool::new(false));

    let second_send_elapsed = std::thread::scope(|scope| {
        let client_ref = &client;
        let first_started_for_thread = first_started.clone();
        let endpoint_for_thread = endpoint.clone();

        scope.spawn(move || {
            first_started_for_thread.store(true, Ordering::SeqCst);
            let _ = client_ref.send(&endpoint_for_thread, b"first-payload");
        });

        let wait_start = Instant::now();
        while !first_started.load(Ordering::SeqCst) && wait_start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(5));
        }

        // Give the first send time to create/connect and enter the write loop.
        thread::sleep(Duration::from_millis(200));

        let second_start = Instant::now();
        let second_send_result = client.send(&endpoint, b"second-payload");
        let elapsed = second_start.elapsed();

        assert!(second_send_result.is_ok(), "second send should be queued while connect is in progress");
        elapsed
    });

    // Regression guard: if stream lock is held for the whole handshake timeout,
    // this takes roughly 10s. We expect quick return even while another send is in progress.
    assert!(
        second_send_elapsed < Duration::from_secs(2),
        "second send waited too long for stream lock: {:?}",
        second_send_elapsed
    );

    client.stop().expect("client stop");
    mux.stop();
}
