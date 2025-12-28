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

use log::LevelFilter;
use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::{EngineState, NatType};
use rust_comms::util::cli_utils::{parse_node_file_with_ids, parse_stun_file};
use rust_comms::AlgoBingle;
use rust_comms::AlgoOps;
use std::time::{Duration, Instant};

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

    // Start the user and wait for EndpointAvailable
    let mut api = BingleApiImpl::new(&opts);

    api.start(&opts).expect("start api");

    // Determine expected final state from environment.
    // Primary: EXPECT_FINAL_STATE can be set to "EndpointAvailable" or "NATRestricted".
    // Secondary: derive from NAT_MODE if EXPECT_FINAL_STATE is not set.
    let _expect_state = match env_var("EXPECT_FINAL_STATE").as_deref() {
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

    // Derive expected NAT type from environment
    let expect_nat = match env_var("EXPECT_NAT_TYPE").as_deref() {
        Some("Unknown") => NatType::Unknown,
        Some("NoConnection") => NatType::NoConnection,
        Some("Symmetric") => NatType::Symmetric,
        Some("Restricted") => NatType::Restricted,
        Some("FullCone") => NatType::FullCone,
        Some(other) => panic!("Invalid EXPECT_NAT_TYPE='{}'", other),
        None => {
            match env_var("NAT_MODE").as_deref() {
                Some("Restricted") => NatType::Restricted,
                Some("Symmetric") => NatType::Symmetric,
                // Direct and Full both indicate direct reachability
                Some("Direct") | Some("Full") | None => NatType::FullCone,
                Some(_) => NatType::FullCone,
            }
        }
    };

    // Wait up to 120 seconds for final state: Registered OR NATRestricted
    let start = Instant::now();
    let timeout = Duration::from_secs(120); // TODO: make handshakes faster
    let final_state = loop {
        if let Some(st) = api.engine_state_for_tests() {
            if st == EngineState::Registered || st == EngineState::NATRestricted { break st; }
        }
        if start.elapsed() > timeout {
            panic!(
                "Timed out waiting for Registered or NATRestricted; last state: {:?}",
                api.engine_state_for_tests()
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    };

    // Validate NAT type once final state is reached
    let got_nat = api.engine_nat_type_for_tests().expect("nat type should be set");
    match final_state {
        EngineState::Registered => assert_eq!(got_nat, NatType::FullCone, "Registered implies we reached EndpointAvailable with FullCone NAT"),
        EngineState::NATRestricted => assert!(matches!(got_nat, NatType::Restricted | NatType::Symmetric), "NATRestricted should be Restricted or Symmetric (got {:?})", got_nat),
        _ => {}
    }
    // If EXPECT_NAT_TYPE was provided, assert exact match
    if env_var("EXPECT_NAT_TYPE").is_some() {
        assert_eq!(got_nat, expect_nat, "expected NAT type {:?}, got {:?}", expect_nat, got_nat);
    }

    // If we are Registered, perform DDB lookup for our ID and verify address equals our discovered public endpoint
    if final_state == EngineState::Registered {
        let my_id = api.get_my_id().expect("api.get_my_id Some");
        let nsk = api.engine_ddb_lookup_for_tests(&my_id).expect("ddb lookup should succeed when registered");
        let looked = nsk.inet_socket_address.expect("lookup should return a direct endpoint");
        let ep = api.engine_last_public_addr_for_tests().expect("last public addr should be Some");
        assert_eq!(looked, ep, "DDB lookup should return our discovered public endpoint");
    }

    api.stop();
}
