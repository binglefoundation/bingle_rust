use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[path = "pki.rs"]
pub mod pki;
#[path = "../test_util.rs"]
pub mod test_util;

use bingle_core::api::bingle_api::NetworkEndpoint;
use bingle_core::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};
use test_util::init_test_logging;

const FAST_ENQUEUE_BUDGET: Duration = Duration::from_millis(500);

fn mock_peer_cert_handler(_cert: &[u8], _ca: &[u8]) -> bingle_core::dtls::Result<String> {
    Ok(test_util::ADDRESS_SPEND.to_string())
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
fn second_send_should_queue_without_waiting_for_stream_lock() {
    init_test_logging();

    let certs = pki::generate_ed25519_test_certs();
    let client = DtlsOpenSsl::new("client".to_string())
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

    let send_elapsed_samples = std::thread::scope(|scope| {
        let client_ref = &client;
        let first_started_for_thread = first_started.clone();
        let endpoint_for_thread = endpoint.clone();

        scope.spawn(move || {
            first_started_for_thread.store(true, Ordering::SeqCst);
            let _ = client_ref.send(&endpoint_for_thread, b"first-payload");
        });

        let wait_start = Instant::now();
        while !first_started.load(Ordering::SeqCst) && wait_start.elapsed() < Duration::from_secs(2)
        {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            first_started.load(Ordering::SeqCst),
            "first send thread did not start in time"
        );

        // Give the first send time to create/connect and enter the write loop.
        thread::sleep(Duration::from_millis(200));

        let mut elapsed_samples = Vec::new();
        for payload in [
            b"second-payload".as_slice(),
            b"third-payload",
            b"fourth-payload",
        ] {
            let send_start = Instant::now();
            let send_result = client.send(&endpoint, payload);
            let elapsed = send_start.elapsed();
            assert!(
                send_result.is_ok(),
                "send should be enqueued while connect is in progress"
            );
            elapsed_samples.push(elapsed);
        }

        elapsed_samples
    });

    assert_eq!(
        send_elapsed_samples.len(),
        3,
        "expected elapsed timing for all queued sends"
    );
    tracing::info!("send elapsed samples: {:?}", send_elapsed_samples);
    for (index, elapsed) in send_elapsed_samples.iter().enumerate() {
        assert!(
            *elapsed < FAST_ENQUEUE_BUDGET,
            "queued send {} waited too long (suggesting stream-lock wait): {:?}",
            index + 2,
            elapsed
        );
    }

    client.stop().expect("client stop");
    mux.stop();
}
