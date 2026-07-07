// Localnet integration tests for superseding an app (the "block old versions" flow).
//
// Exercises the real BingleDapp on algokit localnet: mark an app superseded via
// set_successor_app and verify (a) AlgoBingle::successor_app reads back the successor,
// (b) the user-facing state-changing methods hard-reject once superseded, and (c) the
// migration path still works — a client can migrate its local state OUT of a superseded
// old app into its (un-superseded) successor.

use std::time::{Duration, Instant};

use bingle_core::blockchain::algo_bingle::AlgoBingle;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;
use test_util::{
    deploy_bingle_app, deploy_bingle_app_and_asset, localnet_config, ops_from_mnemonic,
    register_client_on_blockchain, ADDRESS_APP_CREATOR, ADDRESS_RECEIVE,
    PASSPHRASE_APP_CREATOR, PASSPHRASE_RECEIVE,
};

/// Poll the account's local state on `app_id` until it holds `Handle == handle`, or panic.
fn wait_for_handle(ops: &bingle_core::blockchain::algo_ops::AlgoOps, app_id: u64, account: &str, handle: &str) {
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
pub fn set_successor_blocks_old_app_localnet() {
    test_util::assert_localnet_available();
    let cfg = localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_RECEIVE])
        .expect("fund client account; install algokit and start localnet");

    let creator_ops = ops_from_mnemonic(ADDRESS_APP_CREATOR, PASSPHRASE_APP_CREATOR, cfg.clone());

    // Old app (with the Bingle$ ASA): a client registers here before it is superseded.
    let (old_app, asset_id) = deploy_bingle_app_and_asset(&creator_ops, "BLKOLD", 1_000_000);
    let handle = "blocked_one";
    register_client_on_blockchain(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, handle, old_app, asset_id, &creator_ops, cfg.clone());

    // The successor pointer starts empty.
    let creator_bgl = AlgoBingle::new(creator_ops.clone(), old_app, asset_id);
    assert!(
        creator_bgl.successor_app(old_app).expect("successor_app before").is_none(),
        "a freshly deployed app must not be superseded"
    );

    // Deploy the replacement and mark the old app superseded by it.
    let new_app = deploy_bingle_app(&creator_ops);
    creator_bgl.set_successor_app(old_app, new_app).expect("set_successor_app");

    assert_eq!(
        creator_bgl.successor_app(old_app).expect("successor_app after"),
        Some(new_app),
        "successor_app must read back the app the old one was superseded by"
    );

    // Hard block: user-facing state-changing methods now reject on-chain. A fresh buy_bingle
    // against the superseded old app must fail.
    let client_ops = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());
    let client_old_bgl = AlgoBingle::new(client_ops.clone(), old_app, asset_id);
    assert!(
        client_old_bgl.buy_bingle(old_app, asset_id, 1).is_err(),
        "buy_bingle against a superseded app must be rejected on-chain"
    );

    // Migration still works: the new app blesses the (superseded) old app as a predecessor, and
    // the client migrates its local state out of the old app in one hop. Blocking the old app's
    // user methods never impedes migration, which only reads the old app's local state.
    AlgoBingle::new(creator_ops.clone(), new_app, 0)
        .set_predecessor_app(new_app, old_app)
        .expect("set_predecessor_app(new_app, old_app)");

    let client_new_bgl = AlgoBingle::new(client_ops.clone(), new_app, asset_id);
    let tx = client_new_bgl.ensure_local_migrated(new_app).expect("ensure_local_migrated from superseded old app");
    assert!(tx.is_some(), "expected a migration transaction out of the superseded old app");

    wait_for_handle(&client_ops, new_app, ADDRESS_RECEIVE, handle);

    // The successor app itself is not superseded, so it reports no successor.
    assert!(
        client_new_bgl.successor_app(new_app).expect("successor_app(new_app)").is_none(),
        "the successor app must not itself be marked superseded"
    );
}
