use crate::setup_localnet;
use crate::util::test_util;
use bingle_core::api::bingle_api::{BingleApi, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::blockchain::algo_ops::AlgoOps;
use bingle_core::engine::BingleAccessUnsafeForTests;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_relay_start_fails_if_not_allowed_on_chain() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    let creator_addr = test_util::ADDRESS_SPEND;
    let creator_pass = test_util::PASSPHRASE_SPEND;
    let relay_addr_str = test_util::ADDRESS_RECEIVE;
    let relay_pass = test_util::PASSPHRASE_RECEIVE;

    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[creator_addr, relay_addr_str])
        .expect("Failed to fund localnet accounts");

    let ops_creator = AlgoOps::new(Some(creator_pass.to_string()), None, Some(cfg.clone()));
    let ops_admin = AlgoOps::new(
        Some(test_util::PASSPHRASE_APP_ADMIN.to_string()),
        None,
        Some(cfg.clone()),
    );
    let (app_id, _asset_id) =
        test_util::deploy_bingle_app_and_asset(&ops_creator, "BINGLE$", 1_000_000);

    let ops_relay = AlgoOps::new(Some(relay_pass.to_string()), None, Some(cfg.clone()));
    ops_relay.opt_in_app(app_id).expect("relay opt-in app");

    // We do NOT call set_allow_relay here.

    let r_opts = StartOptions {
        handle: "relay_fail".into(),
        algo_passphrase: Some(relay_pass.to_string()),
        static_ip: Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)),
        am_relay: true,
        stun_servers: None,
        algo_provider_config: Some(cfg.clone()),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };

    let relay = BingleApiImpl::new(&r_opts);
    let res = relay.access_unsafe_for_tests(|api| api.start(&r_opts));

    assert!(
        res.is_err(),
        "relay start should fail because allow_relay is not set"
    );
    let err_msg = res.unwrap_err().to_string();
    assert!(
        err_msg.contains("not allowed to relay"),
        "expected 'not allowed to relay' error, got: {}",
        err_msg
    );

    // Now set it and try again (admin signs the allow_relay call)
    let ab_admin = AlgoBingle::new(ops_admin, app_id, 0);
    ab_admin
        .set_allow_relay(app_id, relay_addr_str, true)
        .expect("set_allow_relay");

    let res = relay.access_unsafe_for_tests(|api| api.start(&r_opts));
    assert!(
        res.is_ok(),
        "relay start should succeed after allow_relay is set: {:?}",
        res.err()
    );
}
