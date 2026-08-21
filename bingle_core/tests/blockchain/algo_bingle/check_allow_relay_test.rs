use algo_ops::AlgoOps;
use bingle_core::blockchain::algo_bingle::AlgoBingle;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_check_allow_relay_param_validation() {
    let ops = AlgoOps::new_for_algorand(
        None,
        Some("P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string()),
        None,
    );
    let ab = AlgoBingle::new(ops, 1, 1);

    // app_id 0 should fail
    let res = ab.check_allow_relay(
        0,
        "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA",
    );
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("app_id must be > 0"));
}
