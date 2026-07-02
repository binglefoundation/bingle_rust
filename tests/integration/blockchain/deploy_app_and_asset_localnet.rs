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
///   - ALGO balances decrease by exactly (tx_count × MIN_FEE) for each signing account
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
const MIN_FEE: u64 = 1_000;         // µAlgos per transaction on localnet
const APP_FUND: u64 = 3_210_000;    // µAlgos sent from APP_CREATOR to new app account on deploy
const MIN_BALANCE_WITH_ASA: u64 = 200_000; // minimum balance for an account opted in to 1 ASA

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

fn microalgos(ops: &AlgoOps) -> u64 {
    (ops.account_balance()
        .expect("account_balance")
        .expect("account exists")
        * 1_000_000.0)
        .round() as u64
}

fn app_microalgos(ops: &AlgoOps, app_id: u64) -> u64 {
    let addr = ops.contract_address(app_id).expect("app contract address");
    ops.microalgos_at(&addr).expect("app account balance")
}

struct BalanceSnapshot {
    app_creator: u64,
    app_admin: u64,
    app_withdrawer: u64,
    asset_creator: u64,
    asset_reserve: u64,
}

fn snapshot(creator_ops: &AlgoOps, accounts: &HashMap<String, AlgoOps>) -> BalanceSnapshot {
    BalanceSnapshot {
        app_creator:    microalgos(creator_ops),
        app_admin:      microalgos(&accounts[ACCOUNT_APP_ADMIN]),
        app_withdrawer: microalgos(&accounts[ACCOUNT_APP_WITHDRAWER]),
        asset_creator:  microalgos(&accounts[ACCOUNT_ASSET_CREATOR]),
        asset_reserve:  microalgos(&accounts[ACCOUNT_ASSET_RESERVE]),
    }
}

fn assert_algo_spent(label: &str, before: u64, after: u64, expected_ua: u64) {
    let spent = before as i64 - after as i64;
    assert_eq!(
        spent,
        expected_ua as i64,
        "{}: expected {}uA spent, got {}uA",
        label, expected_ua, spent,
    );
}

/// Run deploy_app_and_asset(new_app=true, new_asset=true) with the full granular accounts.
/// Used by tests that need an initial app+asset where ASSET_CREATOR is the asset manager.
fn deploy_initial(
    creator_ops: &AlgoOps,
    accounts: &HashMap<String, AlgoOps>,
    asset_name: &str,
    total_units: u64,
) -> (u64, u64) {
    let ab = AlgoBingle::new(creator_ops.clone(), 0, 0);
    ab.deploy_app_and_asset(
        std::path::Path::new(TEAL_DIR),
        true, true, None, None, asset_name, total_units, 0, accounts,
    )
    .expect("deploy_initial")
}

