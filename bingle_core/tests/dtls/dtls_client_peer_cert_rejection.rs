use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use bingle_core::dtls::{Dtls, DtlsOpenSsl, Result as DtlsResult};
pub mod pki;

fn reject_handler(_cert_pem: &[u8], _ca_pem: &[u8]) -> DtlsResult<String> {
    Err("rejected".to_string())
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_openssl_client_rejects_server_when_peer_cert_handler_fails() {
    // Generate test certificates
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Create and start a UDP mux for the server and determine its bound address.
    let mux0 = bingle_core::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    let mux = std::sync::Arc::new(mux0);
    let addr: SocketAddr = mux.local_addr().expect("mux addr");

    // Server records whether any application data ever reached its app layer.
    let delivered = std::sync::Arc::new(AtomicBool::new(false));
    let delivered_clone = delivered.clone();

    // Start normal server (accepting), capturing any delivered application data.
    let server = DtlsOpenSsl::new("server".to_string())
        .with_null_encryption()
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_message(std::sync::Arc::new(
            move |_server, _from, _issuer, _data| {
                delivered_clone.store(true, Ordering::SeqCst);
            },
        ));
    mux.start().expect("mux start");
    server.start(mux.clone()).expect("server start");
    thread::sleep(Duration::from_millis(200));

    // Build client with a rejecting peer certificate handler, and provide server credentials for its accept loop
    let certs_b = pki::generate_ed25519_test_certs();
    let client = DtlsOpenSsl::new("client".to_string())
        .with_null_encryption()
        .with_handle_peer_certificate(reject_handler)
        .with_server_signing_cert(certs_b.server_crt.clone())
        .with_server_signing_private_key(certs_b.server_key.clone())
        .with_ca_cert(certs_b.ca_crt.clone());

    // Start client mux and DTLS
    let cmux0 = bingle_core::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux");
    let cmux = std::sync::Arc::new(cmux0);
    cmux.start().expect("client mux start");
    client.start(cmux.clone()).expect("client start");

    // Drive the handshake by attempting to send. Because the client rejects the server's
    // certificate, the handshake must abort and the payload must never reach the server's app
    // layer. Assert on that observable effect (non-delivery) rather than on send()'s return
    // value: send() is asynchronous and only reports whether the packet was queued, not whether
    // the handshake ultimately succeeded, so checking its Ok/Err is racy under load.
    for _ in 0..6 {
        let _ = client.send(
            &bingle_core::api::bingle_api::NetworkEndpoint::new_direct(addr),
            b"should-fail",
        );
        thread::sleep(Duration::from_millis(50));
    }

    // Give the (rejected) handshake ample time to complete-or-fail before checking.
    thread::sleep(Duration::from_millis(300));
    assert!(
        !delivered.load(Ordering::SeqCst),
        "application data was delivered even though the client rejected the server certificate"
    );
}
