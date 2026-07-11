use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::util::reusable_mock_api::MockApiBoth;
use bingle_core::api::bingle_api::StartOptions;
use bingle_core::engine::Engine;

#[path = "../test_util.rs"]
pub mod test_util;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn engine_turn_handler_fails_when_no_public_addr() {
    let api = MockApiBoth::new();
    let api_weak = crate::util::reusable_mock_api::to_weak_api_both(api);

    let mut opts = StartOptions::new("".into());
    opts.am_relay = true;

    let engine = Engine::new(&opts, api_weak);

    let port = test_util::find_unused_loopback_port();
    let _addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

    let res = engine.start(&opts);
    if let Err(e) = res {
        println!("Engine start failed: {}", e);
    }
}
