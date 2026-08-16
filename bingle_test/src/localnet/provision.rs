//! Higher-level localnet provisioning used by cross-crate integration tests (e.g. the `bingle_cli`
//! chat e2e): register two root relays with the app, bring up local STUN servers, and probe whether
//! localnet is reachable. Mirrors the setup helpers that live inline in `bingle_core`'s
//! `send_message_to_id_integration` test, extracted here so other crates can reuse them.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use bingle_core::api::bingle_api::{BingleApi, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::engine::BingleAccessUnsafeForTests;
use bingle_core::stun::{SimpleStunServer, SimpleStunStartOptions};
use bingle_core::util::logging::LogMode;

use super::relay_test_util::{wait_for_handles_visible, wait_for_relays_visible};
use super::test_util;

/// Start an in-process (non-relay) client `BingleApiImpl` with the given STUN list, for use as a
/// peer in cross-crate e2e tests. Mirrors `send_message_to_id_integration::start_client`.
pub fn start_client(
    handle: &str,
    passphrase: &str,
    stun_list: Vec<SocketAddr>,
    app_id: u64,
    cfg: bingle_core::blockchain::algo_ops::AlgoChainConfig,
) -> Arc<BingleApiImpl> {
    let opts = StartOptions {
        handle: handle.into(),
        algo_passphrase: Some(passphrase.to_string()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(stun_list),
        algo_provider_config: Some(cfg),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false,
        log_mode: LogMode::Plain,
        // Short response timeout so a transient relay non-response fails fast and the bounded Listen
        // retry gets multiple attempts within the registration wait.
        wait_response_timeout: Some(Duration::from_secs(20)),
    };
    let api = BingleApiImpl::new(&opts);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts))
        .expect("client start");
    api
}

