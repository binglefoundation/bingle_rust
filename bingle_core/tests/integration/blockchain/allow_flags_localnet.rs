// Localnet integration tests for the packed allow-flag bitfield (issue #232) and the Sidewinder
// client self-grant (issue #239). The first drives the admin-only
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

// Issue #239: registering grants the caller's own Sidewinder client bit with no admin call; an admin
// revoke clears it and is sticky against the user's own enrol_sw_client; an admin enable restores it.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_register_self_grants_sw_client_and_admin_revoke_sticks() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    let user_addr = test_util::ADDRESS_CLIENT_B;
    let unregistered_addr = test_util::ADDRESS_RECEIVE;
    setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[test_util::ADDRESS_SPEND, user_addr, unregistered_addr],
    )
    .expect("Failed to fund localnet accounts");

    let ops_creator = AlgoOps::new_for_algorand(
        Some(test_util::PASSPHRASE_SPEND.to_string()),
        None,
        Some(cfg.clone()),
    );
    let (app_id, asset_id) =
        test_util::deploy_bingle_app_and_asset(&ops_creator, "BINGLE$", 1_000_000);
    let admin = AlgoBingle::new(
        AlgoOps::new_for_algorand(
            Some(test_util::PASSPHRASE_APP_ADMIN.to_string()),
            None,
            Some(cfg.clone()),
        ),
        app_id,
        asset_id,
    );
    let user = AlgoBingle::new(
        AlgoOps::new_for_algorand(
            Some(test_util::PASSPHRASE_CLIENT_B.to_string()),
            None,
            Some(cfg.clone()),
        ),
        app_id,
        asset_id,
    );

    // Register with no admin call: the client bit is granted, the node bit is not.
    user.buy_bingle(app_id, asset_id, 1)
        .expect("buy Bingle$ for registration fee");
    user.register(app_id, asset_id, "sw_client_self_grant", 1)
        .expect("register");
    assert_eq!(
        admin.check_allow_sw_client(app_id, user_addr).unwrap(),
        Some(true),
        "registration grants the client bit"
    );
    assert_eq!(
        admin.check_allow_sw_node(app_id, user_addr).unwrap(),
        Some(false),
        "registration never grants the node bit"
    );
    assert_eq!(
        admin.check_sw_client_revoked(app_id, user_addr).unwrap(),
        Some(false)
    );
    // Enrolling again while granted is harmless.
    user.enrol_sw_client(app_id)
        .expect("enrol while already granted");
    assert_eq!(
        admin.check_allow_sw_client(app_id, user_addr).unwrap(),
        Some(true)
    );

    // An admin revoke clears the bit, and the user cannot grant it back.
    admin
        .set_allow_sw_client(app_id, user_addr, false)
        .expect("admin revoke");
    assert_eq!(
        admin.check_allow_sw_client(app_id, user_addr).unwrap(),
        Some(false)
    );
    assert_eq!(
        admin.check_sw_client_revoked(app_id, user_addr).unwrap(),
        Some(true)
    );
    user.enrol_sw_client(app_id)
        .expect("enrol after revoke is accepted but grants nothing");
    assert_eq!(
        admin.check_allow_sw_client(app_id, user_addr).unwrap(),
        Some(false),
        "a revoke is sticky against self-enrolment"
    );

    // An admin enable restores the bit and lifts the revoke.
    admin
        .set_allow_sw_client(app_id, user_addr, true)
        .expect("admin enable");
    assert_eq!(
        admin.check_allow_sw_client(app_id, user_addr).unwrap(),
        Some(true)
    );
    assert_eq!(
        admin.check_sw_client_revoked(app_id, user_addr).unwrap(),
        Some(false)
    );

    // An opted-in account with no handle cannot enrol itself.
    let unregistered_ops = AlgoOps::new_for_algorand(
        Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        None,
        Some(cfg.clone()),
    );
    unregistered_ops
        .opt_in_app(app_id)
        .expect("opt in without registering");
    let unregistered = AlgoBingle::new(unregistered_ops, app_id, asset_id);
    assert!(
        unregistered.enrol_sw_client(app_id).is_err(),
        "enrol needs a registered handle"
    );
    assert_eq!(
        admin
            .check_allow_sw_client(app_id, unregistered_addr)
            .unwrap(),
        Some(false)
    );
}
