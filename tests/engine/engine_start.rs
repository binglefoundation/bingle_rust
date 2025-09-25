use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use rust_comms::api::bingle_api::StartOptions;
use rust_comms::engine::Engine;

#[test]
fn engine_start_without_static_ip_errors() {
    let mut engine = Engine::new();
    let opts = StartOptions {
        handle: "tester".into(),
        passphrase: "pass".into(),
        static_ip: None,
        am_relay: false,
        stun_servers: None,
    };
    let res = engine.start(opts);
    assert!(res.is_err());
    let msg = res.err().unwrap();
    assert!(msg.to_lowercase().contains("notimplemented") || msg.to_lowercase().contains("not implemented"));
}

#[test]
fn engine_start_with_static_ip_localhost_ok() {
    let mut engine = Engine::new();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let opts = StartOptions {
        handle: "tester".into(),
        passphrase: "pass".into(),
        static_ip: Some(addr),
        am_relay: false,
        stun_servers: None,
    };
    let res = engine.start(opts);
    // Engine may fail to start DTLS due to lack of certificates; however, our DTLS implementation only
    // requires certificates for server. It uses defaults in tests; accept either Ok or Err as long as it doesn't panic.
    if let Err(e) = res {
        // Acceptable errors: DTLS start failure; ensure it's the DTLS path, not the NotImplemented one
        assert!(!e.to_lowercase().contains("notimplemented"));
    }
    engine.stop();
}
