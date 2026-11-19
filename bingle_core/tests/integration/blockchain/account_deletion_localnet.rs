// Localnet integration tests for the on-chain half of account deletion (issue #100).
//
// Exercises AlgoBingle::leave_app_lineage against the real BingleDapp on algokit localnet: a
// deleted account's handle is released so another account can register it, an account migrated
// from an ancestor app is cleared there too (so the handle can't be migrated back), and a second
// call is a no-op.

use std::time::{Duration, Instant};

use bingle_core::blockchain::algo_bingle::AlgoBingle;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;
use test_util::{
    ADDRESS_APP_CREATOR, ADDRESS_RECEIVE, ADDRESS_SPEND, PASSPHRASE_APP_CREATOR,
    PASSPHRASE_RECEIVE, PASSPHRASE_SPEND, deploy_bingle_app, deploy_bingle_app_and_asset,
    localnet_config, ops_from_mnemonic, register_client_on_blockchain,
};

/// Poll the indexer until `handle` resolves to `expected` on `bgl`'s app, or panic.
fn wait_for_handle_owner(bgl: &AlgoBingle, handle: &str, expected: Option<&str>) {
    let start = Instant::now();
    let mut last = None;
    while start.elapsed() < Duration::from_secs(30) {
        if let Ok(owner) = bgl.handle_lookup(handle) {
            if owner.as_deref() == expected {
                return;
            }
            last = owner;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    panic!("handle '{handle}' owner stayed {last:?}, expected {expected:?}");
}

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn leave_app_lineage_releases_the_handle_localnet() {
    test_util::assert_localnet_available();
    let cfg = localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_RECEIVE, ADDRESS_SPEND])
        .expect("fund client accounts; install algokit and start localnet");

    let creator_ops = ops_from_mnemonic(ADDRESS_APP_CREATOR, PASSPHRASE_APP_CREATOR, cfg.clone());
    let (app_id, asset_id) = deploy_bingle_app_and_asset(&creator_ops, "DELETE", 1_000_000);
    let handle = "deleted_one";
    register_client_on_blockchain(
        ADDRESS_RECEIVE,
        PASSPHRASE_RECEIVE,
        handle,
        app_id,
        asset_id,
        &creator_ops,
        cfg.clone(),
    );

    let leaver_ops = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());
    let leaver = AlgoBingle::new(leaver_ops.clone(), app_id, asset_id);
    wait_for_handle_owner(&leaver, handle, Some(ADDRESS_RECEIVE));

    let cleared = leaver.leave_app_lineage(app_id).expect("leave_app_lineage");
    assert_eq!(cleared, vec![app_id], "only the current app holds state");
    assert!(
        leaver_ops
            .local_state_for_account(app_id, ADDRESS_RECEIVE)
            .expect("read local state")
            .is_none(),
        "the account no longer holds local state on the app"
    );
    // The registration-fee asset stays with the user.
    assert!(
        leaver_ops
            .is_account_opted_in_to_asset(ADDRESS_RECEIVE, asset_id)
            .expect("read asset holding"),
        "the Bingle$ holding is left in place"
    );

    // A second call has nothing left to clear.
    assert!(
        leaver
            .leave_app_lineage(app_id)
            .expect("second leave_app_lineage")
            .is_empty(),
        "leaving again is a no-op"
    );

    // The handle is free at once: another account can register it.
    wait_for_handle_owner(&leaver, handle, None);
    register_client_on_blockchain(
        ADDRESS_SPEND,
        PASSPHRASE_SPEND,
        handle,
        app_id,
        asset_id,
        &creator_ops,
        cfg.clone(),
    );
    wait_for_handle_owner(&leaver, handle, Some(ADDRESS_SPEND));
}

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn leave_app_lineage_clears_ancestor_apps_localnet() {
    test_util::assert_localnet_available();
    let cfg = localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_RECEIVE])
        .expect("fund client account; install algokit and start localnet");

    let creator_ops = ops_from_mnemonic(ADDRESS_APP_CREATOR, PASSPHRASE_APP_CREATOR, cfg.clone());

    // Register on the old app, then migrate to a new app that blesses it.
    let (old_app, asset_id) = deploy_bingle_app_and_asset(&creator_ops, "DELOLD", 1_000_000);
    let handle = "deleted_migrant";
    register_client_on_blockchain(
        ADDRESS_RECEIVE,
        PASSPHRASE_RECEIVE,
        handle,
        old_app,
        asset_id,
        &creator_ops,
        cfg.clone(),
    );
    let new_app = deploy_bingle_app(&creator_ops);
    AlgoBingle::new(creator_ops.clone(), new_app, 0)
        .set_predecessor_app(new_app, old_app)
        .expect("set_predecessor_app");

    let client_ops = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());
    let client = AlgoBingle::new(client_ops.clone(), new_app, asset_id);
    assert!(
        client
            .ensure_local_migrated(new_app)
            .expect("ensure_local_migrated")
            .is_some(),
        "the client migrates to the new app"
    );

    // Deletion clears both apps, the current one first.
    let cleared = client.leave_app_lineage(new_app).expect("leave_app_lineage");
    assert_eq!(cleared, vec![new_app, old_app]);
    for app in [new_app, old_app] {
        assert!(
            client_ops
                .local_state_for_account(app, ADDRESS_RECEIVE)
                .expect("read local state")
                .is_none(),
            "no local state left on app {app}"
        );
    }

    // Nothing is left to migrate back.
    assert!(
        client
            .ensure_local_migrated(new_app)
            .expect("ensure_local_migrated after deletion")
            .is_none(),
        "a deleted account has nothing to migrate"
    );
}
