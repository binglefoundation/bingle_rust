use std::sync::Arc;

#[path = "pki.rs"]
pub mod pki;
#[path = "test_handlers.rs"]
pub mod test_handlers;
#[path = "../test_util.rs"]
pub mod test_util;

use rust_comms::api::bingle_api::NetworkEndpoint;
use rust_comms::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};
use test_handlers::*;
use test_util::init_test_logging;

#[test]
fn dtls_handshake_failure_retry() {
    init_test_logging();

    // 1. Generate credentials
    let certs = pki::generate_ed25519_test_certs();
    let ca_pem = certs.ca_crt.clone();

    // 2. Setup Server (but don't start it yet)
    let mux_srv = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind server mux"));
    let addr_srv = mux_srv.local_addr().expect("server addr");

    let mut server = DtlsOpenSsl::new("server".to_string())
        .with_null_encryption()
        .with_handle_message(Arc::new(|_, _, _, _| {}))
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    // 3. Setup Client
    let mux_cli = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux"));
    let mut client = DtlsOpenSsl::new("client".to_string())
        .with_null_encryption()
        .with_handle_message(Arc::new(|_, _, _, _| {}))
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    mux_cli.start().expect("client mux start");
    client.start(mux_cli.clone()).expect("client start");

    let endpoint = NetworkEndpoint::new_direct(addr_srv);

    // 4. Trigger failure by stopping mux before send
    // (Actually, to trigger the specific bug of leaving an entry in peer_states,
    // we want a failure DURING the handshake loop in send)

    tracing::info!("Attempting send with stopped mux to trigger failure...");
    mux_cli.stop();

    let res = client.send(&endpoint, b"Hello fail");
    assert!(res.is_err(), "Send should fail because mux is stopped");
    tracing::info!("First send failed as expected: {}", res.err().unwrap());

    // 5. Start server and a NEW mux for client
    tracing::info!("Starting server and new client mux...");
    mux_srv.start().expect("server mux start");
    server.start(mux_srv.clone()).expect("server start");

    let mux_cli2 = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux 2"));
    mux_cli2.start().expect("client mux 2 start");
    client
        .start(mux_cli2.clone())
        .expect("client restart with new mux");

    // 6. Attempt send again.
    // If the bug exists, this will use the old SslStream (linked to mux_cli) and fail.
    // If fixed, it will create a new SslStream (linked to mux_cli2) and succeed.
    tracing::info!("Attempting second send (should succeed if bug is fixed)...");
    let res2 = client.send(&endpoint, b"Hello success");

    assert!(
        res2.is_ok(),
        "Second send should succeed after re-starting with new mux: {:?}",
        res2.err()
    );
    tracing::info!("Second send succeeded!");

    // 7. Test with SAME endpoint coming online
    tracing::info!("Testing same endpoint coming online...");

    // Create a new endpoint on a fresh port
    let mux_srv3 = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind server mux 3"));
    let addr_srv3 = mux_srv3.local_addr().expect("server addr 3");
    let endpoint3 = NetworkEndpoint::new_direct(addr_srv3);

    // Try to send while server is NOT started - this should fail fast because mux write will fail if we use a non-listening port?
    // Actually UDP send doesn't fail if nobody listens.
    // So we need to wait for timeout OR trigger a write failure.
    // To trigger a write failure without stopping the mux, we can't easily.
    // So let's just use the timeout but with a shorter deadline for the test if we could...
    // But we can't.

    // Instead, let's just simulate the peer state being there by manually stopping the mux AGAIN.
    mux_cli2.stop();
    let _ = client.send(&endpoint3, b"Hello fail 3");

    // Now start a NEW mux for client and the server for endpoint3
    let mux_cli3 = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux 3"));
    mux_cli3.start().expect("client mux 3 start");
    client.start(mux_cli3).expect("client restart 3");

    mux_srv3.start().expect("server mux 3 start");
    server.start(mux_srv3).expect("server 3 start");

    let res4 = client.send(&endpoint3, b"Hello success 3");
    assert!(
        res4.is_ok(),
        "Fourth send should succeed to the same endpoint after it comes online: {:?}",
        res4.err()
    );
    tracing::info!("Fourth send succeeded!");
}