// ── Test 1: both new ────────────────────────────────────────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_both_new() {
    unsafe { std::env::set_var("BINGLE_ALGO_DEBUG", "true"); }
    init_test_logging_with_filter("info");
    let (_cfg, creator_ops, accounts) = setup();

    const INITIAL_HOT_BINGLE: u64 = 100;
    let ab = AlgoBingle::new(creator_ops.clone(), 0, 0);
    let before = snapshot(&creator_ops, &accounts);
    let (app_id, asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            true, // new_app
            true, // new_asset
            None,
            None,
            "BINGLE",
            1_000_000,
            INITIAL_HOT_BINGLE,
            &accounts,
        )
        .expect("deploy_app_and_asset (both new)");
    let after = snapshot(&creator_ops, &accounts);

    assert!(app_id > 0, "app_id must be non-zero");
    assert!(asset_id > 0, "asset_id must be non-zero");
    verify_app_opted_into_asset(&creator_ops, app_id, asset_id);
    verify_clawback_is_app(&creator_ops, app_id, asset_id);
    verify_reserve(&creator_ops, asset_id, crate::blockchain_users::ADDRESS_ASSET_RESERVE);
    // APP_CREATOR:  create_app(1 fee) + send_algo to app(1 fee + APP_FUND)
    // APP_ADMIN:    set_bingle_price(1) + opt_in_app_to_asset(1)
    // ASSET_CREATOR: create_asset(1) + send_asset initial_hot_bingle(1)
    // APP_WITHDRAWER, ASSET_RESERVE: 1 fee each
    assert_algo_spent("APP_CREATOR",    before.app_creator,    after.app_creator,    2 * MIN_FEE + APP_FUND);
    assert_algo_spent("APP_ADMIN",      before.app_admin,      after.app_admin,      2 * MIN_FEE);
    assert_algo_spent("ASSET_CREATOR",  before.asset_creator,  after.asset_creator,  2 * MIN_FEE);
    assert_algo_spent("APP_WITHDRAWER", before.app_withdrawer, after.app_withdrawer, MIN_FEE);
    assert_algo_spent("ASSET_RESERVE",  before.asset_reserve,  after.asset_reserve,  MIN_FEE);

    let app_addr = creator_ops.contract_address(app_id).expect("app contract address");
    let app_asa_balance = creator_ops.asset_holding(&app_addr, asset_id).expect("app asset holding");
    assert_eq!(app_asa_balance, INITIAL_HOT_BINGLE, "app should hold initial_hot_bingle units of the new asset");
    // New app: funded with APP_FUND, pays 1 inner fee for ASA opt-in.
    assert_eq!(app_microalgos(&creator_ops, app_id), APP_FUND - MIN_FEE, "app ALGO balance after deploy");
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
    // Use deploy_initial (full granular accounts) so ASSET_CREATOR is the asset manager.
    let (old_app_id, asset_id) = deploy_initial(&creator_ops, &accounts, "BINGLE", 1_000_000);

    // Fund the old app with some units so we can verify the balance transfer.
    // ASSET_CREATOR holds the supply after create_asset_configured.
    let old_app_addr = creator_ops.contract_address(old_app_id).expect("old app addr");
    accounts[ACCOUNT_ASSET_CREATOR]
        .send_asset(asset_id, 50, &old_app_addr)
        .expect("fund old app with ASA");

    // Deploy a new app while reusing the existing asset.
    let ab = AlgoBingle::new(creator_ops.clone(), old_app_id, asset_id);
    let before = snapshot(&creator_ops, &accounts);
    let (new_app_id, same_asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            true,  // new_app
            false, // reuse existing asset
            None,
            Some(asset_id),
            "BINGLE",
            1_000_000,
            0,     // initial_hot_bingle ignored when reusing existing asset
            &accounts,
        )
        .expect("deploy_app_and_asset (new app, existing asset)");
    let after = snapshot(&creator_ops, &accounts);

    assert!(new_app_id > 0 && new_app_id != old_app_id, "a new app should have been created");
    assert_eq!(same_asset_id, asset_id, "asset id should be unchanged");

    verify_app_opted_into_asset(&creator_ops, new_app_id, asset_id);
    verify_clawback_is_app(&creator_ops, new_app_id, asset_id);
    // APP_CREATOR:   create_app(1 fee) + send_algo to app(1 fee + APP_FUND) + migrate_reserve(1 fee)
    // APP_ADMIN:     set_bingle_price(1) + opt_in_app_to_asset(1) — new app not yet opted in
    // ASSET_CREATOR: set_clawback(1) + transfer_old_balance[UpdateAsset(1)+Clawback(1)+set_clawback(1)]
    // APP_WITHDRAWER, ASSET_RESERVE: already opted in by deploy_initial → 0 fees
    assert_algo_spent("APP_CREATOR",    before.app_creator,    after.app_creator,    3 * MIN_FEE + APP_FUND);
    assert_algo_spent("APP_ADMIN",      before.app_admin,      after.app_admin,      2 * MIN_FEE);
    assert_algo_spent("ASSET_CREATOR",  before.asset_creator,  after.asset_creator,  4 * MIN_FEE);
    assert_algo_spent("APP_WITHDRAWER", before.app_withdrawer, after.app_withdrawer, 0);
    assert_algo_spent("ASSET_RESERVE",  before.asset_reserve,  after.asset_reserve,  0);

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

    // migrate_reserve leaves the old app at exactly its minimum balance (1 ASA opt-in).
    assert_eq!(
        app_microalgos(&creator_ops, old_app_id),
        MIN_BALANCE_WITH_ASA,
        "old app ALGO balance should equal minimum balance after migration"
    );

    // New app: funded with APP_FUND, pays 1 inner fee for ASA opt-in, then receives
    // (old_app_balance - MIN_BALANCE_WITH_ASA - MIN_FEE) from migrate_reserve.
    // Old app had APP_FUND - MIN_FEE after deploy_initial's opt-in inner tx.
    let migrated = (APP_FUND - MIN_FEE) - MIN_BALANCE_WITH_ASA - MIN_FEE;
    assert_eq!(
        app_microalgos(&creator_ops, new_app_id),
        (APP_FUND - MIN_FEE) + migrated,
        "new app ALGO balance after deploy + migration"
    );
}

