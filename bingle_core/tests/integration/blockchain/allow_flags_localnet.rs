// Localnet integration test for the packed allow-flag bitfield (issue #232): drives the admin-only
// set_allow_* setters against a real deployed contract and reads each flag back, verifying that
// per-flag read-modify-write leaves the other flags intact and that revoking one bit keeps the rest.
use crate::setup_localnet;
use crate::util::test_util;
use algo_ops::AlgoOps;
use bingle_core::blockchain::algo_bingle::AlgoBingle;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_allow_flags_set_clear_read_each_flag() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    let creator_pass = test_util::PASSPHRASE_SPEND;
    let target_addr = test_util::ADDRESS_RECEIVE;
    let target_pass = test_util::PASSPHRASE_RECEIVE;

    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND, target_addr])
        .expect("Failed to fund localnet accounts");

    let ops_creator =
        AlgoOps::new_for_algorand(Some(creator_pass.to_string()), None, Some(cfg.clone()));
    let ops_admin = AlgoOps::new_for_algorand(
        Some(test_util::PASSPHRASE_APP_ADMIN.to_string()),
        None,
        Some(cfg.clone()),
    );
    let (app_id, _asset_id) =
        test_util::deploy_bingle_app_and_asset(&ops_creator, "BINGLE$", 1_000_000);

    let ab = AlgoBingle::new(ops_admin, app_id, 0);

    // Before opt-in, the target has no local state for this app => every read is None.
    assert_eq!(ab.check_allow_sw_node(app_id, target_addr).unwrap(), None);

    let ops_target =
        AlgoOps::new_for_algorand(Some(target_pass.to_string()), None, Some(cfg.clone()));
    ops_target.opt_in_app(app_id).expect("target opt-in app");

    // Freshly opted-in: every flag reads false.
    assert_eq!(
        ab.check_allow_static(app_id, target_addr).unwrap(),
        Some(false)
    );
    assert_eq!(
        ab.check_allow_relay(app_id, target_addr).unwrap(),
        Some(false)
    );
    assert_eq!(
        ab.check_allow_sw_node(app_id, target_addr).unwrap(),
        Some(false)
    );
    assert_eq!(
        ab.check_allow_sw_client(app_id, target_addr).unwrap(),
        Some(false)
    );

    // Grant sw_node — the first write migrates the account to the packed encoding.
    ab.set_allow_sw_node(app_id, target_addr, true)
        .expect("set_allow_sw_node");
    assert_eq!(
        ab.check_allow_sw_node(app_id, target_addr).unwrap(),
        Some(true)
    );
    assert_eq!(
        ab.check_allow_static(app_id, target_addr).unwrap(),
        Some(false)
    );
    assert_eq!(
        ab.check_allow_relay(app_id, target_addr).unwrap(),
        Some(false)
    );
    assert_eq!(
        ab.check_allow_sw_client(app_id, target_addr).unwrap(),
        Some(false)
    );

    // Grant sw_client and static; each single-bit write must preserve the already-set bits.
    ab.set_allow_sw_client(app_id, target_addr, true)
        .expect("set_allow_sw_client");
    ab.set_allow_static(app_id, target_addr, true)
        .expect("set_allow_static");
    assert_eq!(
        ab.check_allow_sw_node(app_id, target_addr).unwrap(),
        Some(true)
    );
    assert_eq!(
        ab.check_allow_sw_client(app_id, target_addr).unwrap(),
        Some(true)
    );
    assert_eq!(
        ab.check_allow_static(app_id, target_addr).unwrap(),
        Some(true)
    );
    assert_eq!(
        ab.check_allow_relay(app_id, target_addr).unwrap(),
        Some(false)
    );

    // Revoke just sw_node; the other granted bits remain set.
    ab.set_allow_sw_node(app_id, target_addr, false)
        .expect("clear allow_sw_node");
    assert_eq!(
        ab.check_allow_sw_node(app_id, target_addr).unwrap(),
        Some(false)
    );
    assert_eq!(
        ab.check_allow_sw_client(app_id, target_addr).unwrap(),
        Some(true)
    );
    assert_eq!(
        ab.check_allow_static(app_id, target_addr).unwrap(),
        Some(true)
    );
}
