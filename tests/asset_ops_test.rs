use rust_comms::algo_ops::{byte_key_to_address, AlgoOps};

#[test]
fn create_asset_validations() {
    let mut ops = AlgoOps::new(None, None, None);
    let _addr = ops.create_address(false, false).unwrap();

    // Empty name
    let e1 = ops.create_asset("", 1000).unwrap_err();
    assert!(format!("{}", e1).contains("asset name"));

    // Zero units
    let e2 = ops.create_asset("TKN", 0).unwrap_err();
    assert!(format!("{}", e2).contains("units_in_issue"));
}

#[test]
fn create_asset_requires_account_access() {
    // Provide an address but no key
    let mut pk = [0u8; 32];
    for i in 0..32 { pk[i] = i as u8; }
    let addr = byte_key_to_address(&pk).unwrap();
    let ops = AlgoOps::new(None, Some(addr), None);
    let err = ops.create_asset("TKN", 1000).unwrap_err();
    assert!(format!("{}", err).contains("account access"));
}

#[test]
fn opt_in_to_asset_requires_account_access() {
    // Provide an address but no key
    let mut pk = [1u8; 32];
    for i in 0..32 { pk[i] = (255 - i as u8) as u8; }
    let addr = byte_key_to_address(&pk).unwrap();
    let ops = AlgoOps::new(None, Some(addr), None);
    let err = ops.opt_in_to_asset(1234).unwrap_err();
    assert!(format!("{}", err).contains("account access"));
}
