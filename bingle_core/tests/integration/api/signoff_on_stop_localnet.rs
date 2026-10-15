// Issue #279: a client that stops signs off from the DDB, so both relays drop its advert. Covers a
// direct client (working NAT) and a relayed client (broken NAT).

use crate::setup_localnet;
use crate::util::test_util;
use crate::util::test_util::register_client_on_blockchain;
use bingle_core::api::bingle_api::{BingleApi, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::engine::BingleAccessUnsafeForTests;
use bingle_core::stun::{SimpleStunServer, SimpleStunStartOptions};
use serial_test::serial;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::send_message_to_id_integration::register_relays;

fn start_stun_pair(broken_nat: bool) -> (SimpleStunServer, SimpleStunServer, Vec<SocketAddr>) {
    let a1 = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        test_util::find_unused_loopback_port(),
    );
    let a2 = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        test_util::find_unused_loopback_port(),
    );
    let s1 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: a1,
        attach_to: None,
        broken_nat,
    })
    .expect("start stun s1");
    let s2 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: a2,
        attach_to: None,
        broken_nat,
    })
    .expect("start stun s2");
    (s1, s2, vec![a1, a2])
}

fn relay_has_record(relay: &Arc<BingleApiImpl>, id: &str) -> bool {
    relay.with_engine_mut(|e| e.ddb_backend_lookup_for_tests(id).is_some())
}

/// Poll until every relay's DDB holds (`present`) or lacks the record for `id`.
fn wait_for_record_on_relays(
    relays: &[&Arc<BingleApiImpl>],
    id: &str,
    present: bool,
    timeout: Duration,
) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if relays.iter().all(|r| relay_has_record(r, id) == present) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn run_signoff_on_stop_test(broken_nat: bool) {
    test_util::init_test_logging_with_filter("info,bingle_core::dtls=info");
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    let _ = setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[
            test_util::ADDRESS_SPEND,
            test_util::ADDRESS_RECEIVE,
            test_util::ADDRESS_10MIL,
        ],
    );

    let relay1_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        test_util::find_unused_loopback_port(),
    );
    let relay2_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        test_util::find_unused_loopback_port(),
    );

    let creator = test_util::ops_from_mnemonic(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        cfg.clone(),
    );
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);
    register_relays(app_id, asset_id, relay1_addr, relay2_addr);

    let relay1 = test_util::start_root_relay(
        "relay1",
        relay1_addr,
        test_util::PASSPHRASE_SPEND,
        app_id,
        cfg.clone(),
    );
    let relay2 = test_util::start_root_relay(
        "relay2",
        relay2_addr,
        test_util::PASSPHRASE_RECEIVE,
        app_id,
        cfg.clone(),
    );

    let (mut s1, mut s2, stun_list) = start_stun_pair(broken_nat);
    register_client_on_blockchain(
        test_util::ADDRESS_10MIL,
        test_util::PASSPHRASE_10MIL,
        "client_a",
        app_id,
        asset_id,
        &creator,
        cfg.clone(),
    );

    let opts = StartOptions {
        handle: "client_a".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_10MIL.parse().unwrap()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(stun_list),
        algo_provider_config: Some(cfg.clone()),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: Some(Duration::from_secs(20)),
        advert_ttl: None,
    };
    let client = BingleApiImpl::new(&opts);
    client
        .access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts))
        .expect("client start");
    assert!(
        test_util::wait_for_registered(&client, Duration::from_secs(120)),
        "client did not reach Registered state (state = {:?})",
        client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests())
    );
    let client_id = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id())
        .expect("client id");

    // The advert must be on both relays (registered on one, rippled to the other) before the stop,
    // so its absence afterwards is down to the sign-off.
    assert!(
        wait_for_record_on_relays(
            &[&relay1, &relay2],
            &client_id,
            true,
            Duration::from_secs(30)
        ),
        "client advert did not reach both relays (relay1={}, relay2={})",
        relay_has_record(&relay1, &client_id),
        relay_has_record(&relay2, &client_id)
    );
    let ep = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_ddb_lookup_for_tests(&client_id))
        .expect("lookup client before stop");
    assert_eq!(
        ep.is_relay(),
        broken_nat,
        "client should be registered as a {} endpoint",
        if broken_nat { "relay" } else { "direct" }
    );

    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());

    assert!(
        wait_for_record_on_relays(
            &[&relay1, &relay2],
            &client_id,
            false,
            Duration::from_secs(15)
        ),
        "client advert survived its stop (relay1={}, relay2={})",
        relay_has_record(&relay1, &client_id),
        relay_has_record(&relay2, &client_id)
    );
    let after = relay1
        .access_unsafe_for_tests(|r: &mut BingleApiImpl| r.engine_ddb_lookup_for_tests(&client_id));
    assert!(
        after.is_err(),
        "DDB lookup of a stopped client should be not-found, got {:?}",
        after.ok()
    );

    relay1.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    relay2.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    s1.stop();
    s2.stop();
}

#[serial(send_message_to_id)]
#[test]
#[cfg(not(target_os = "ios"))]
#[ntest::timeout(900_000)]
pub fn direct_client_signs_off_on_stop() {
    run_signoff_on_stop_test(false);
}

#[serial(send_message_to_id)]
#[test]
#[cfg(not(target_os = "ios"))]
#[ntest::timeout(900_000)]
pub fn relayed_client_signs_off_on_stop() {
    run_signoff_on_stop_test(true);
}
