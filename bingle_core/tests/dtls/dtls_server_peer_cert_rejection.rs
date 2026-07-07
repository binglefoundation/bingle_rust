use std::net::SocketAddr;
use std::thread;
use std::time::Duration;

use bingle_core::dtls::{Dtls, DtlsOpenSsl, Result as DtlsResult};
pub mod pki;

fn reject_handler(_cert_pem: &[u8], _ca_pem: &[u8]) -> DtlsResult<String> {
    Err("rejected".to_string())
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_openssl_server_rejects_client_when_peer_cert_handler_fails() {
    // Generate test certificates
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let _client_cert_pem: Vec<u8> = certs.client_crt.clone();
    let _client_key_pem: Vec<u8> = certs.client_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Create and start a UDP mux for the server and determine its bound address.
    let mux0 = bingle_core::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    let mux = std::sync::Arc::new(mux0);
    let addr: SocketAddr = mux.local_addr().expect("mux addr");

    // Start server with a peer certificate handler that rejects
    let mut server = DtlsOpenSsl::new("server".to_string())
        .with_null_encryption()
        .with_handle_peer_certificate(reject_handler)
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

    mux.start().expect("mux start");
    server.start(mux.clone()).expect("server start");
    thread::sleep(Duration::from_millis(200));

    // Build client with valid certs and provide server credentials for its accept loop
    let certs_b = pki::generate_ed25519_test_certs();
    let mut client = DtlsOpenSsl::new("client".to_string())
        .with_null_encryption()
        .with_client_cert(certs_b.client_crt.clone())
        .with_client_private_key(certs_b.client_key.clone())
        .with_server_signing_cert(certs_b.server_crt.clone())
        .with_server_signing_private_key(certs_b.server_key.clone())
        .with_ca_cert(certs_b.ca_crt.clone());

    // Start client mux and DTLS
    let cmux0 = bingle_core::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux");
    let cmux = std::sync::Arc::new(cmux0);
    cmux.start().expect("client mux start");
    client.start(cmux.clone()).expect("client start");

    // Attempt to send; expect handshake failure => Err
    let mut any_ok = false;
    for _ in 0..6 {
        if client
            .send(
                &bingle_core::api::bingle_api::NetworkEndpoint::new_direct(addr),
                b"should-fail",
            )
            .is_ok()
        {
            any_ok = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !any_ok,
        "handshake unexpectedly succeeded when server rejected peer certificate"
    );
}
