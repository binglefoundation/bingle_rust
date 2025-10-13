use std::net::SocketAddr;

use rust_comms::engine::Engine;

#[cfg(not(target_os = "ios"))]
use rust_comms::dtls::DtlsOpenSsl;

#[test]
fn dtls_send_without_dtls_returns_error() {
    let engine = Engine::new();
    let to: SocketAddr = "127.0.0.1:9".parse().unwrap(); // discard port
    let res = engine.dtls_send(to, b"hello");
    assert!(res.is_err(), "dtls_send should error when DTLS not configured");
}

#[cfg(not(target_os = "ios"))]
#[test]
fn dtls_send_with_unstarted_dtls_does_not_lazy_start() {
    let mut engine = Engine::new();
    // Provide a DTLS instance but DO NOT call engine.start(); ensure dtls_send does not lazily start it.
    engine.set_dtls(Box::new(DtlsOpenSsl::new()));

    let to: SocketAddr = "127.0.0.1:9".parse().unwrap(); // discard port
    let res = engine.dtls_send(to, b"hello");
    assert!(res.is_err(), "dtls_send should error when DTLS was not started");
    let msg = res.err().unwrap();
    // Accept either the Engine-layer error or the underlying DTLS error message
    assert!(msg.contains("not started") || msg.to_lowercase().contains("requires start"),
        "unexpected error message: {}", msg);
}
