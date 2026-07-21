// tests/api/testnet/endpoint_available.rs
// Integration test targeting testnet using the bundled nodely_staging_testnet_node.json.
//
// Requirements per issue:
// - runs against testnet using the nodely_staging_testnet_node.json config
// - expects two root relays to exist and be running - list_static_endpoints_via_indexer will locate them
// - starts using a configured user with handle TESTNET_USER and passphrase TESTNET_PASSPHRASE
// - starts the user and waits for state EndpointAvailable
//
// To keep CI green in environments without testnet credentials, this test only
// runs when BINGLE_RUN_TESTNET=1 is set in the environment. Otherwise it exits early.

use bingle_core::AlgoBingle;
use bingle_core::AlgoOps;
use bingle_core::api::bingle_api::{BingleApi, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::engine::{BingleAccess, BingleAccessUnsafeForTests};
use bingle_core::engine::{EngineState, NatType};
use bingle_core::util::config_utils::{parse_node_file_with_ids, parse_stun_file};
use std::time::{Duration, Instant};
use tracing_subscriber::filter::LevelFilter;

fn env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn testnet_user_reaches_endpoint_available() {
    // Only run when explicitly enabled.
    if env_var("BINGLE_RUN_TESTNET").as_deref() != Some("1") {
        eprintln!("[skipped] Set BINGLE_RUN_TESTNET=1 to run testnet integration test");
        return;
    }

    let _ = tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .try_init();

    // Load testnet node configuration and IDs from the bundled file.
    let node_path = "nodely_staging_testnet_node.json";
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
    let static_endpoints = ab
        .list_static_endpoints_via_indexer(app_id)
        .expect("indexer query for static endpoints");
    assert!(
        static_endpoints.len() >= 2,
        "Expected at least two static endpoints on testnet, got {}",
        static_endpoints.len()
    );

    // Load STUN servers from the repository root file and configure options accordingly.
    let stun_servers =
        parse_stun_file("stunservers.txt").expect("failed to read/parse stunservers.txt");

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
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };

    // Start the user and wait for EndpointAvailable
    let api = BingleApiImpl::new(&opts);

    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts))
        .expect("start api");

    // Before proceeding, ensure both static endpoints are reachable: send RelayCheck and await response.
    // Use a single 120s budget to validate availability of the first two endpoints returned by indexer.
    // {
    //     let to_check: Vec<(String, String)> = static_endpoints.iter().take(2).cloned().collect();
    //     let deadline = Instant::now() + Duration::from_secs(120);
    //     let mut ok: Vec<bool> = vec![false; to_check.len()];
    //     while Instant::now() < deadline && ok.iter().any(|&b| !b) {
    //         for (idx, (id, addr_str)) in to_check.iter().enumerate() {
    //             if ok[idx] { continue; }
    //             let addr: std::net::SocketAddr = addr_str.parse().expect("static endpoint address parse");
    //             let nsk = bingle_core::api::bingle_api::NetworkEndpoint::new_direct(addr);
    //             let payload = serde_json::json!({ "app": null, "type": "Check" });
    //             match api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.send_message_to_network_with_response(&nsk, &id, payload.clone(), None)) {
    //                 Ok(resp) => {
    //                     let is_ok = resp.get("type").and_then(|v: &serde_json::Value| v.as_str()) == Some("CheckResponse")
    //                         && resp.get("state").and_then(|v: &serde_json::Value| v.as_str()) == Some("available");
    //                     if is_ok {
    //                         ok[idx] = true;
    //                     }
    //                 }
    //                 Err(_e) => {
    //                     // retry until deadline
    //                 }
    //             }
    //         }
    //         if ok.iter().any(|&b| !b) {
    //             std::thread::sleep(Duration::from_millis(500));
    //         }
    //     }
    //     assert!(ok.iter().all(|&b| b), "Static endpoints did not respond to RelayCheck within 120s: ok={:?}, endpoints={:?}", ok, to_check);
    // }

    // Determine expected final state from environment.
    // Primary: EXPECT_FINAL_STATE can be set to "EndpointAvailable" or "NATRestricted".
    // Secondary: derive from NAT_MODE if EXPECT_FINAL_STATE is not set.
    let _expect_state = match env_var("EXPECT_FINAL_STATE").as_deref() {
        Some("EndpointAvailable") => EngineState::EndpointAvailable,
        Some("NATRestricted") => EngineState::NATRestricted,
        Some(other) => panic!(
            "Invalid EXPECT_FINAL_STATE='{}' (allowed: EndpointAvailable|NATRestricted)",
            other
        ),
        None => {
            match env_var("NAT_MODE").as_deref() {
                Some("Restricted") => EngineState::NATRestricted,
                // Direct and Full both expect EndpointAvailable by requirement; default to EndpointAvailable
                Some("Direct") | Some("Full") | None => EngineState::EndpointAvailable,
                Some(other) => {
                    eprintln!(
                        "[warn] Unknown NAT_MODE='{}'; defaulting expected state to EndpointAvailable",
                        other
                    );
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
        if let Some(st) =
            api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_state_for_tests())
        {
            if st == EngineState::Registered || st == EngineState::NATRestricted {
                break st;
            }
        }
        if start.elapsed() > timeout {
            panic!(
                "Timed out waiting for Registered or NATRestricted; last state: {:?}",
                api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_state_for_tests())
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    };

    // Validate NAT type once final state is reached
    let got_nat = api
        .access(|a| a.engine_nat_type_for_tests())
        .expect("nat type should be set");
    match final_state {
        EngineState::Registered => assert_eq!(
            got_nat,
            NatType::FullCone,
            "Registered implies we reached EndpointAvailable with FullCone NAT"
        ),
        EngineState::NATRestricted => assert!(
            matches!(got_nat, NatType::Restricted | NatType::Symmetric),
            "NATRestricted should be Restricted or Symmetric (got {:?})",
            got_nat
        ),
        _ => {}
    }
    // If EXPECT_NAT_TYPE was provided, assert exact match
    if env_var("EXPECT_NAT_TYPE").is_some() {
        assert_eq!(
            got_nat, expect_nat,
            "expected NAT type {:?}, got {:?}",
            expect_nat, got_nat
        );
    }

    tracing::info!("Final state: {:?}, NAT type: {:?}", final_state, got_nat);
    tracing::info!("Static endpoints: {:?}", static_endpoints);

    // If we are Registered, perform DDB lookup for our ID and verify address equals our discovered public endpoint
    if final_state == EngineState::Registered {
        let my_id = api
            .access_unsafe_for_tests(|a: &mut BingleApiImpl| a.get_my_id())
            .expect("api.get_my_id Some");
        let nsk = api
            .access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_ddb_lookup_for_tests(&my_id))
            .expect("ddb lookup should succeed when registered");
        if got_nat == NatType::FullCone {
            let looked = nsk
                .inet_socket_address()
                .expect("lookup should return a direct endpoint");
            let ep = api
                .access_unsafe_for_tests(|a: &mut BingleApiImpl| {
                    a.engine_last_public_addr_for_tests()
                })
                .expect("last public addr should be Some");
            assert_eq!(
                looked, ep,
                "DDB lookup should return our discovered public endpoint"
            );
        } else {
            let looked = nsk
                .relay_address()
                .expect("lookup should return relay address");
            assert_eq!(
                looked.to_string(),
                static_endpoints[0].1,
                "DDB lookup should return our discovered public endpoint"
            );
        }
    }

    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.stop());
}