// ── Test 3: existing app, new asset ────────────────────────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_existing_app_new_asset() {
    unsafe { std::env::set_var("BINGLE_ALGO_DEBUG", "true"); }
    init_test_logging_with_filter("info");
    let (_cfg, creator_ops, accounts) = setup();

    // Use deploy_initial so ASSET_CREATOR is the asset manager for the initial asset.
    let (app_id, old_asset_id) = deploy_initial(&creator_ops, &accounts, "BINGLE_OLD", 500_000);

    // Keep the same app but issue a brand-new asset.
    let ab = AlgoBingle::new(creator_ops.clone(), app_id, 0);
    let before = snapshot(&creator_ops, &accounts);
    const INITIAL_HOT_BINGLE: u64 = 250;
    let (same_app_id, new_asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            false, // reuse existing app
            true,  // new_asset
            Some(app_id),
            None,
            "BINGLE_V2",
            2_000_000,
            INITIAL_HOT_BINGLE,
            &accounts,
        )
        .expect("deploy_app_and_asset (existing app, new asset)");
    let after = snapshot(&creator_ops, &accounts);

    assert_eq!(same_app_id, app_id, "app id should be unchanged");
    assert!(new_asset_id > 0 && new_asset_id != old_asset_id, "a new asset should have been created");

    verify_app_opted_into_asset(&creator_ops, app_id, new_asset_id);
    verify_clawback_is_app(&creator_ops, app_id, new_asset_id);
    verify_reserve(&creator_ops, new_asset_id, crate::blockchain_users::ADDRESS_ASSET_RESERVE);
    // APP_CREATOR: nothing (existing app, no migrate)
    // APP_ADMIN:   opt_in_app_to_asset(1) — new asset so not yet opted in; no set_bingle_price
    // ASSET_CREATOR: create_asset(1) + send_asset initial_hot_bingle(1)
    // APP_WITHDRAWER, ASSET_RESERVE: 1 fee each
    assert_algo_spent("APP_CREATOR",    before.app_creator,    after.app_creator,    0);
    assert_algo_spent("APP_ADMIN",      before.app_admin,      after.app_admin,      MIN_FEE);
    assert_algo_spent("ASSET_CREATOR",  before.asset_creator,  after.asset_creator,  2 * MIN_FEE);
    assert_algo_spent("APP_WITHDRAWER", before.app_withdrawer, after.app_withdrawer, MIN_FEE);
    assert_algo_spent("ASSET_RESERVE",  before.asset_reserve,  after.asset_reserve,  MIN_FEE);

    let app_addr = creator_ops.contract_address(app_id).expect("app contract address");
    let app_asa_balance = creator_ops.asset_holding(&app_addr, new_asset_id).expect("app asset holding");
    assert_eq!(app_asa_balance, INITIAL_HOT_BINGLE, "app should hold initial_hot_bingle units of the new asset");
    // Existing app: had APP_FUND - MIN_FEE after deploy_initial, pays 1 more inner fee for new ASA opt-in.
    assert_eq!(app_microalgos(&creator_ops, app_id), APP_FUND - 2 * MIN_FEE, "app ALGO balance after new asset deploy");
}

// ── Test 4: both existing ───────────────────────────────────────────────────

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn deploy_app_and_asset_both_existing() {
    unsafe { std::env::set_var("BINGLE_ALGO_DEBUG", "true"); }
    init_test_logging_with_filter("info");
    let (_cfg, creator_ops, accounts) = setup();

    // Use deploy_initial so ASSET_CREATOR is the asset manager.
    let (app_id, asset_id) = deploy_initial(&creator_ops, &accounts, "BINGLE", 1_000_000);

    // Re-run with explicit ids — should be idempotent.
    let ab = AlgoBingle::new(creator_ops.clone(), app_id, asset_id);
    let before = snapshot(&creator_ops, &accounts);
    let (same_app_id, same_asset_id) = ab
        .deploy_app_and_asset(
            std::path::Path::new(TEAL_DIR),
            false, // reuse existing app
            false, // reuse existing asset
            Some(app_id),
            Some(asset_id),
            "BINGLE",
            1_000_000,
            0,     // initial_hot_bingle ignored when reusing existing asset
            &accounts,
        )
        .expect("deploy_app_and_asset (both existing)");
    let after = snapshot(&creator_ops, &accounts);

    assert_eq!(same_app_id, app_id, "app id should be unchanged");
    assert_eq!(same_asset_id, asset_id, "asset id should be unchanged");

    verify_app_opted_into_asset(&creator_ops, app_id, asset_id);
    verify_clawback_is_app(&creator_ops, app_id, asset_id);
    // APP_CREATOR:   nothing (existing app, no migrate)
    // APP_ADMIN:     opt_in_app_to_asset skipped (app already opted in → early return)
    // ASSET_CREATOR: set_asset_clawback_to_app(1)
    // APP_WITHDRAWER, ASSET_RESERVE: already opted in by deploy_initial → 0 fees
    assert_algo_spent("APP_CREATOR",    before.app_creator,    after.app_creator,    0);
    assert_algo_spent("APP_ADMIN",      before.app_admin,      after.app_admin,      0);
    assert_algo_spent("ASSET_CREATOR",  before.asset_creator,  after.asset_creator,  MIN_FEE);
    assert_algo_spent("APP_WITHDRAWER", before.app_withdrawer, after.app_withdrawer, 0);
    assert_algo_spent("ASSET_RESERVE",  before.asset_reserve,  after.asset_reserve,  0);
    // Existing app already opted in — no inner fee, balance unchanged from deploy_initial.
    assert_eq!(app_microalgos(&creator_ops, app_id), APP_FUND - MIN_FEE, "app ALGO balance unchanged on idempotent redeploy");
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
            true, true, None, None, "BINGLE", 1_000_000, 0, &partial,
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
        true, true, None, None, "BINGLE", 1_000_000, 0, &accounts,
    );
    assert!(result.is_err(), "should fail when two roles share the same address");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("not unique"),
        "error should mention 'not unique', got: {}",
        msg
    );
}
