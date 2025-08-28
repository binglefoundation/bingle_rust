#![cfg(all(unix, not(target_os = "ios")))]

use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use rust_comms::dtls::{Dtls, DtlsOpenSsl};

#[test]
fn dtls_client_send_path_builds_and_returns_ok() {
    // Use the same self-signed server cert as both CA and client cert for the test environment.
    let cert_pem: Vec<u8> = include_bytes!("../dtls_test/server.crt").to_vec();
    let key_pem: Vec<u8> = include_bytes!("../dtls_test/server.key").to_vec();
    let cert_pem_for_server = cert_pem.clone();
    let key_pem_for_server = key_pem.clone();

    // Pick an unused port for the server.
    let probe = UdpSocket::bind(("127.0.0.1", 0)).expect("bind probe");
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let addr: SocketAddr = SocketAddr::from(([127, 0, 0, 1], port));

    // Start a plaintext UDP listener so the port is bound (server DTLS loop may fall back to UDP).
    // We don't need to receive data here; this test focuses on the client send path building OK.
    let server_cert_pem = cert_pem_for_server.clone();
    let server_key_pem = key_pem_for_server.clone();
    let server_ca_pem = cert_pem_for_server.clone();
    std::thread::spawn(move || {
        let mut server = DtlsOpenSsl::new()
            .as_server()
            .with_handle_message(|_, _, _| {})
            .with_server_signing_cert(server_cert_pem)
            .with_server_signing_private_key(server_key_pem)
            .with_ca_cert(server_ca_pem);
        let _ = server.start_server(addr);
        // Keep thread alive briefly to allow client call to occur.
        std::thread::sleep(Duration::from_millis(100));
    });

    // Build the DTLS client and validate that send() returns Ok after preparing context.
    let client = DtlsOpenSsl::new()
        .as_client()
        .with_client_cert(cert_pem.clone())
        .with_client_private_key(key_pem.clone())
        .with_ca_cert(cert_pem.clone());

    // Even though a full DTLS client handshake isn't implemented yet, send() currently
    // prepares the client context and returns Ok. Ensure this path doesn't error with
    // provided credentials.
    assert!(client.send(addr, b"ping").is_ok());
}
