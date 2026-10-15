// Issue #324: client relay discovery via the indexer honours on-chain allow_relay. A relay whose
// permission is withdrawn but whose static endpoint is left in place drops out of discovery, while
// the generic static-endpoint listing still shows it.

use crate::setup_localnet;
use crate::util::test_util;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use serial_test::serial;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use super::send_message_to_id_integration::register_relays;

fn ids(list: &[(String, String)]) -> Vec<String> {
    list.iter().map(|(id, _)| id.clone()).collect()
}

/// Poll relay discovery and the static-endpoint listing until `check` holds for both.
fn wait_for(
    ab: &AlgoBingle,
    app_id: u64,
    timeout: Duration,
    check: impl Fn(&[String], &[String]) -> bool,
) -> (Vec<String>, Vec<String>) {
    let start = Instant::now();
    loop {
        let relays = ids(&ab
            .list_relay_endpoints_via_indexer_sync(app_id)
            .unwrap_or_default());
        let statics = ids(&ab
            .list_static_endpoints_via_indexer_sync(app_id)
            .unwrap_or_default());
        if check(&relays, &statics) || start.elapsed() >= timeout {
            return (relays, statics);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[serial(send_message_to_id)]
#[test]
#[cfg(not(target_os = "ios"))]
#[ntest::timeout(600_000)]
pub fn discovery_drops_relay_whose_allow_relay_is_withdrawn() {
    test_util::init_test_logging_with_filter("info");
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    let _ = setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE],
    );
    let creator = test_util::ops_from_mnemonic(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        cfg.clone(),
    );
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);
    let relay1_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        test_util::find_unused_loopback_port(),
    );
    let relay2_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        test_util::find_unused_loopback_port(),
    );
    register_relays(app_id, asset_id, relay1_addr, relay2_addr);

    let admin = test_util::ops_from_mnemonic(
        test_util::ADDRESS_APP_ADMIN,
        test_util::PASSPHRASE_APP_ADMIN,
        cfg.clone(),
    );
    let ab = AlgoBingle::new(admin, app_id, 0);
    let relay1 = test_util::ADDRESS_SPEND.to_string();
    let relay2 = test_util::ADDRESS_RECEIVE.to_string();

    let (relays, _) = wait_for(&ab, app_id, Duration::from_secs(60), |r, _| {
        r.contains(&relay1) && r.contains(&relay2)
    });
    assert!(
        relays.contains(&relay1) && relays.contains(&relay2),
        "both permitted relays should be discovered, got {relays:?}"
    );

    // Withdraw relay2's permission, leaving its static endpoint registered.
    ab.set_allow_relay(app_id, &relay2, false)
        .expect("set_allow_relay false");

    let (relays, statics) = wait_for(&ab, app_id, Duration::from_secs(60), |r, s| {
        !r.contains(&relay2) && s.contains(&relay2)
    });
    assert!(
        !relays.contains(&relay2),
        "a relay without allow_relay should not be discovered, got {relays:?}"
    );
    assert!(
        relays.contains(&relay1),
        "the permitted relay should still be discovered, got {relays:?}"
    );
    assert!(
        statics.contains(&relay2),
        "the static-endpoint listing should still show the endpoint, got {statics:?}"
    );

    // The discovery closure clients use returns only the permitted relay.
    let discover = bingle_core::relay::discovery::indexer_discover_closure(app_id, Some(cfg), None);
    let discovered: Vec<String> = discover().iter().map(|r| r.id().to_string()).collect();
    assert_eq!(discovered, vec![relay1]);
}
