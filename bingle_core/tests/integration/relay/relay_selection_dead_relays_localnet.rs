// Issue #122: a client starts and reaches a live relay when the relays it ranks first and second
// are permissioned on chain but not running. Before the fix, selection only ever tried those two,
// so the client ended at NoConnection although a live relay existed.

use crate::setup_localnet;
use crate::util::test_util;
use crate::util::test_util::register_client_on_blockchain;
use bingle_core::api::bingle_api::{BingleApi, BingleApiBoth, OnListeningHandler, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::ddb::{AdvertRecord, InetSocketAddress};
use bingle_core::engine::BingleAccessUnsafeForTests;
use bingle_core::relay::relay_finder::{RelayFinder, RelayFinderTestTrait, RelayInfo};
use bingle_core::stun::{SimpleStunServer, SimpleStunStartOptions};
use serial_test::serial;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use super::send_message_to_id_integration::register_relays;

fn loopback() -> SocketAddr {
    SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        test_util::find_unused_loopback_port(),
    )
}

/// Register a third relay on chain: opted in, allowed static and relay, handle and endpoint.
fn register_extra_relay(
    address: &str,
    passphrase: &str,
    handle: &str,
    relay_addr: SocketAddr,
    app_id: u64,
    asset_id: u64,
) {
    let cfg = test_util::localnet_config();
    let ops_admin = test_util::ops_from_mnemonic(
        test_util::ADDRESS_APP_ADMIN,
        test_util::PASSPHRASE_APP_ADMIN,
        cfg.clone(),
    );
    let ops_relay = test_util::ops_from_mnemonic(address, passphrase, cfg.clone());
    let ab_admin = AlgoBingle::new(ops_admin.clone(), app_id, 0);
    let ab_relay = AlgoBingle::new(ops_relay.clone(), app_id, 0);
    ops_relay
        .opt_in_app(app_id)
        .expect("extra relay opt-in app");
    ab_admin
        .set_allow_static(app_id, address, true)
        .expect("set_allow_static extra relay");
    ab_admin
        .set_allow_relay(app_id, address, true)
        .expect("set_allow_relay extra relay");
    register_client_on_blockchain(
        address, passphrase, handle, app_id, asset_id, &ops_admin, cfg,
    );
    let compact = test_util::get_compact_advert_record(&ops_relay, relay_addr, true);
    ab_relay
        .register_endpoint(app_id, &compact)
        .expect("register_endpoint extra relay");
}

/// The relays in the order the client ranks them (preferred first), as relay selection does.
fn ranked_for(client_id: &str, relays: &[(String, SocketAddr)]) -> Vec<String> {
    let mut infos: Vec<RelayInfo> = relays
        .iter()
        .map(|(id, a)| {
            RelayInfo::root(AdvertRecord::new_unsigned(
                id.clone(),
                Some(InetSocketAddress::from(*a)),
                Some(true),
                None,
                None,
                "1970-01-01T00:00:00Z".to_string(),
            ))
        })
        .collect();
    infos.sort_by(|a, b| a.id().cmp(b.id()));
    let no_api: Weak<dyn BingleApiBoth> = Weak::<BingleApiImpl>::new();
    let finder = RelayFinder::new(no_api, Arc::new(Vec::new));
    let (preferred, _) = finder.select_indices(&infos, client_id);
    let n = infos.len();
    (0..n)
        .map(|offset| infos[(preferred + offset) % n].id().to_string())
        .collect()
}

#[serial(send_message_to_id)]
#[test]
#[cfg(not(target_os = "ios"))]
#[ntest::timeout(900_000)]
pub fn client_reaches_live_relay_past_two_dead_ones() {
    test_util::init_test_logging_with_filter("info,bingle_core::dtls=info");
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    let _ = setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[
            test_util::ADDRESS_SPEND,
            test_util::ADDRESS_RECEIVE,
            test_util::ADDRESS_CLIENT_B,
            test_util::ADDRESS_10MIL,
        ],
    );

    let accounts = [
        (
            test_util::ADDRESS_SPEND,
            test_util::PASSPHRASE_SPEND,
            "relay1",
            loopback(),
        ),
        (
            test_util::ADDRESS_RECEIVE,
            test_util::PASSPHRASE_RECEIVE,
            "relay2",
            loopback(),
        ),
        (
            test_util::ADDRESS_CLIENT_B,
            test_util::PASSPHRASE_CLIENT_B,
            "relay3",
            loopback(),
        ),
    ];

    let creator = test_util::ops_from_mnemonic(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        cfg.clone(),
    );
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);
    register_relays(app_id, asset_id, accounts[0].3, accounts[1].3);
    let (addr3, pass3, handle3, relay3_addr) = accounts[2];
    register_extra_relay(addr3, pass3, handle3, relay3_addr, app_id, asset_id);

    // Leave the client's first- and second-ranked relays down; start only the third.
    let relays: Vec<(String, SocketAddr)> = accounts
        .iter()
        .map(|(a, _, _, ep)| (a.to_string(), *ep))
        .collect();
    let ranked = ranked_for(test_util::ADDRESS_10MIL, &relays);
    tracing::info!(
        "[Test] client ranks relays {:?}; only the last is started",
        ranked
    );
    let live_id = ranked[2].clone();
    let (_, live_pass, live_handle, live_addr) = *accounts
        .iter()
        .find(|(a, _, _, _)| *a == live_id)
        .expect("live relay account");
    let live_relay =
        test_util::start_root_relay(live_handle, live_addr, live_pass, app_id, cfg.clone());

    let s1_addr = loopback();
    let s2_addr = loopback();
    let mut s1 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: s1_addr,
        attach_to: None,
        broken_nat: false,
    })
    .expect("start stun s1");
    let mut s2 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: s2_addr,
        attach_to: None,
        broken_nat: false,
    })
    .expect("start stun s2");

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
        stun_servers: Some(vec![s1_addr, s2_addr]),
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
    let listening = Arc::new(AtomicBool::new(false));
    let flag = listening.clone();
    let handler: Arc<OnListeningHandler> = Arc::new(move |is_listening, _nat| {
        if is_listening {
            flag.store(true, Ordering::SeqCst);
        }
    });
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.set_on_listening(Some(handler)));

    let started = Instant::now();
    client
        .access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts))
        .expect("client start");
    assert!(
        test_util::wait_for_registered(&client, Duration::from_secs(120)),
        "client did not reach Registered with its two top-ranked relays down (state = {:?})",
        client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests())
    );
    tracing::info!("[Test] client Registered after {:?}", started.elapsed());

    let deadline = Instant::now() + Duration::from_secs(10);
    while !listening.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        listening.load(Ordering::SeqCst),
        "on_listening(true) was not called"
    );

    let client_id = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id())
        .expect("client id");
    assert!(
        live_relay.with_engine_mut(|e| e.ddb_backend_lookup_for_tests(&client_id).is_some()),
        "the live relay should hold the client's advert"
    );

    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    live_relay.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    s1.stop();
    s2.stop();
}
