/// Integration test for issue #120: a relay disabled on chain leaves another relay's set at the
/// next permission refresh, without a sign-off or a restart, and is not re-learned from gossip.
///
/// Drives the real chain path (`Engine::evict_disallowed_relays` → on-chain `allow_relay`) on
/// algokit localnet. The unit tests in `tests/ddb/relay_eviction.rs` cover the eviction rules.
///
/// Run with:
///   cargo test --test integration relay_permission_eviction_localnet
///
/// Prerequisites: `algokit localnet start` must be running.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use algo_ops::AlgoOps;
use bingle_core::api::bingle_api::StartOptions;
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::ddb::{AdvertRecord, InetSocketAddress};
use bingle_core::protocol::ISSUER_SUFFIX;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;

fn signed_relay_record(id: &str, passphrase: &str) -> AdvertRecord {
    let seed = algonaut::crypto::mnemonic::to_key(passphrase).expect("valid mnemonic");
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed.try_into().expect("32 bytes"));
    AdvertRecord::new(
        id.to_string(),
        Some(InetSocketAddress {
            host: "127.0.0.1".to_string(),
            port: 12121,
        }),
        Some(true),
        None,
        None,
        "2026-01-01T00:00:00Z".to_string(),
        &signing_key,
    )
}

#[test]
#[serial]
#[cfg(not(target_os = "ios"))]
pub fn relay_disabled_on_chain_leaves_the_set_at_the_next_refresh() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    let creator_addr = test_util::ADDRESS_SPEND;
    let creator_pass = test_util::PASSPHRASE_SPEND;
    let peer_addr = test_util::ADDRESS_RECEIVE;
    let peer_pass = test_util::PASSPHRASE_RECEIVE;

    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[creator_addr, peer_addr])
        .expect("fund localnet accounts");

    let ops_creator =
        AlgoOps::new_for_algorand(Some(creator_pass.to_string()), None, Some(cfg.clone()));
    let (app_id, _asset_id) =
        test_util::deploy_bingle_app_and_asset(&ops_creator, "BINGLE$", 1_000_000);

    // The peer relay: opted in and permitted to relay by the app admin.
    let ops_peer = AlgoOps::new_for_algorand(Some(peer_pass.to_string()), None, Some(cfg.clone()));
    ops_peer.opt_in_app(app_id).expect("peer opt-in app");
    let ops_admin = AlgoOps::new_for_algorand(
        Some(test_util::PASSPHRASE_APP_ADMIN.to_string()),
        None,
        Some(cfg.clone()),
    );
    let admin = AlgoBingle::new(ops_admin, app_id, 0);
    admin
        .set_allow_relay(app_id, peer_addr, true)
        .expect("set_allow_relay true");

    // This node: a relay engine on the same app (not started; only its DDB is exercised).
    let opts = StartOptions {
        handle: "relay_evictor".into(),
        algo_passphrase: Some(creator_pass.to_string()),
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
        advert_ttl: None,
    };
    let api = BingleApiImpl::new(&opts);
    api.with_engine_mut(|e| e.set_issuer(format!("{creator_addr}{ISSUER_SUFFIX}")));

    // Permitted: the peer's advert is accepted and survives a refresh.
    api.with_engine_mut(|e| e.ddb_upsert_record(signed_relay_record(peer_addr, peer_pass)));
    assert!(
        api.with_engine_mut(|e| e.ddb_backend_lookup_for_tests(peer_addr))
            .is_some(),
        "a permitted relay's advert is accepted"
    );
    let evicted = api.with_engine_mut(|e| e.evict_disallowed_relays());
    assert!(evicted.is_empty(), "a permitted relay is kept: {evicted:?}");

    // Disabled on chain: the next refresh evicts it.
    admin
        .set_allow_relay(app_id, peer_addr, false)
        .expect("set_allow_relay false");
    let evicted = api.with_engine_mut(|e| e.evict_disallowed_relays());
    assert_eq!(evicted, vec![peer_addr.to_string()]);
    assert!(
        api.with_engine_mut(|e| e.ddb_backend_lookup_for_tests(peer_addr))
            .is_none(),
        "the disabled relay has left the set"
    );

    // Gossip (DumpResolve and UpsertResolve both land in ddb_upsert_record) cannot re-add it.
    api.with_engine_mut(|e| e.ddb_upsert_record(signed_relay_record(peer_addr, peer_pass)));
    assert!(
        api.with_engine_mut(|e| e.ddb_backend_lookup_for_tests(peer_addr))
            .is_none(),
        "a gossiped advert for a disabled relay is not re-added"
    );
}
