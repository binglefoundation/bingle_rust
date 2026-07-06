// Localnet integration tests for the local-storage migration flow.
//
// Exercises the real BingleDapp on algokit localnet: register a client on an "old" app,
// deploy a "new" app that blesses the old one as a predecessor, and verify that
// AlgoBingle::ensure_local_migrated copies the client's local state (Handle) into the new
// app. Also covers the fresh-install no-op, idempotency, and the two-versions-behind case
// where the client's data lives on an ancestor older than the immediate predecessor.

use std::time::{Duration, Instant};

use rust_comms::blockchain::algo_bingle::AlgoBingle;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;
use test_util::{
    deploy_bingle_app, deploy_bingle_app_and_asset, localnet_config, ops_from_mnemonic,
    register_client_on_blockchain, ADDRESS_APP_CREATOR, ADDRESS_RECEIVE, ADDRESS_SPEND,
    PASSPHRASE_APP_CREATOR, PASSPHRASE_RECEIVE, PASSPHRASE_SPEND,
};

/// Poll the account's local state on `app_id` until it holds `Handle == handle`, or panic.
fn wait_for_handle(ops: &rust_comms::blockchain::algo_ops::AlgoOps, app_id: u64, account: &str, handle: &str) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        if let Ok(Some(entries)) = ops.local_state_for_account(app_id, account)
            && entries.iter().any(|(k, v)| k == "Handle" && v == handle) {
                return;
            }
        std::thread::sleep(Duration::from_millis(500));
    }
    panic!("Handle '{}' not visible in local state of app {} for {} within timeout", handle, app_id, account);
}

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn migrate_local_from_predecessor_localnet() {
    test_util::assert_localnet_available();
    let cfg = localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_RECEIVE, ADDRESS_SPEND])
        .expect("fund client accounts; install algokit and start localnet");

    let creator_ops = ops_from_mnemonic(ADDRESS_APP_CREATOR, PASSPHRASE_APP_CREATOR, cfg.clone());

    // Old app (with the Bingle$ ASA) — the client registers here.
    let (old_app, asset_id) = deploy_bingle_app_and_asset(&creator_ops, "MIGOLD", 1_000_000);
    let handle = "migrant_one";
    register_client_on_blockchain(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, handle, old_app, asset_id, &creator_ops, cfg.clone());

    // New app (same creator) blesses the old app as a migration source.
    let new_app = deploy_bingle_app(&creator_ops);
    let creator_bgl = AlgoBingle::new(creator_ops.clone(), new_app, 0);
    creator_bgl.set_predecessor_app(new_app, old_app).expect("set_predecessor_app");

    // A genuinely fresh account (no data on any ancestor) must be a no-op.
    let fresh_ops = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let fresh_bgl = AlgoBingle::new(fresh_ops, new_app, asset_id);
    assert!(
        fresh_bgl.ensure_local_migrated(new_app).expect("fresh ensure_local_migrated").is_none(),
        "a fresh install with no ancestor data must not migrate"
    );

    // The registered client migrates its local state to the new app.
    let client_ops = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());
    let client_bgl = AlgoBingle::new(client_ops.clone(), new_app, asset_id);
    let tx = client_bgl.ensure_local_migrated(new_app).expect("ensure_local_migrated");
    assert!(tx.is_some(), "expected a migration transaction for the registered client");

    wait_for_handle(&client_ops, new_app, ADDRESS_RECEIVE, handle);

    // Idempotency: a second call is a no-op now that the handle exists on the new app.
    assert!(
        client_bgl.ensure_local_migrated(new_app).expect("second ensure_local_migrated").is_none(),
        "migration must be idempotent once the account is registered on the new app"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn migrate_local_two_versions_back_localnet() {
    test_util::assert_localnet_available();
    let cfg = localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_RECEIVE])
        .expect("fund client account; install algokit and start localnet");

    let creator_ops = ops_from_mnemonic(ADDRESS_APP_CREATOR, PASSPHRASE_APP_CREATOR, cfg.clone());

    // app_a (with the ASA): the client registers here — this is two versions back from app_c.
    let (app_a, asset_id) = deploy_bingle_app_and_asset(&creator_ops, "MIGA", 1_000_000);
    let handle = "migrant_two";
    register_client_on_blockchain(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, handle, app_a, asset_id, &creator_ops, cfg.clone());

    // app_b -> predecessor app_a; app_c -> predecessor app_b. app_c's lineage accumulates to
    // {app_b, app_a}, so a client whose data lives on app_a can migrate directly to app_c.
    let app_b = deploy_bingle_app(&creator_ops);
    AlgoBingle::new(creator_ops.clone(), app_b, 0)
        .set_predecessor_app(app_b, app_a)
        .expect("set_predecessor_app(app_b, app_a)");

    let app_c = deploy_bingle_app(&creator_ops);
    AlgoBingle::new(creator_ops.clone(), app_c, 0)
        .set_predecessor_app(app_c, app_b)
        .expect("set_predecessor_app(app_c, app_b)");

    // Sanity: app_c's lineage includes app_a (two versions back).
    let lineage = AlgoBingle::new(creator_ops.clone(), app_c, 0)
        .ancestor_apps(app_c)
        .expect("ancestor_apps(app_c)");
    assert!(lineage.contains(&app_a), "app_c lineage {:?} should contain app_a {}", lineage, app_a);

    // The client migrates directly from app_a (its data source) to app_c in one hop.
    let client_ops = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());
    let client_bgl = AlgoBingle::new(client_ops.clone(), app_c, asset_id);
    let tx = client_bgl.ensure_local_migrated(app_c).expect("ensure_local_migrated to app_c");
    assert!(tx.is_some(), "expected a migration transaction from two versions back");

    wait_for_handle(&client_ops, app_c, ADDRESS_RECEIVE, handle);
}
