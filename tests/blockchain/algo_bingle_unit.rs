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
}
