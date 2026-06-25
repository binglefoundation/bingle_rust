/// Integration tests for `AlgoBingle::deploy_app_and_asset`.
///
/// All combinations of (new_app, new_asset) are exercised against a live algokit localnet.
/// Every test uses the full set of granular named accounts (APP_CREATOR, APP_ADMIN,
/// APP_WITHDRAWER, ASSET_CREATOR, ASSET_RESERVE), all at distinct addresses.
///
/// Tests:
///   1. Both new  — deploys app from TEAL + creates ASA
///   2. New app, existing asset — deploys app, reuses ASA, transfers old balance
///   3. Existing app, new asset — reuses app, creates new ASA
///   4. Both existing — reuses app + ASA, only reconfigures clawback/reserve
///
/// Each test verifies:
///   - returned ids are non-zero
///   - the app account is opted-in to the ASA
///   - the ASA clawback address equals the app account address
///   - for combinations 1 and 3, the ASA reserve equals the ASSET_RESERVE account
///   - for combination 2, the old-app balance is transferred to the new app
use rust_comms::algo_ops::{AlgoChainConfig, AlgoOps};
use rust_comms::blockchain::algo_bingle::{
    AlgoBingle, ACCOUNT_APP_ADMIN, ACCOUNT_APP_WITHDRAWER, ACCOUNT_ASSET_CREATOR,
    ACCOUNT_ASSET_RESERVE,
};
use serial_test::serial;
use std::collections::HashMap;

use crate::util::test_util;
use test_util::init_test_logging_with_filter;

const TEAL_DIR: &str = "dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp";

fn setup() -> (AlgoChainConfig, AlgoOps, HashMap<String, AlgoOps>) {
    test_util::assert_localnet_available();
    let cfg = test_util::localnet_config();
    crate::blockchain_users::ensure_funded(&cfg)
        .expect("fund predefined accounts; ensure algokit localnet is running");
    let creator_ops = test_util::ops_from_mnemonic(
        crate::blockchain_users::ADDRESS_APP_CREATOR,
        crate::blockchain_users::PASSPHRASE_APP_CREATOR,
        cfg.clone(),
    );
    let accounts = make_accounts(&cfg);
    (cfg, creator_ops, accounts)
}

fn make_accounts(cfg: &AlgoChainConfig) -> HashMap<String, AlgoOps> {
    use crate::blockchain_users::{
        ADDRESS_APP_ADMIN, ADDRESS_APP_WITHDRAWER, ADDRESS_ASSET_CREATOR, ADDRESS_ASSET_RESERVE,
        PASSPHRASE_APP_ADMIN, PASSPHRASE_APP_WITHDRAWER, PASSPHRASE_ASSET_CREATOR,
        PASSPHRASE_ASSET_RESERVE,
    };
    let mut accounts = HashMap::new();
    accounts.insert(
        ACCOUNT_APP_ADMIN.to_string(),
        test_util::ops_from_mnemonic(ADDRESS_APP_ADMIN, PASSPHRASE_APP_ADMIN, cfg.clone()),
    );
    accounts.insert(
        ACCOUNT_APP_WITHDRAWER.to_string(),
        test_util::ops_from_mnemonic(ADDRESS_APP_WITHDRAWER, PASSPHRASE_APP_WITHDRAWER, cfg.clone()),
    );
    accounts.insert(
        ACCOUNT_ASSET_CREATOR.to_string(),
        test_util::ops_from_mnemonic(ADDRESS_ASSET_CREATOR, PASSPHRASE_ASSET_CREATOR, cfg.clone()),
    );
    accounts.insert(
        ACCOUNT_ASSET_RESERVE.to_string(),
        test_util::ops_from_mnemonic(ADDRESS_ASSET_RESERVE, PASSPHRASE_ASSET_RESERVE, cfg.clone()),
    );
    accounts
}

fn verify_app_opted_into_asset(ops: &AlgoOps, app_id: u64, asset_id: u64) {
    let app_addr = ops.contract_address(app_id).expect("app contract address");
    let opted_in = ops
        .is_account_opted_in_to_asset(&app_addr, asset_id)
        .expect("is_account_opted_in_to_asset");
    assert!(opted_in, "app {} should be opted in to asset {}", app_id, asset_id);
}