/// Non-panicking localnet reachability probe: returns `false` when algod is not accepting
/// connections, so a test can skip cleanly instead of failing when no localnet is running.
pub fn localnet_available() -> bool {
    let cfg = test_util::localnet_config();
    let host = cfg
        .client_api_url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .to_string();
    // Resolve the host (e.g. "localhost") to a socket address, then probe with a short timeout.
    (host.as_str(), cfg.client_api_port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .map(|sa| TcpStream::connect_timeout(&sa, Duration::from_secs(2)).is_ok())
        .unwrap_or(false)
}

/// Opt two relay accounts into the app, grant them static/relay permissions, register their handles,
/// register their static endpoints, and wait for the indexer to surface both the endpoints and the
/// handles. Ported from `send_message_to_id_integration::register_relays`.
pub fn register_relays(
    app_id: u64,
    asset_id: u64,
    relay1_addr: SocketAddr,
    relay2_addr: SocketAddr,
) {
    let cfg = test_util::localnet_config();
    let ops_admin = test_util::ops_from_mnemonic(
        test_util::ADDRESS_APP_ADMIN,
        test_util::PASSPHRASE_APP_ADMIN,
        cfg.clone(),
    );
    let ops_relay1 = test_util::ops_from_mnemonic(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        cfg.clone(),
    );
    let ops_relay2 = test_util::ops_from_mnemonic(
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        cfg.clone(),
    );

    let ab_creator = AlgoBingle::new(ops_admin.clone(), app_id, 0);
    let ab_r1 = AlgoBingle::new(ops_relay1.clone(), app_id, 0);
    let ab_r2 = AlgoBingle::new(ops_relay2.clone(), app_id, 0);

    ops_relay1.opt_in_app(app_id).expect("relay1 opt-in app");
    ops_relay2.opt_in_app(app_id).expect("relay2 opt-in app");
    ab_creator
        .set_allow_static(app_id, test_util::ADDRESS_SPEND, true)
        .expect("set_allow_static r1");
    ab_creator
        .set_allow_static(app_id, test_util::ADDRESS_RECEIVE, true)
        .expect("set_allow_static r2");
    ab_creator
        .set_allow_relay(app_id, test_util::ADDRESS_SPEND, true)
        .expect("set_allow_relay r1");
    ab_creator
        .set_allow_relay(app_id, test_util::ADDRESS_RECEIVE, true)
        .expect("set_allow_relay r2");

    test_util::register_client_on_blockchain(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        "relay1",
        app_id,
        asset_id,
        &ops_admin,
        cfg.clone(),
    );
    test_util::register_client_on_blockchain(
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        "relay2",
        app_id,
        asset_id,
        &ops_admin,
        cfg.clone(),
    );

    let r1_compact = test_util::get_compact_advert_record(&ops_relay1, relay1_addr, true);
    ab_r1
        .register_endpoint(app_id, &r1_compact)
        .expect("register_endpoint r1");
    let r2_compact = test_util::get_compact_advert_record(&ops_relay2, relay2_addr, true);
    ab_r2
        .register_endpoint(app_id, &r2_compact)
        .expect("register_endpoint r2");

    let expected = vec![
        (test_util::ADDRESS_SPEND.to_string(), relay1_addr),
        (test_util::ADDRESS_RECEIVE.to_string(), relay2_addr),
    ];
    if !wait_for_relays_visible(&ab_creator, app_id, &expected, Duration::from_secs(60)) {
        panic!(
            "Relays did not become visible via list_static_endpoints_via_indexer_sync within 60s"
        );
    }
    if !wait_for_handles_visible(
        cfg.clone(),
        app_id,
        &["relay1", "relay2"],
        Duration::from_secs(60),
    ) {
        panic!("Relay handles did not become visible via indexer within 60s");
    }
}

/// Start two local STUN servers on loopback and return them with their address list. `broken_nat`
/// forces clients to fall back to relays. Ported from `send_message_to_id_integration`.
pub fn setup_stun_servers(
    broken_nat: bool,
) -> (SimpleStunServer, SimpleStunServer, Vec<SocketAddr>) {
    setup_stun_servers_advertised(broken_nat, IpAddr::V4(Ipv4Addr::LOCALHOST))
}

/// Like [`setup_stun_servers`] but advertises the servers at `advert_ip` (issue #137). For a
/// non-loopback `advert_ip` (e.g. an Android emulator's `10.0.2.2` gateway) the servers bind all
/// interfaces so both the emulator (via its qemu gateway → host loopback) and a host-side peer (via
/// a matching loopback alias) can reach them; the returned list uses `advert_ip` (what clients dial).
pub fn setup_stun_servers_advertised(
    broken_nat: bool,
    advert_ip: IpAddr,
) -> (SimpleStunServer, SimpleStunServer, Vec<SocketAddr>) {
    let p1 = test_util::find_unused_loopback_port();
    let p2 = test_util::find_unused_loopback_port();
    let bind_ip = if advert_ip.is_loopback() {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    };

    let s1 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: SocketAddr::new(bind_ip, p1),
        attach_to: None,
        broken_nat,
    })
    .expect("start s1");
    let s2 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: SocketAddr::new(bind_ip, p2),
        attach_to: None,
        broken_nat,
    })
    .expect("start s2");

    (
        s1,
        s2,
        vec![
            SocketAddr::new(advert_ip, p1),
            SocketAddr::new(advert_ip, p2),
        ],
    )
}

/// Write a `bingle_cli`-compatible `--node-file` JSON for localnet with the given app/asset ids, to
/// `path`. The chat CLI reads algod/indexer endpoints, token and ids from this file.
pub fn write_localnet_node_file(path: &std::path::Path, app_id: u64, asset_id: u64) {
    write_localnet_node_file_host(path, app_id, asset_id, "http://localhost");
}

/// Like [`write_localnet_node_file`] but with a configurable algod/indexer host URL (issue #137).
/// For an Android emulator, pass `http://10.0.2.2` (its alias for the host loopback where the
/// Docker localnet is exposed), since the emulator's own `localhost` is the emulator, not the host.
pub fn write_localnet_node_file_host(
    path: &std::path::Path,
    app_id: u64,
    asset_id: u64,
    host_url: &str,
) {
    let node_file = serde_json::json!({
        "network": "localnet",
        "client_api_url": host_url,
        "client_api_port": 4001,
        "indexer_api_url": host_url,
        "indexer_api_port": 8980,
        "token": test_util::LOCALNET_TOKEN,
        "token_key": "X-Algo-API-Token",
        "app_id": app_id,
        "asset_id": asset_id,
    });
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&node_file).expect("serialize node file"),
    )
    .expect("write node file");
}
