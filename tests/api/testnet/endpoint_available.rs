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
use std::fs;
use std::net::{SocketAddr, ToSocketAddrs};
use log::LevelFilter;
use rust_comms::AlgoBingle;
use rust_comms::AlgoOps;
use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;
use rust_comms::util::cli_utils::{parse_node_file_with_ids, parse_stun_file};

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

    log::set_max_level(LevelFilter::Info);

    // Load testnet node configuration and IDs from the bundled file.
    let node_path = "nodely_testnet_node.json";
    let (network_name, provider_cfg, node_app_id, node_asset_id) =
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

    let ab = AlgoBingle::new(ops.clone(), app_id, 0);
    let list = ab
        .list_static_endpoints_via_indexer(app_id)
        .expect("indexer query for static endpoints");
    assert!(list.len() >= 2, "Expected at least two static endpoints on testnet, got {}", list.len());

    // Start the user and wait for EndpointAvailable
    let mut api = BingleApiImpl::new();

    // Load STUN servers from the repository root file and configure options accordingly.
    let stun_servers = parse_stun_file("stunservers.txt").expect("failed to read/parse stunservers.txt");

    let opts = StartOptions {
        handle: handle.clone(),
        algo_passphrase: Some(passphrase.clone()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(stun_servers),
        algo_provider_config: Some(provider_cfg.clone()),
        algo_network: network_name.clone(),
        app_id: Some(app_id),
        asset_id: node_asset_id,
        log_level: None,
    };

    api.start(opts).expect("start api");

    // Determine expected final state from environment.
    // Primary: EXPECT_FINAL_STATE can be set to "EndpointAvailable" or "NATRestricted".
    // Secondary: derive from NAT_MODE if EXPECT_FINAL_STATE is not set.
    let expect_state = match env_var("EXPECT_FINAL_STATE").as_deref() {
        Some("EndpointAvailable") => EngineState::EndpointAvailable,
        Some("NATRestricted") => EngineState::NATRestricted,
        Some(other) => panic!("Invalid EXPECT_FINAL_STATE='{}' (allowed: EndpointAvailable|NATRestricted)", other),
        None => {
            match env_var("NAT_MODE").as_deref() {
                Some("Restricted") => EngineState::NATRestricted,
                // Direct and Full both expect EndpointAvailable by requirement; default to EndpointAvailable
                Some("Direct") | Some("Full") | None => EngineState::EndpointAvailable,
                Some(other) => {
                    eprintln!("[warn] Unknown NAT_MODE='{}'; defaulting expected state to EndpointAvailable", other);
                    EngineState::EndpointAvailable
                }
            }
        }
    };

    // Wait up to 60 seconds for expected state
    let start = Instant::now();
    let timeout = Duration::from_secs(120); // TODO: make handshakes faster
    loop {
        match (expect_state, api.engine_state_for_tests()) {
            (EngineState::EndpointAvailable, Some(EngineState::EndpointAvailable)) => break,
            (EngineState::NATRestricted, Some(EngineState::NATRestricted)) => break,
            _ => {}
        }
        if start.elapsed() > timeout {
            panic!(
                "Timed out waiting for {:?}; last state: {:?}",
                expect_state,
                api.engine_state_for_tests()
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    api.stop();
}