fn verify_clawback_is_app(ops: &AlgoOps, app_id: u64, asset_id: u64) {
    use algonaut::core::AssetId;
    let client = ops.algod_client().expect("algod client");
    let asset_info = ops
        .algod_call(|| client.asset(AssetId(asset_id)))
        .expect("fetch asset info");
    let v = serde_json::to_value(&asset_info).expect("serialize asset info");
    let clawback = v
        .get("params")
        .and_then(|p| p.get("clawback").and_then(|x| x.as_str()))
        .expect("clawback field in asset params");
    let app_addr = ops.contract_address(app_id).expect("app contract address");
    assert_eq!(
        clawback, app_addr,
        "ASA clawback should equal app {} address {}",
        app_id, app_addr
    );
}

fn verify_reserve(ops: &AlgoOps, asset_id: u64, expected_reserve: &str) {
    use algonaut::core::AssetId;
    let client = ops.algod_client().expect("algod client");
    let asset_info = ops
        .algod_call(|| client.asset(AssetId(asset_id)))
        .expect("fetch asset info");
    let v = serde_json::to_value(&asset_info).expect("serialize asset info");
    let reserve = v
        .get("params")
        .and_then(|p| p.get("reserve").and_then(|x| x.as_str()))
        .expect("reserve field in asset params");
    assert_eq!(reserve, expected_reserve, "ASA reserve should equal {}", expected_reserve);
}

// ── Test 1: both new ────────────────────────────────────────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_both_new() {
    unsafe { std::env::set_var("BINGLE_ALGO_DEBUG", "true"); }
    init_test_logging_with_filter("info");
    let (_cfg, creator_ops, accounts) = setup();

    let ab = AlgoBingle::new(creator_ops.clone(), 0, 0);
    let (app_id, asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            true, // new_app
            true, // new_asset
            None,
            None,
            "BINGLE",
            1_000_000,
            &accounts,
        )
        .expect("deploy_app_and_asset (both new)");

    assert!(app_id > 0, "app_id must be non-zero");
    assert!(asset_id > 0, "asset_id must be non-zero");
    verify_app_opted_into_asset(&creator_ops, app_id, asset_id);
    verify_clawback_is_app(&creator_ops, app_id, asset_id);
    verify_reserve(&creator_ops, asset_id, crate::blockchain_users::ADDRESS_ASSET_RESERVE);
}

// ── Test 2: new app, existing asset (balance transfer) ─────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_new_app_existing_asset() {
    unsafe { std::env::set_var("BINGLE_ALGO_DEBUG", "true"); }
    init_test_logging_with_filter("info");
    let (_cfg, creator_ops, accounts) = setup();

    // Create the initial deployment so we have an app+asset to migrate FROM.
    let (old_app_id, asset_id) =
        test_util::deploy_bingle_app_and_asset(&creator_ops, "BINGLE", 1_000_000);

    // Fund the old app with some units so we can verify the balance transfer.
    let old_app_addr = creator_ops.contract_address(old_app_id).expect("old app addr");
    creator_ops
        .send_asset(asset_id, 50, &old_app_addr)
        .expect("fund old app with ASA");

    // Deploy a new app while reusing the existing asset.
    let ab = AlgoBingle::new(creator_ops.clone(), old_app_id, asset_id);
    let (new_app_id, same_asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            true,  // new_app
            false, // reuse existing asset
            None,
            Some(asset_id),
            "BINGLE",
            1_000_000,
            &accounts,
        )
        .expect("deploy_app_and_asset (new app, existing asset)");

    assert!(new_app_id > 0 && new_app_id != old_app_id, "a new app should have been created");
    assert_eq!(same_asset_id, asset_id, "asset id should be unchanged");

    verify_app_opted_into_asset(&creator_ops, new_app_id, asset_id);
    verify_clawback_is_app(&creator_ops, new_app_id, asset_id);

    // Old app's 50-unit balance should have been transferred to the new app.
    let new_app_addr = creator_ops.contract_address(new_app_id).expect("new app addr");
    let new_balance = creator_ops.asset_holding(&new_app_addr, asset_id).expect("new app balance");
    assert!(
        new_balance >= 50,
        "new app should hold at least 50 units after balance transfer, got {}",
        new_balance
    );

    let old_balance = creator_ops.asset_holding(&old_app_addr, asset_id).expect("old app balance");
    assert_eq!(old_balance, 0, "old app should have zero balance after transfer, got {}", old_balance);
}

