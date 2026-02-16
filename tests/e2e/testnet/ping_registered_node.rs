// tests/e2e/testnet/ping_registered_node.rs
// End-to-end test: start a user on testnet and send a Ping to a registered remote node (pinguser20).
// Requirements:
// - run only when BINGLE_RUN_TESTNET=1
// - load testnet config from nodely_testnet_node.json
// - ensure two root relays available via indexer
// - start user with TESTNET_USER/TESTNET_PASSPHRASE
// - wait for Registered or NATRestricted, assert Registered
// - send Ping to pinguser20 by Algorand address and expect PingResponse

#![cfg(not(target_os = "ios"))]

use std::sync::Arc;
use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::engine::{BingleAccess, BingleAccessUnsafeForTests};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;

#[path = "common.rs"]
mod common;

fn pingable_address() -> Option<String> {
    // Prefer explicit env var if provided by runner
    if let Some(addr) = common::env_var("PINGABLE_ADDRESS") { return Some(addr); }
    // Fallback to the known address used by scripts/run_testnet_tests.sh
    Some("SRIDF3MQNHGWOKYNZOSS7VONPKJB2LM52DOZGPY7QLT5ONZ5BPUAKH3Q4A".to_string())
}

#[test]
fn testnet_send_ping_to_registered_node() {
    // Only run when explicitly enabled.
    if common::env_var("BINGLE_RUN_TESTNET").as_deref() != Some("1") {
        eprintln!("[skipped] Set BINGLE_RUN_TESTNET=1 to run testnet e2e ping test");
        return;
    }

    let handle = common::env_var("TESTNET_USER").expect("TESTNET_USER env var must be set");
    let passphrase = common::env_var("TESTNET_PASSPHRASE").expect("TESTNET_PASSPHRASE env var must be set");

    // 1) Load network config and ensure relays
    let (network_name, provider_cfg, app_id, asset_id) = common::load_testnet_config();
    let ops = common::build_ops(&passphrase, &provider_cfg);
    common::ensure_two_relays(app_id, &ops);

    // 2) Load STUN servers and start API
    let stun_servers = common::load_stun_servers();
    let opts: StartOptions = common::make_start_options(&handle, &passphrase, &provider_cfg, network_name, app_id, asset_id, stun_servers);
    let (api, final_state): (Arc<BingleApiImpl>, EngineState) = common::start_api_and_wait(&opts);

    // 3) We require Registered for direct send
    assert_eq!(final_state, EngineState::Registered, "Expected Registered state before sending Ping (got {:?})", final_state);

    // 4) Compose Ping request
    let dest_id = pingable_address().expect("pingable address must resolve");
    // Validate Option returns Some where used later
    let my_id = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.get_my_id()).expect("api.get_my_id Some");
    assert!(!my_id.is_empty(), "my id should not be empty");

    let ping_req = serde_json::json!({
        "app": "ping",
        "type": "ping",
        "text": "hello from e2e",
    });

    // 5) Send and validate response with timing and progress
    use std::time::Instant;

    // Create progress callback that logs at INFO level
    let progress_callback = Arc::new(|progress: u8, message: String| {
        log::info!("Ping message progress: {}% - {}", progress, message);
    });

    // Time the send_message_to_id_with_response call
    let start_time = Instant::now();
    let resp = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.send_message_to_id_with_response(&dest_id, ping_req, Some(progress_callback)))
        .expect("send_message_to_id_with_response should succeed");
    let elapsed = start_time.elapsed();

    // Output timing clearly
    println!("TIMING: send_message_to_id_with_response took {:.3} seconds", elapsed.as_secs_f64());
    log::info!("send_message_to_id_with_response completed in {:.3} seconds", elapsed.as_secs_f64());

    // Expected: { app: "ping", type: "response", verifiedId: dest_id, text: "ACK: ..." }
    let app = resp.get("app").and_then(|v: &serde_json::Value| v.as_str());
    assert_eq!(app, Some("ping"), "response app should be 'ping': {:?}", resp);
    let rtype = resp.get("type").and_then(|v: &serde_json::Value| v.as_str());
    assert_eq!(rtype, Some("response"), "response type should be 'response': {:?}", resp);
    let vid = resp.get("verifiedId").and_then(|v: &serde_json::Value| v.as_str());
    assert_eq!(vid, Some(dest_id.as_str()), "verifiedId should equal destination id: {:?}", resp);
    let text = resp.get("text").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
    assert!(text.starts_with("ACK:"), "text should be an ACK: {:?}", resp);
}
