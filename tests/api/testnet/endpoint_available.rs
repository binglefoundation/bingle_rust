// tests/api/testnet/endpoint_available.rs
// Integration test targeting testnet using the bundled nodely_testnet_node.json.
//
// Requirements per issue:
// - runs against testnet using the nodely_testnet_node.json config
// - expects two root relays to exist and be running - list_static_endpoints_via_indexer will locate them
// - starts using a configured user with handle TESTNET_USER and passphrase TESTNET_PASSPHRASE
// - starts the user and waits for state EndpointAvailable
//
// To keep CI green in environments without testnet credentials, this test only
// runs when BINGLE_RUN_TESTNET=1 is set in the environment. Otherwise it exits early.

use std::time::{Duration, Instant};

use rust_comms::AlgoBingle;
use rust_comms::AlgoOps;
use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;
use rust_comms::util::cli_utils::parse_node_file_with_ids;

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

#[test]
fn testnet_user_reaches_endpoint_available() {
    // Only run when explicitly enabled.
    if env_var("BINGLE_RUN_TESTNET").as_deref() != Some("1") {
        eprintln!("[skipped] Set BINGLE_RUN_TESTNET=1 to run testnet integration test");
        return;
    }

    // Load testnet node configuration and IDs from the bundled file.
    let node_path = "nodely_testnet_node.json";
    let (network_name, provider_cfg, node_app_id, _node_asset_id) =
        parse_node_file_with_ids(node_path).expect("parse testnet node file");

    // Always validate that Option succeeds where required
    let app_id = node_app_id.expect("app_id must be present in the testnet node file");

    // Expect two static endpoints via indexer
    // Construct an AlgoOps using the provided config and the user's credentials.
    let handle = env_var("TESTNET_USER").expect("TESTNET_USER env var must be set");
    let passphrase = env_var("TESTNET_PASSPHRASE").expect("TESTNET_PASSPHRASE env var must be set");

    // Build ops: AlgoOps::new(provider_cfg, Some(passphrase), None)
    // Discover constructor signature from existing tests: use AlgoOps::from config-like builder.
    // The struct provides a public constructor via AlgoOps { config, .. } pattern with helpers.
    let ops = AlgoOps::new(Some(passphrase.clone()), None, Some(provider_cfg.clone()));

    let ab = AlgoBingle::new(ops.clone());
    let list = ab
        .list_static_endpoints_via_indexer(app_id)
        .expect("indexer query for static endpoints");
    assert!(list.len() >= 2, "Expected at least two static endpoints on testnet, got {}", list.len());

    // Start the user and wait for EndpointAvailable
    let mut api = BingleApiImpl::new();
    let opts = StartOptions {
        handle: handle.clone(),
        algo_passphrase: Some(passphrase.clone()),
        static_ip: None,
        am_relay: false,
        stun_servers: None,
        algo_provider_config: Some(provider_cfg.clone()),
        algo_network: network_name.clone(),
    };

    api.start(opts).expect("start api");

    // Wait up to 60 seconds for EndpointAvailable
    let start = Instant::now();
    let timeout = Duration::from_secs(60);
    loop {
        if let Some(EngineState::EndpointAvailable) = api.engine_state_for_tests() {
            break;
        }
        if start.elapsed() > timeout {
            panic!("Timed out waiting for EndpointAvailable; last state: {:?}", api.engine_state_for_tests());
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    api.stop();
}
