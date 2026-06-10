// tests/blockchain/algo_bingle/register_uniqueness.rs
use rust_comms::blockchain::algo_bingle::AlgoBingle;

#[path = "../../setup_localnet.rs"]
pub mod setup_localnet;
#[path = "../../test_util.rs"]
pub mod test_util;

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_register_handle_uniqueness() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();
    let cfg = test_util::localnet_config();
    
    // 1. Setup accounts
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE])
        .expect("Failed to fund accounts");

    let ops_a = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let ops_b = test_util::ops_from_mnemonic(test_util::ADDRESS_RECEIVE, test_util::PASSPHRASE_RECEIVE, cfg.clone());

    // 2. Deploy app and asset
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&ops_a, "BINGLE", 1000000);
    
    let handle = "unique_handle";
    let ab_a = AlgoBingle::new(ops_a.clone(), app_id, asset_id);
    let ab_b = AlgoBingle::new(ops_b.clone(), app_id, asset_id);

    // 3. Register with A (should succeed)
    ab_a.register(app_id, asset_id, handle, 1).expect("A register handle");

    // 4. Try to register same handle with B (should fail pre-check)
    // Give B some Bingle$ first
    ab_b.opt_in_sender_to_asset(asset_id).expect("B opt-in asset");
    ops_a.send_asset(asset_id, 10, test_util::ADDRESS_RECEIVE).expect("send Bingle$ to B");
    
    let res_b = ab_b.register(app_id, asset_id, handle, 1);
    assert!(res_b.is_err(), "B should fail to register handle that is already in use");
    let err_msg = res_b.err().unwrap().to_string();
    assert!(err_msg.contains("already in use"), "Error message should mention 'already in use', got: {}", err_msg);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_register_handle_race_condition() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();
    let cfg = test_util::localnet_config();
    
    // 1. Setup accounts
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE])
        .expect("Failed to fund accounts");

    let ops_a = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let ops_b = test_util::ops_from_mnemonic(test_util::ADDRESS_RECEIVE, test_util::PASSPHRASE_RECEIVE, cfg.clone());

    // 2. Deploy app and asset
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&ops_a, "BINGLE", 1000000);
    
    // Give B some Bingle$
    let ab_b = AlgoBingle::new(ops_b.clone(), app_id, asset_id);
    ab_b.opt_in_sender_to_asset(asset_id).expect("B opt-in asset");
    ops_a.send_asset(asset_id, 10, test_util::ADDRESS_RECEIVE).expect("send Bingle$ to B");

    let handle = "race_handle";
    
    // We want to simulate a race condition.
    // Since ab.register() has pre-check and post-check, we can't easily make them both pass pre-check 
    // unless we run them in parallel.
    
    let ab_a = AlgoBingle::new(ops_a.clone(), app_id, asset_id);
    let ab_b = AlgoBingle::new(ops_b.clone(), app_id, asset_id);

    let handle_str = handle.to_string();
    let handle_str_2 = handle.to_string();

    let t1 = std::thread::spawn(move || {
        ab_a.register(app_id, asset_id, &handle_str, 1)
    });

    let t2 = std::thread::spawn(move || {
        // Sleep a tiny bit to increase chance of A being first
        std::thread::sleep(std::time::Duration::from_millis(100));
        ab_b.register(app_id, asset_id, &handle_str_2, 1)
    });

    let res1 = t1.join().unwrap();
    let res2 = t2.join().unwrap();

    // One should succeed, the other should fail either at pre-check or post-check.
    // In many cases, B will fail pre-check if A is fast enough.
    // If they both pass pre-check, one will fail post-check.
    
    assert!(res1.is_ok() || res2.is_ok(), "At least one registration should succeed");
    assert!(res1.is_err() || res2.is_err(), "At least one registration should fail");
    
    if let Err(e) = res1 {
        let msg = e.to_string();
        assert!(msg.contains("already in use") || msg.contains("post-check failed"), "Unexpected error: {}", msg);
    }
    if let Err(e) = res2 {
        let msg = e.to_string();
        assert!(msg.contains("already in use") || msg.contains("post-check failed"), "Unexpected error: {}", msg);
    }
}
