use rust_comms::api::bingle_api::StartOptions;
use rust_comms::engine::Engine;
use std::net::SocketAddr;

use crate::util::reusable_mock_api::MockApiBoth;
#[cfg(not(target_os = "ios"))]
use rust_comms::dtls::DtlsOpenSsl;

#[cfg(not(target_os = "ios"))]
#[test]
fn engine_dtls_send_without_start_fails() {
    let mut engine = Engine::new_with_dtls(&StartOptions::default(), 
                                 crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()), Box::new(DtlsOpenSsl::new()));
    // Provide a DTLS instance but DO NOT call engine.start(); ensure direct send fails.
    let to: SocketAddr = "127.0.0.1:9".parse().unwrap(); // discard port
    let res = engine
        .dtls()
        .send(&rust_comms::api::bingle_api::NetworkEndpoint::new_direct(to), b"hello");
    assert!(res.is_err(), "Dtls::send should error when DTLS was not started");
    let msg = res.err().unwrap();
    // Accept common error messages
    assert!(msg.contains("not started") || msg.to_lowercase().contains("requires start") || msg.to_lowercase().contains("bind"),
        "unexpected error message: {}", msg);
}
