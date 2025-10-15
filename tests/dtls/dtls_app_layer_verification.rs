#![cfg(not(target_os = "ios"))]

use std::net::SocketAddr;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;

use rust_comms::dtls::{Dtls, DtlsOpenSsl, Result as DtlsResult};

mod pki;

fn reject_handler(_cert_pem: &[u8], _ca_pem: &[u8]) -> DtlsResult<String> {
    Err("rejected".to_string())
}

fn accept_all_handler(_cert_pem: &[u8], _ca_pem: &[u8]) -> DtlsResult<String> {
    Ok("issuer".to_string())
}

#[ntest::timeout(30_000)]
#[test]
fn dtls_app_layer_verification_reject_blocks_delivery_but_handshake_succeeds() {
    // Generate test certificates (CA + server/client certs and keys)
    let certs = pki::generate_ed25519_test_certs();

    // Start server mux and DTLS with app-layer-only verification and a rejecting handler.
    let smux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind server mux");
    let smux = Arc::new(smux0);
    let saddr: SocketAddr = smux.local_addr().expect("server addr");

    let delivered = Arc::new(AtomicBool::new(false));
    let delivered_clone = delivered.clone();

    let mut server = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(certs.ca_crt.clone())
        .with_app_layer_only_verification(true)
        .with_handle_peer_certificate(reject_handler)
        .with_handle_message(Arc::new(move |_server, _from, _issuer, _data| {
            delivered_clone.store(true, Ordering::SeqCst);
        }));

    smux.start().expect("server mux start");
    server.start(smux.clone()).expect("server start");

    // Start client mux and DTLS with app-layer-only verification, configured with a client cert/key.
    let cmux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux");
    let cmux = Arc::new(cmux0);
    let mut client = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(certs.ca_crt.clone())
        .with_app_layer_only_verification(true);

    cmux.start().expect("client mux start");
    client.start(cmux.clone()).expect("client start");

    // Attempt to send a payload; handshake should succeed (Ok) even though the handler rejects later.
    let ok = client.send(saddr, b"hello-app").is_ok();
    assert!(ok, "handshake should succeed under app-layer-only verification");

    // Give server some time; because the handler rejects the cert at app layer, delivery should be blocked.
    thread::sleep(Duration::from_millis(300));
    assert_eq!(delivered.load(Ordering::SeqCst), false, "application data should be blocked when app-layer verification rejects");
}

#[ntest::timeout(30_000)]
#[test]
fn dtls_app_layer_verification_accept_all_delivers_application_data() {
    let certs = pki::generate_ed25519_test_certs();

    let smux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind server mux");
    let smux = Arc::new(smux0);
    let saddr: SocketAddr = smux.local_addr().expect("server addr");

    let delivered = Arc::new(AtomicBool::new(false));
    let delivered_clone = delivered.clone();

    let mut server = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(certs.ca_crt.clone())
        .with_app_layer_only_verification(true)
        .with_handle_peer_certificate(accept_all_handler)
        .with_handle_message(Arc::new(move |_server, _from, _issuer, _data| {
            delivered_clone.store(true, Ordering::SeqCst);
        }));

    smux.start().expect("server mux start");
    server.start(smux.clone()).expect("server start");

    // Start client
    let cmux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux");
    let cmux = Arc::new(cmux0);
    let mut client = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(certs.ca_crt.clone())
        .with_app_layer_only_verification(true);

    cmux.start().expect("client mux start");
    client.start(cmux.clone()).expect("client start");

    let ok = client.send(saddr, b"hello-app").is_ok();
    assert!(ok, "handshake should succeed under app-layer-only verification");

    // Wait until delivered or timeout
    for _ in 0..40 {
        if delivered.load(Ordering::SeqCst) { break; }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(delivered.load(Ordering::SeqCst), "application data should be delivered when app-layer verification accepts");
}