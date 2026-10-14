/// Integration test for issue #280: a live client's advert is kept on its relay by the refresh,
/// and a client that stops refreshing (as if it crashed) is not found once the time to live has
/// passed, and is then removed from the relay's memory by the sweep.
///
/// Runs a real relay and client on algokit localnet with a short time to live. The unit tests in
/// `tests/ddb/advert_expiry.rs` and `tests/engine/advert_refresh.rs` cover the rules.
///
/// Run with:
///   cargo test --test integration advert_expiry_localnet
///
/// Prerequisites: `algokit localnet start` must be running.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use algo_ops::AlgoOps;
use bingle_core::api::bingle_api::{BingleApi, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::engine::BingleAccessUnsafeForTests;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::relay_test_util::wait_for_relays_visible;
use crate::util::test_util;

const TTL: Duration = Duration::from_secs(6);

fn start_options(
    handle: &str,
    passphrase: &str,
    am_relay: bool,
    cfg: &algo_ops::AlgoChainConfig,
    app_id: u64,
    static_ip: SocketAddr,
) -> StartOptions {
    let mut opts = StartOptions::new(handle.into());
    opts.algo_passphrase = Some(passphrase.to_string());
    opts.static_ip = Some(static_ip);
    opts.am_relay = am_relay;
    opts.algo_provider_config = Some(cfg.clone());
    opts.app_id = Some(app_id);
    opts.dangerous_debug = true;
    opts.advert_ttl = Some(TTL);
    opts
}

fn relay_has(relay: &Arc<BingleApiImpl>, id: &str) -> bool {
    relay
        .with_engine_mut(|e| e.ddb_backend_lookup_for_tests(id))
        .is_some()
}

#[test]
#[serial]
#[cfg(not(target_os = "ios"))]
pub fn live_advert_is_kept_and_a_silent_client_expires() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    let relay_account = test_util::ADDRESS_SPEND;
    let client_account = test_util::ADDRESS_RECEIVE;
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[relay_account, client_account])
        .expect("fund localnet accounts");

    let admin_ops = test_util::ops_from_mnemonic(
        test_util::ADDRESS_APP_ADMIN,
        test_util::PASSPHRASE_APP_ADMIN,
        cfg.clone(),
    );
    let relay_ops =
        test_util::ops_from_mnemonic(relay_account, test_util::PASSPHRASE_SPEND, cfg.clone());
    let (app_id, asset_id) =
        test_util::deploy_bingle_app_and_asset(&relay_ops, "BINGLE$", 1_000_000);
    let admin = AlgoBingle::new(admin_ops.clone(), app_id, 0);

    // The relay: opted in, permitted, and discoverable through its static endpoint.
    let relay_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        test_util::find_unused_loopback_port(),
    );
    register_relay_static_endpoint(&relay_ops, &admin, relay_addr, relay_account, app_id);
    assert!(
        wait_for_relays_visible(
            &admin,
            app_id,
            &[(relay_account.to_string(), relay_addr)],
            Duration::from_secs(60)
        ),
        "relay did not become visible via the indexer"
    );
    let relay_opts = start_options(
        "advert-expiry-relay",
        test_util::PASSPHRASE_SPEND,
        true,
        &cfg,
        app_id,
        relay_addr,
    );
    let relay = BingleApiImpl::new(&relay_opts);
    test_util::register_client_on_blockchain(
        relay_account,
        test_util::PASSPHRASE_SPEND,
        &relay_opts.handle,
        app_id,
        asset_id,
        &admin_ops,
        cfg.clone(),
    );
    relay
        .access_unsafe_for_tests(|api| api.start(&relay_opts))
        .expect("relay start");
    assert!(
        test_util::wait_for_relay_available(&relay, Duration::from_secs(60)),
        "relay did not become Available"
    );
    assert!(
        relay.with_engine_mut(|e| e.advert_sweep_running_for_tests()),
        "the relay sweeps expired adverts"
    );

    // The client registers its advert with the relay.
    let client_opts = start_options(
        "advert-expiry-client",
        test_util::PASSPHRASE_RECEIVE,
        false,
        &cfg,
        app_id,
        SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            test_util::find_unused_loopback_port(),
        ),
    );
    let client = BingleApiImpl::new(&client_opts);
    test_util::register_client_on_blockchain(
        client_account,
        test_util::PASSPHRASE_RECEIVE,
        &client_opts.handle,
        app_id,
        asset_id,
        &admin_ops,
        cfg.clone(),
    );
    client
        .access_unsafe_for_tests(|api| api.start(&client_opts))
        .expect("client start");
    assert!(
        test_util::wait_for_registered(&client, Duration::from_secs(60)),
        "client did not become Registered"
    );
    let client_id = client.get_my_id().expect("client id");
    assert!(relay_has(&relay, &client_id), "the client's advert is held");
    assert!(client.with_engine_mut(|e| e.advert_refresh_running_for_tests()));

    // Live: over two time-to-live periods the advert is never absent for longer than one
    // refresh interval (a third of the time to live).
    let refresh_interval = TTL / 3;
    let watch_until = Instant::now() + TTL * 2;
    let mut absent_since: Option<Instant> = None;
    let mut longest_absence = Duration::ZERO;
    while Instant::now() < watch_until {
        if relay_has(&relay, &client_id) {
            absent_since = None;
        } else {
            let since = *absent_since.get_or_insert_with(Instant::now);
            longest_absence = longest_absence.max(since.elapsed());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        longest_absence <= refresh_interval,
        "a live advert was absent for {longest_absence:?}"
    );
    assert!(relay_has(&relay, &client_id), "the live advert is held");

    // Silent: the client stops refreshing without signing off, as if it had crashed. Its advert
    // is not found within one time to live of the last refresh.
    client.with_engine_mut(|e| e.stop_advert_refresh_for_tests());
    let stopped = Instant::now();
    let deadline = stopped + TTL + Duration::from_secs(1);
    while relay_has(&relay, &client_id) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        !relay_has(&relay, &client_id),
        "the silent client's advert outlived the time to live"
    );
    assert!(
        stopped.elapsed() <= TTL + Duration::from_secs(1),
        "expired after {:?}",
        stopped.elapsed()
    );

    // The background sweep (every half time to live) then removes it from the relay's memory,
    // so a manual sweep finds nothing left to remove.
    std::thread::sleep(TTL / 2 + Duration::from_secs(1));
    let left = relay.with_engine_mut(|e| e.sweep_expired_adverts());
    assert!(
        !left.contains(&client_id),
        "the expired advert was still held in memory: {left:?}"
    );

    client.access_unsafe_for_tests(|api| api.stop());
    relay.access_unsafe_for_tests(|api| api.stop());
}

fn register_relay_static_endpoint(
    relay_ops: &AlgoOps,
    admin: &AlgoBingle,
    relay_addr: SocketAddr,
    relay_account: &str,
    app_id: u64,
) {
    let relay_ab = AlgoBingle::new(relay_ops.clone(), app_id, 0);
    relay_ops
        .opt_in_app(app_id)
        .expect("relay opt-in should succeed");
    admin
        .set_allow_static(app_id, relay_account, true)
        .expect("set_allow_static should succeed");
    admin
        .set_allow_relay(app_id, relay_account, true)
        .expect("set_allow_relay should succeed");
    let compact = test_util::get_compact_advert_record(relay_ops, relay_addr, true);
    relay_ab
        .register_endpoint(app_id, &compact)
        .expect("register_endpoint should succeed");
}
