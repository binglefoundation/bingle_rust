#![cfg(not(target_os = "ios"))]

use std::net::{SocketAddr, UdpSocket};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use rust_comms::dtls::{Dtls, DtlsOpenSsl};
mod pki;

static SERVER_HELLO: OnceLock<Vec<u8>> = OnceLock::new();
static SERVER_CLIENT_ECHOED: OnceLock<Vec<u8>> = OnceLock::new();
static CLIENT_PING_SEEN: OnceLock<Vec<u8>> = OnceLock::new();

fn client_echo_handler(server: &dyn Dtls, from: &SocketAddr, _issuer: &str, data: &[u8]) {
    // Record that client received the Ping
    if data == b"Ping" {
        let _ = CLIENT_PING_SEEN.set(data.to_vec());
    }
    // Echo back to the server with the required prefix
    let mut echoed = b"CLIENT ECHOED: ".to_vec();
    echoed.extend_from_slice(data);
    let _ = server.send(*from,  &echoed);
}

fn server_capture_and_trigger_handler(server: &dyn Dtls, from: &SocketAddr, _issuer: &str, data: &[u8]) {
    // Capture the initial Hello and immediately send Ping to the client
    if data == b"Hello" {
        let _ = SERVER_HELLO.set(data.to_vec());
        let _ = server.send(*from,  b"Ping");
        return;
    }
    // Capture the client's echoed message
    if data.starts_with(b"CLIENT ECHOED: ") {
        let _ = SERVER_CLIENT_ECHOED.set(data.to_vec());
    }
}

#[test]
fn dtls_client_echo_roundtrip() {
    use std::time::Instant;

    // Generate Ed25519 CA, server, and client credentials dynamically
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let client_cert_pem: Vec<u8> = certs.client_crt.clone();
    let client_key_pem: Vec<u8> = certs.client_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Create and start a UDP mux for the server and determine its bound address.
    let mux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    let mux = std::sync::Arc::new(mux0);
    let addr: SocketAddr = mux.local_addr().expect("mux addr");

    // Build and configure the server instance with handler that captures and triggers Ping
    let mut server = DtlsOpenSsl::new()
        .with_handle_message(std::sync::Arc::new(server_capture_and_trigger_handler))
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

    // Start mux then the DTLS server with the mux
    mux.start().expect("mux start");
    server.start(mux.clone()).expect("start should succeed");

    // Give the background thread a moment
    thread::sleep(Duration::from_millis(400));

    // Build the DTLS client with the echo handler
    let mut client = DtlsOpenSsl::new()
        .with_handle_message(std::sync::Arc::new(client_echo_handler))
        .with_client_cert(client_cert_pem.clone())
        .with_client_private_key(client_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

    // Start client mux and DTLS
    let cmux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux");
    let cmux = std::sync::Arc::new(cmux0);
    cmux.start().expect("client mux start");
    client.start(cmux.clone()).expect("client start");

    // Step 1: Send initial "Hello" from client to server and validate reception
    let mut ok = false;
    for _ in 0..6 {
        if client.send(addr,  b"Hello").is_ok() { ok = true; break; }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(ok, "client DTLS send of 'Hello' failed");

    let start = Instant::now();
    while SERVER_HELLO.get().is_none() && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(10));
    }
    let hello = SERVER_HELLO.get().expect("server did not receive 'Hello' within timeout");
    assert_eq!(hello.as_slice(), b"Hello", "server did not capture initial 'Hello'");

    // Step 2: Validate that the client echo handler received the "Ping" (server sends Ping after Hello)
    let start = Instant::now();
    while CLIENT_PING_SEEN.get().is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    let ping = CLIENT_PING_SEEN.get().expect("client did not receive 'Ping' within timeout");
    assert_eq!(ping.as_slice(), b"Ping", "client did not capture 'Ping'");

    // Step 3: Validate that the server received the client's echoed message
    let start = Instant::now();
    while SERVER_CLIENT_ECHOED.get().is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    let echoed = SERVER_CLIENT_ECHOED.get().expect("server did not receive client's echo within timeout");
    assert_eq!(echoed.as_slice(), b"CLIENT ECHOED: Ping", "server did not receive 'CLIENT ECHOED: Ping'");
}
