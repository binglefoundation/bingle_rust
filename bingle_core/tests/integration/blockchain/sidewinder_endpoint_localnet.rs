// Localnet integration test for register_sidewinder_endpoint (issue #237): a permitted Sidewinder
// cluster node (allow_sw_node granted) publishes its endpoint record into rsvd_l_b1, reads it back
// through the shared codec, and clears it; an account without the sw_node bit has its write refused.
// Reads go straight to algod local state (no indexer), so no eventual-consistency polling is needed.

use algo_ops::AlgoChainConfig;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::blockchain::sidewinder_endpoint::SidewinderEndpointRecord;

use crate::setup_localnet;
use crate::util::test_util;

use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};
use test_util::{
    ADDRESS_RECEIVE, ADDRESS_SPEND, PASSPHRASE_RECEIVE, PASSPHRASE_SPEND, localnet_config,
    ops_from_mnemonic,
};

/// Poll the on-chain record (algod is immediately consistent post-confirmation, but allow a brief
/// window for the confirming round) until it matches `want`, up to ~10s.
fn wait_for_record(
    ab: &AlgoBingle,
    app_id: u64,
    address: &str,
    want: Option<SidewinderEndpointRecord>,
) -> Option<SidewinderEndpointRecord> {
    let start = Instant::now();
    loop {
        let got = ab
            .get_sidewinder_endpoint(app_id, address)
            .expect("get_sidewinder_endpoint");
        if got == want || start.elapsed() > Duration::from_secs(10) {
            return got;
        }
        thread::sleep(Duration::from_millis(250));
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn permitted_node_publishes_and_clears_endpoint_unpermitted_is_refused() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();
    let cfg: AlgoChainConfig = localnet_config();

    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND, ADDRESS_RECEIVE])
        .expect(
            "Failed to ensure localnet test accounts funded; install algokit and start localnet",
        );

    // Creator (== initial admin) deploys; the node account opts in for local state.
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let node = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());
    let app_id = test_util::deploy_bingle_app(&creator);
    node.opt_in_app(app_id).expect("node opt-in app");

    let ab_admin = AlgoBingle::new(creator.clone(), app_id, 0);
    let ab_node = AlgoBingle::new(node.clone(), app_id, 0);

    let v4: SocketAddr = "203.0.113.7:4000".parse().unwrap();
    let v6: SocketAddr = "[2001:db8::1]:4001".parse().unwrap();
    let expected = SidewinderEndpointRecord::from_socket_addrs(Some(v4), Some(v6)).unwrap();

    // Not yet permitted: the write must be refused on-chain (the allow_sw_node assert fails).
    let refused = ab_node.register_sidewinder_endpoint(app_id, Some(v4), Some(v6));
    assert!(
        refused.is_err(),
        "an account without the allow_sw_node bit must not be able to publish: {refused:?}"
    );
    assert_eq!(
        ab_admin
            .get_sidewinder_endpoint(app_id, ADDRESS_RECEIVE)
            .expect("read after refused write"),
        None,
        "nothing should have been written by the refused call"
    );

    // Grant sw_node, then the node publishes its record.
    ab_admin
        .set_allow_sw_node(app_id, ADDRESS_RECEIVE, true)
        .expect("set_allow_sw_node");
    ab_node
        .register_sidewinder_endpoint(app_id, Some(v4), Some(v6))
        .expect("register_sidewinder_endpoint");

    let got = wait_for_record(&ab_admin, app_id, ADDRESS_RECEIVE, Some(expected));
    assert_eq!(
        got,
        Some(expected),
        "published record should round-trip back through the codec"
    );

    // Clearing (all-None) removes the key.
    ab_node
        .register_sidewinder_endpoint(app_id, None, None)
        .expect("clear sidewinder endpoint");
    let cleared = wait_for_record(&ab_admin, app_id, ADDRESS_RECEIVE, None);
    assert_eq!(
        cleared, None,
        "record should be gone after an all-None call"
    );
}
