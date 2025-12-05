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

use rust_comms::AlgoBingle;
use rust_comms::AlgoOps;
use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;
use rust_comms::util::cli_utils::parse_node_file_with_ids;

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

// Local STUN parser for tests: supports comma/whitespace separated values and '#' comments.
fn parse_stun_list(s: &str) -> Result<Vec<SocketAddr>, String> {
    let mut cleaned = String::with_capacity(s.len());
    for line in s.lines() {
        let line_no_comment = match line.find('#') {
            Some(idx) => &line[..idx],
            None => line,
        };
        cleaned.push_str(line_no_comment);
        cleaned.push('\n');
    }

    let mut addrs = Vec::new();
    for part in cleaned.split(|c: char| c == ',' || c.is_whitespace()) {
        let p = part.trim();
        if p.is_empty() { continue; }
        let parsed = p.parse::<SocketAddr>().ok()
            .or_else(|| p.to_socket_addrs().ok().and_then(|mut it| it.next()));
        if let Some(addr) = parsed {
            addrs.push(addr);
        } else {
            return Err(format!("Invalid STUN server entry '{}': must be <host:port> or <ip:port>", p));
        }
    }
    if addrs.is_empty() {
        Err("No valid STUN servers provided".to_string())
    } else {
        Ok(addrs)
    }
}

fn parse_stun_file(path: &str) -> Result<Vec<SocketAddr>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read STUN servers file '{}': {}", path, e))?;
    parse_stun_list(&content)
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

    let ab = AlgoBingle::new(ops.clone());
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
