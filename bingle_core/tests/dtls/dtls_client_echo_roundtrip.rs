use std::net::SocketAddr;
use std::thread;
use std::time::Duration;
pub mod pki;
pub mod test_handlers;
#[path = "../test_util.rs"]
pub mod test_util;
use bingle_core::dtls::{Dtls, DtlsOpenSsl};
use test_handlers::*;
use test_util::init_test_logging;

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_client_echo_roundtrip() {
    init_test_logging();
    use std::time::Instant;

    // Generate Ed25519 CA, server, and client credentials dynamically
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

    // Build and configure the server instance with handler that captures and triggers Ping
    let server = DtlsOpenSsl::new("server".to_string())
        .with_null_encryption()
        .with_handle_message(std::sync::Arc::new(server_capture_and_trigger_handler))
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    // Start mux then the DTLS server with the mux
    mux.start().expect("mux start");
    server.start(mux.clone()).expect("start should succeed");

    // Give the background thread a moment
    thread::sleep(Duration::from_millis(400));

    // Build the DTLS client with the echo handler and provide server creds for its accept loop
    let certs_b = pki::generate_ed25519_test_certs();
    let client = DtlsOpenSsl::new("client".to_string())
        .with_null_encryption()
        .with_handle_message(std::sync::Arc::new(client_echo_handler))
        .with_client_cert(certs_b.client_crt.clone())
        .with_client_private_key(certs_b.client_key.clone())
        .with_server_signing_cert(certs_b.server_crt.clone())
        .with_server_signing_private_key(certs_b.server_key.clone())
        .with_ca_cert(certs_b.ca_crt.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);

    // Start client mux and DTLS
    let cmux0 = bingle_core::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux");
    let cmux = std::sync::Arc::new(cmux0);
    cmux.start().expect("client mux start");
    client.start(cmux.clone()).expect("client start");

    // Give the client a brief moment to be fully ready before sending
    thread::sleep(Duration::from_millis(250));

    // Step 1: Send initial "Hello" from client to server and validate reception
    let mut ok = false;
    for _ in 0..20 {
        if client
            .send(
                &bingle_core::api::bingle_api::NetworkEndpoint::new_direct(addr),
                b"Hello",
            )
            .is_ok()
        {
            ok = true;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(ok, "client DTLS send of 'Hello' failed");

    let start = Instant::now();
    while SERVER_HELLO.get().is_none() && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(10));
    }
    let hello = SERVER_HELLO
        .get()
        .expect("server did not receive 'Hello' within timeout");
    assert_eq!(
        hello.as_slice(),
        b"Hello",
        "server did not capture initial 'Hello'"
    );

    // Step 2: Validate that the client echo handler received the "Ping" (server sends Ping after Hello)
    let start = Instant::now();
    while CLIENT_PING_SEEN.get().is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    let ping = CLIENT_PING_SEEN
        .get()
        .expect("client did not receive 'Ping' within timeout");
    assert_eq!(ping.as_slice(), b"Ping", "client did not capture 'Ping'");

    // Step 3: Validate that the server received the client's echoed message
    let start = Instant::now();
    while SERVER_CLIENT_ECHOED.get().is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    let echoed = SERVER_CLIENT_ECHOED
        .get()
        .expect("server did not receive client's echo within timeout");
    assert_eq!(
        echoed.as_slice(),
        b"CLIENT ECHOED: Ping",
        "server did not receive 'CLIENT ECHOED: Ping'"
    );
}
