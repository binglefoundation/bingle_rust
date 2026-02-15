use std::net::SocketAddr;
use rust_comms::api::bingle_api::{BingleApi, StartOptions, NetworkEndpoint, UserId, Handle, ProgressCallback};
use rust_comms::engine::Engine;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};

#[cfg(not(target_os = "ios"))]
use rust_comms::dtls::DtlsOpenSsl;
use crate::util::mock_api::MockApiBoth;

#[test]
fn engine_dtls_is_none_before_configuration() {
    let engine = Engine::new(&StartOptions::default(), crate::util::mock_api::to_weak(MockApiBoth::new()));
    assert!(engine.dtls().is_none(), "Engine::dtls() should be None when DTLS not configured");
}

#[cfg(not(target_os = "ios"))]
#[test]
fn engine_dtls_send_without_start_fails() {
    let mut engine = Engine::new(&StartOptions::default(), crate::util::mock_api::to_weak(MockApiBoth::new()));
    // Provide a DTLS instance but DO NOT call engine.start(); ensure direct send fails.
    engine.set_dtls(Box::new(DtlsOpenSsl::new()));

    let to: SocketAddr = "127.0.0.1:9".parse().unwrap(); // discard port
    let res = engine
        .dtls()
        .expect("dtls should be present")
        .send(&rust_comms::api::bingle_api::NetworkEndpoint::new_direct(to), b"hello");
    assert!(res.is_err(), "Dtls::send should error when DTLS was not started");
    let msg = res.err().unwrap();
    // Accept common error messages
    assert!(msg.contains("not started") || msg.to_lowercase().contains("requires start") || msg.to_lowercase().contains("bind"),
        "unexpected error message: {}", msg);
}
