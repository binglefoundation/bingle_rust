use rust_comms::engine::BingleAccessUnsafeForTests;
use rust_comms::api::bingle_api::{StartOptions, Handle, NetworkEndpoint, BingleApi};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use std::net::SocketAddr;
use std::sync::Arc;

#[path = "../test_util.rs"]
pub mod test_util;


#[cfg_attr(not(target_os = "ios"), test)]
pub fn bingle_api_relay_check_two_nodes() {


    // 1) Pick unused ports up-front for relay and client, and compute addresses
    let relay_port = test_util::find_unused_loopback_port();
    let client_port = test_util::find_unused_loopback_port();
    let relay_addr = SocketAddr::from(([127, 0, 0, 1], relay_port));
    let client_addr = SocketAddr::from(([127, 0, 0, 1], client_port));
    let relay = BingleApiImpl::new(&StartOptions::default());
    let relay_opts = StartOptions {
        handle: Handle::from("relay"),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(relay_addr),
        am_relay: true,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None, handle_cache_expiry: None,
    };
    relay.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.start(&relay_opts)).expect("relay start");
    if !test_util::wait_for_relay_available(&relay, std::time::Duration::from_secs(30)) {
        panic!("relay did not become Available within 30s");
    }

    // Install an on_message on the relay that responds to RelayCheck with RelayCheckResponse.
    // We need to send back to the client's socket address; use the pre-known client_addr.
    // For user_id, use the known ADDRESS_SPEND (client id).
    let relay_arc = relay.clone();
    let relay_for_cb = relay_arc.clone();
    let client_id = test_util::ADDRESS_SPEND.to_string();
    relay.access_unsafe_for_tests(|guard: &mut BingleApiImpl| {
        let client_addr_for_cb = client_addr.clone();
        guard.set_on_message(Some(Arc::new(move |sender_id, sender_handle, msg| {
            log::info!("[test][relay on_message] sender={} handle={} msg={}", sender_id, sender_handle, msg);
            let is_check = msg.get("type").and_then(|v: &serde_json::Value| v.as_str()) == Some("Check")
                && msg.get("app").map(|v| v.is_null()).unwrap_or(true);
            if is_check {
                let resp = serde_json::json!({
                    "app": null,
                    "type": "CheckResponse",
                    "state": "available", 
                });
                let nsk = NetworkEndpoint::new_direct(client_addr_for_cb);
                // Validate that locking succeeds and attempt to send the response.
                let _ok = relay_for_cb.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.send_message_to_network(&nsk, &client_id, resp, None));
            }
        })));
    });

    // 2) Start client node on the pre-selected port with PASSPHRASE_SPEND and id ADDRESS_SPEND
    let client = BingleApiImpl::new(&StartOptions::default());
    let client_opts = StartOptions {
        handle: Handle::from("client"),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()),
        static_ip: Some(client_addr),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None, handle_cache_expiry: None,
    };
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.start(&client_opts)).expect("client start");

    // 3) Send RelayCheck from client to relay directly
    let nsk_relay = NetworkEndpoint::new_direct(relay_addr);
    let relay_id = test_util::ADDRESS_RECEIVE.to_string();
    let payload = serde_json::json!({ "app": null, "type": "Check" });

    let response = client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_network_with_response(&nsk_relay, &relay_id, payload, None));
    assert!(response.is_ok(), "client send_message_to_network should return true");
    
    let seen = response.unwrap();
    assert_eq!(seen.get("app"), Some(&serde_json::Value::Null));
    assert_eq!(seen.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("CheckResponse"));
    assert_eq!(seen.get("state").and_then(|v: &serde_json::Value| v.as_str()), Some("available"));

    // Optional: stop nodes (best-effort)
    relay_arc.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
}
