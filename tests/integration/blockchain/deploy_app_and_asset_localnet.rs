/// Integration tests for `AlgoBingle::deploy_app_and_asset`.
///
/// Four combinations of (new_app, new_asset) are exercised against a live algokit localnet:
///   1. Both new  — deploys app from TEAL + creates ASA
///   2. New app, existing asset — deploys app, reuses ASA, transfers old balance
///   3. Existing app, new asset — reuses app, creates new ASA
///   4. Both existing — reuses app + ASA, only reconfigures clawback/reserve
///
/// Each test verifies:
///   - returned ids are non-zero
///   - the app account is opted-in to the ASA
///   - the ASA clawback address equals the app account address
///   - for combination 2, old-app balance is transferred to the new app
use rust_comms::blockchain::algo_bingle::AlgoBingle;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;

const TEAL_DIR: &str = "dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp";

fn fund_accounts() {
    let cfg = test_util::localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE],
    )
    .expect("ensure localnet accounts funded");
}

fn creator() -> rust_comms::algo_ops::AlgoOps {
    test_util::ops_from_mnemonic(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        test_util::localnet_config(),
    )
}

fn verify_app_opted_into_asset(ops: &rust_comms::algo_ops::AlgoOps, app_id: u64, asset_id: u64) {
    let app_addr = ops.contract_address(app_id).expect("app contract address");
    let opted_in = ops
        .is_account_opted_in_to_asset(&app_addr, asset_id)
        .expect("is_account_opted_in_to_asset");
    assert!(opted_in, "app {} should be opted in to asset {}", app_id, asset_id);
}

fn verify_clawback_is_app(ops: &rust_comms::algo_ops::AlgoOps, app_id: u64, asset_id: u64) {
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

// ── Test 1: both new ────────────────────────────────────────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_both_new() {
    test_util::assert_localnet_available();
    fund_accounts();

    let ops = creator();
    // AlgoBingle with zero ids — no pre-existing app or asset.
    let ab = AlgoBingle::new(ops.clone(), 0, 0);

    let (app_id, asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            true,  // new_app
            true,  // new_asset
            None,
            None,
            "BINGLE",
            1_000_000,
        )
        .expect("deploy_app_and_asset (both new)");

    assert!(app_id > 0, "app_id must be non-zero");
    assert!(asset_id > 0, "asset_id must be non-zero");
    verify_app_opted_into_asset(&ops, app_id, asset_id);
    verify_clawback_is_app(&ops, app_id, asset_id);
}

// ── Test 2: new app, existing asset (balance transfer) ─────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_new_app_existing_asset() {
    test_util::assert_localnet_available();
    fund_accounts();

    let ops = creator();

    // Create the initial deployment so we have an app+asset to migrate FROM.
    let (old_app_id, asset_id) =
        test_util::deploy_bingle_app_and_asset(&ops, "BINGLE", 1_000_000);

    // Fund the old app with some units so we can verify the balance transfer.
    let old_app_addr = ops.contract_address(old_app_id).expect("old app addr");
    ops.send_asset(asset_id, 50, &old_app_addr)
        .expect("fund old app with ASA");

    // Deploy a new app while reusing the existing asset.
    let ab = AlgoBingle::new(ops.clone(), old_app_id, asset_id);
    let (new_app_id, same_asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            true,  // new_app
            false, // reuse existing asset
            None,
            Some(asset_id),
            "BINGLE",
            1_000_000,
        )
        .expect("deploy_app_and_asset (new app, existing asset)");

    assert!(new_app_id > 0 && new_app_id != old_app_id, "a new app should have been created");
    assert_eq!(same_asset_id, asset_id, "asset id should be unchanged");

    verify_app_opted_into_asset(&ops, new_app_id, asset_id);
    verify_clawback_is_app(&ops, new_app_id, asset_id);

    // Old app's 50-unit balance should have been transferred to the new app.
    let new_app_addr = ops.contract_address(new_app_id).expect("new app addr");
    let new_balance = ops.asset_holding(&new_app_addr, asset_id).expect("new app balance");
    // New app was also funded by opt_in_app_to_asset (0 units), so at minimum 50.
    assert!(
        new_balance >= 50,
        "new app should hold at least 50 units after balance transfer, got {}",
        new_balance
    );

    let old_balance = ops.asset_holding(&old_app_addr, asset_id).expect("old app balance");
    assert_eq!(old_balance, 0, "old app should have zero balance after transfer, got {}", old_balance);
}

// ── Test 3: existing app, new asset ────────────────────────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_existing_app_new_asset() {
    test_util::assert_localnet_available();
    fund_accounts();

    let ops = creator();
    let (app_id, _old_asset_id) =
        test_util::deploy_bingle_app_and_asset(&ops, "BINGLE_OLD", 500_000);

    // Keep the same app but issue a brand-new asset.
    let ab = AlgoBingle::new(ops.clone(), app_id, 0);
    let (same_app_id, new_asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            false, // reuse existing app
            true,  // new_asset
            Some(app_id),
            None,
            "BINGLE_V2",
            2_000_000,
        )
        .expect("deploy_app_and_asset (existing app, new asset)");

    assert_eq!(same_app_id, app_id, "app id should be unchanged");
    assert!(new_asset_id > 0 && new_asset_id != _old_asset_id, "a new asset should have been created");

    verify_app_opted_into_asset(&ops, app_id, new_asset_id);
    verify_clawback_is_app(&ops, app_id, new_asset_id);
}

// ── Test 4: both existing ───────────────────────────────────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_both_existing() {
    test_util::assert_localnet_available();
    fund_accounts();

    let ops = creator();
    let (app_id, asset_id) =
        test_util::deploy_bingle_app_and_asset(&ops, "BINGLE", 1_000_000);

    // Re-run with explicit ids — should be idempotent.
    let ab = AlgoBingle::new(ops.clone(), app_id, asset_id);
    let (same_app_id, same_asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            false, // reuse existing app
            false, // reuse existing asset
            Some(app_id),
            Some(asset_id),
            "BINGLE",
            1_000_000,
        )
        .expect("deploy_app_and_asset (both existing)");

    assert_eq!(same_app_id, app_id, "app id should be unchanged");
    assert_eq!(same_asset_id, asset_id, "asset id should be unchanged");

    verify_app_opted_into_asset(&ops, app_id, asset_id);
    verify_clawback_is_app(&ops, app_id, asset_id);
}
