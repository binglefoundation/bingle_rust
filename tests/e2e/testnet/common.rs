use tracing_subscriber::filter::LevelFilter;
use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::engine::BingleAccessUnsafeForTests;
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;
use rust_comms::{AlgoBingle, AlgoOps};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use rust_comms::util::config_utils::{parse_node_file_with_ids, parse_stun_file};

pub fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Load testnet node config + ids from the standard bundled file.
pub fn load_testnet_config() -> (Option<String>, rust_comms::blockchain::algo_ops::AlgoChainConfig, u64, Option<u64>) {
    let node_path = "nodely_testnet_node.json";
    let (network_name, provider_cfg, node_app_id, node_asset_id) =
        parse_node_file_with_ids(node_path).expect("parse testnet node file");
    let app_id = node_app_id.expect("app_id must be present in the testnet node file");
    (network_name, provider_cfg, app_id, node_asset_id)
}

pub fn build_ops(passphrase: &str, provider_cfg: &rust_comms::blockchain::algo_ops::AlgoChainConfig) -> AlgoOps {
    AlgoOps::new(Some(passphrase.to_string()), None, Some(provider_cfg.clone()))
}

/// Ensure there are at least two static endpoints via indexer
pub fn ensure_two_relays(app_id: u64, ops: &AlgoOps) {
    let ab = AlgoBingle::new(ops.clone(), app_id, 0);
    let list = ab
        .list_static_endpoints_via_indexer(app_id)
        .expect("indexer query for static endpoints");
    assert!(list.len() >= 2, "Expected at least two static endpoints on testnet, got {}", list.len());
}

pub fn load_stun_servers() -> Vec<SocketAddr> {
    parse_stun_file("stunservers.txt").expect("failed to read/parse stunservers.txt")
}

pub fn make_start_options(
    handle: &str,
    passphrase: &str,
    provider_cfg: &rust_comms::blockchain::algo_ops::AlgoChainConfig,
    network_name: Option<String>,
    app_id: u64,
    asset_id: Option<u64>,
    stun_servers: Vec<SocketAddr>,
) -> StartOptions {
    StartOptions {
        handle: handle.to_string(),
        algo_passphrase: Some(passphrase.to_string()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(stun_servers),
        algo_provider_config: Some(provider_cfg.clone()),
        algo_network: network_name, 
        app_id: Some(app_id),
        asset_id,
        log_level: None, handle_cache_expiry: None, dangerous_debug: true, log_mode: rust_comms::util::logging::LogMode::Plain, wait_response_timeout: None,
    }
}

pub fn start_api_and_wait(options: &StartOptions) -> (Arc<BingleApiImpl>, EngineState) {
    let _ = tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .try_init();

    let api = BingleApiImpl::new(options);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(options)).expect("start api");

    // Wait up to 120 seconds for final state: Registered OR NATRestricted
    let start = Instant::now();
    let timeout = Duration::from_secs(120);
    let final_state = loop {
        if let Some(st) = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_state_for_tests()) {
            if st == EngineState::Registered || st == EngineState::NATRestricted { break st; }
        }
        if start.elapsed() > timeout {
            panic!(
                "Timed out waiting for Registered or NATRestricted; last state: {:?}",
                api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_state_for_tests())
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    };

    tracing::info!("[start_api_and_wait] state: {:?}", final_state);
    (api, final_state)
}