// ── Test 3: existing app, new asset ────────────────────────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_existing_app_new_asset() {
    unsafe { std::env::set_var("BINGLE_ALGO_DEBUG", "true"); }
    init_test_logging_with_filter("info");
    let (_cfg, creator_ops, accounts) = setup();

    let (app_id, old_asset_id) =
        test_util::deploy_bingle_app_and_asset(&creator_ops, "BINGLE_OLD", 500_000);

    // Keep the same app but issue a brand-new asset.
    let ab = AlgoBingle::new(creator_ops.clone(), app_id, 0);
    let (same_app_id, new_asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            false, // reuse existing app
            true,  // new_asset
            Some(app_id),
            None,
            "BINGLE_V2",
            2_000_000,
            &accounts,
        )
        .expect("deploy_app_and_asset (existing app, new asset)");

    assert_eq!(same_app_id, app_id, "app id should be unchanged");
    assert!(new_asset_id > 0 && new_asset_id != old_asset_id, "a new asset should have been created");

    verify_app_opted_into_asset(&creator_ops, app_id, new_asset_id);
    verify_clawback_is_app(&creator_ops, app_id, new_asset_id);
    verify_reserve(&creator_ops, new_asset_id, crate::blockchain_users::ADDRESS_ASSET_RESERVE);
}

// ── Test 4: both existing ───────────────────────────────────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_both_existing() {
    unsafe { std::env::set_var("BINGLE_ALGO_DEBUG", "true"); }
    init_test_logging_with_filter("info");
    let (_cfg, creator_ops, accounts) = setup();

    let (app_id, asset_id) =
        test_util::deploy_bingle_app_and_asset(&creator_ops, "BINGLE", 1_000_000);

    // Re-run with explicit ids — should be idempotent.
    let ab = AlgoBingle::new(creator_ops.clone(), app_id, asset_id);
    let (same_app_id, same_asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            false, // reuse existing app
            false, // reuse existing asset
            Some(app_id),
            Some(asset_id),
            "BINGLE",
            1_000_000,
            &accounts,
        )
        .expect("deploy_app_and_asset (both existing)");

    assert_eq!(same_app_id, app_id, "app id should be unchanged");
    assert_eq!(same_asset_id, asset_id, "asset id should be unchanged");

    verify_app_opted_into_asset(&creator_ops, app_id, asset_id);
    verify_clawback_is_app(&creator_ops, app_id, asset_id);
}

// ── Validation: accounts map must be complete and unique ────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_missing_required_account_fails() {
    unsafe { std::env::set_var("BINGLE_ALGO_DEBUG", "true"); }
    init_test_logging_with_filter("info");
    let (_cfg, creator_ops, accounts) = setup();

    let required = [
        ACCOUNT_APP_ADMIN,
        ACCOUNT_APP_WITHDRAWER,
        ACCOUNT_ASSET_CREATOR,
        ACCOUNT_ASSET_RESERVE,
    ];
    for missing in required {
        let mut partial = accounts.clone();
        partial.remove(missing);
        let ab = AlgoBingle::new(creator_ops.clone(), 0, 0);
        let result = ab.deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            true, true, None, None, "BINGLE", 1_000_000, &partial,
        );
        assert!(result.is_err(), "should fail when '{}' is missing", missing);
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains(missing),
            "error should name the missing role '{}', got: {}",
            missing, msg
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_duplicate_address_fails() {
    unsafe { std::env::set_var("BINGLE_ALGO_DEBUG", "true"); }
    init_test_logging_with_filter("info");
    let (cfg, creator_ops, mut accounts) = setup();

    // Give ASSET_RESERVE the same address as ASSET_CREATOR — two roles, one account.
    accounts.insert(
        ACCOUNT_ASSET_RESERVE.to_string(),
        test_util::ops_from_mnemonic(
            crate::blockchain_users::ADDRESS_ASSET_CREATOR,
            crate::blockchain_users::PASSPHRASE_ASSET_CREATOR,
            cfg,
        ),
    );

    let ab = AlgoBingle::new(creator_ops, 0, 0);
    let result = ab.deploy_app_and_asset(
        std::path::Path::new(TEAL_DIR),
        true, true, None, None, "BINGLE", 1_000_000, &accounts,
    );
    assert!(result.is_err(), "should fail when two roles share the same address");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("not unique"),
        "error should mention 'not unique', got: {}",
        msg
    );
}
