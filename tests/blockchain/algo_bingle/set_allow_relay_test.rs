use rust_comms::blockchain::algo_bingle::AlgoBingle;
use rust_comms::blockchain::algo_ops::{AlgoOps};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_set_allow_relay_param_validation() {
    let ops = AlgoOps::new(None, Some("P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string()), None);
    let ab = AlgoBingle::new(ops, 1, 1);

    // app_id 0 should fail
    let res = ab.set_allow_relay(0, "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA", true);
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("app_id must be > 0"));

    // invalid target address should fail
    let res = ab.set_allow_relay(1, "invalid_addr", true);
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("invalid target address"));
}
