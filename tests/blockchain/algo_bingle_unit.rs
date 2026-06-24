use rust_comms::blockchain::algo_bingle::AlgoBingle;
use rust_comms::algo_ops::{AlgoOps};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn algo_bingle_param_validation() {
    // Minimal ops; methods should fail fast on invalid params without network access
    let ops = AlgoOps::new(None, Some("P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string()), None);
    let ab = AlgoBingle::new(ops, 1, 1);

    assert!(ab.buy_bingle(0, 123, 1).is_err(), "app_id == 0 should error");
    assert!(ab.buy_bingle(1, 0, 1).is_err(), "asset_id == 0 should error");

    assert!(ab.sell_bingle(0, 123, 1, 1).is_err(), "app_id == 0 should error");
    assert!(ab.sell_bingle(1, 0, 1, 1).is_err(), "asset_id == 0 should error");
    assert!(ab.sell_bingle(1, 123, 0, 1).is_err(), "amount == 0 should error");

    assert!(ab.register(0, 123, "alice", 1).is_err(), "app_id == 0 should error");
    assert!(ab.register(1, 0, "alice", 1).is_err(), "asset_id == 0 should error");
    assert!(ab.register(1, 123, "", 1).is_err(), "empty handle should error");

    let addr = "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA";
    assert!(ab.withdraw(0, addr, 1, 0, 0).is_err(), "withdraw: app_id == 0 should error");

    assert!(ab.set_predecessor_app(0, 99).is_err(), "set_predecessor_app: app_id == 0 should error");
    assert!(ab.set_predecessor_app(1, 0).is_err(), "set_predecessor_app: predecessor_app_id == 0 should error");

    assert!(ab.set_app_admin(0, addr).is_err(), "set_app_admin: app_id == 0 should error");
    assert!(ab.set_app_withdrawer(0, addr).is_err(), "set_app_withdrawer: app_id == 0 should error");

    assert!(ab.migrate_global(0, 99).is_err(), "migrate_global: app_id == 0 should error");
    assert!(ab.migrate_global(1, 0).is_err(), "migrate_global: old_app_id == 0 should error");

    assert!(ab.migrate_reserve(0, 99, 0).is_err(), "migrate_reserve: app_id == 0 should error");
    assert!(ab.migrate_reserve(1, 0, 0).is_err(), "migrate_reserve: new_app_id == 0 should error");

    assert!(ab.migrate_local(0, 99).is_err(), "migrate_local: app_id == 0 should error");
    assert!(ab.migrate_local(1, 0).is_err(), "migrate_local: old_app_id == 0 should error");
}
