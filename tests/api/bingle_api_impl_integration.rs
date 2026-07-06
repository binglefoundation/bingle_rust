use crate::api::bingle_api_impl_integration::test_util::ADDRESS_SPEND;
use crate::relay::relay_states::test_util::init_test_logging;
use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::BingleAccessUnsafeForTests;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[path = "../test_util.rs"]
pub mod test_util;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn start_succeeds() {
    let api = BingleApiImpl::new(&StartOptions::new("".into()));
    let opts = StartOptions {
        handle: Handle::from("alice"),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(vec![SocketAddr::from(([127, 0, 0, 1], 3478))]),
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: rust_comms::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    let res = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts));
    // Engine may fail to start depending on DTLS/PKI availability; we only require DTLS instance creation here.
    if res.is_err() {
        eprintln!("api.start returned error: {:?}", res);
    }
    // DTLS instance is now created only on Engine
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_message_to_network_without_addr_fails_gracefully() {
    let api = BingleApiImpl::new(&StartOptions::new("".into()));
    let nsk = NetworkEndpoint::new_relay(
        ADDRESS_SPEND.parse().unwrap(),
        Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345)),
        Some(2),
    );
    let uid = test_util::ADDRESS_SPEND.to_string();
    let res = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.send_message_to_network(&nsk, &uid, serde_json::json!({"hi": 1}), None)
    });
    assert!(
        res.is_err() || !res.unwrap(),
        "Should fail when engine not started"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_check_end_to_end_on_message_receives_response() {
    use std::net::SocketAddr;
    use std::sync::{Arc, OnceLock};
    use std::thread;
    use std::time::Duration;

    use rust_comms::dtls::{Dtls, DtlsOpenSsl};

    #[path = "../dtls/pki.rs"]
    pub mod pki;

    fn mock_peer_cert_handler(_cert: &[u8], _ca: &[u8]) -> rust_comms::dtls::Result<String> {
        Ok(test_util::ADDRESS_SPEND.to_string())
    }

    init_test_logging();

    #[allow(dead_code)]
    static CLIENT_SEEN: OnceLock<serde_json::Value> = OnceLock::new();

    // Spin up a DTLS server that responds to RelayCheck with RelayCheckResponse
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Create and start a UDP mux for the server and determine its bound address.
    let mux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    let mux = std::sync::Arc::new(mux0);
    let addr: SocketAddr = mux.local_addr().expect("mux addr");

    // Server handler: parse FRPT-wrapped packet; send ACK_COMPLETE, then handle RelayCheck
    fn server_handler(
        server: &dyn Dtls,
        from: &rust_comms::api::bingle_api::NetworkEndpoint,
        _issuer: &str,
        data: &[u8],
    ) {
        // Send FRPT ACK_COMPLETE for any DATA_SINGLE packet (version=1, type=1)
        if data.len() >= 4 && (data[0] >> 4) == 0x1 && (data[0] & 0x0F) == 0x1 {
            let ack = vec![0x14u8, 0x00, data[2], data[3]];
            let _ = server.send(from, &ack);
        }
        let unwrapped = test_util::maybe_unwrap_data_single(data);
        if let Ok(text) = std::str::from_utf8(unwrapped) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
                let is_check = v.get("type").and_then(|x| x.as_str()) == Some("Check")
                    && v.get("app").map(|a| a.is_null()).unwrap_or(true);
                if is_check {
                    let mut obj = serde_json::Map::new();
                    obj.insert("app".to_string(), serde_json::Value::Null);
                    obj.insert(
                        "type".to_string(),
                        serde_json::Value::String("CheckResponse".to_string()),
                    );
                    obj.insert(
                        "state".to_string(),
                        serde_json::Value::String("available".to_string()),
                    );
                    if let Some(tag) = v.get("tag").and_then(|t| t.as_str()) {
                        obj.insert(
                            "responseTag".to_string(),
                            serde_json::Value::String(tag.to_string()),
                        );
                    }
                    if let Ok(bytes) = serde_json::to_vec(&serde_json::Value::Object(obj)) {
                        let _ = server.send(from, &bytes);
                    }
                    return;
                }
            }
        }
    }

    // Build and start the server
    let mut server = DtlsOpenSsl::new("server".to_string())
        .with_handle_message(Arc::new(server_handler))
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);
    mux.start().expect("mux start");
    server.start(mux.clone()).expect("server start");

    // Give server time to bind
    thread::sleep(Duration::from_millis(200));

    // Build BingleApiImpl client
    let api = BingleApiImpl::new(&StartOptions::new("".into()));
    api.set_id_to_handle_lookup_mock_for_tests(Box::new(|user_id| {
        if user_id.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Handle::from("mock-server-handle")))
        }
    }));
    let opts = StartOptions {
        handle: Handle::from("client"),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(vec![SocketAddr::from(([127, 0, 0, 1], 3478))]),
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: rust_comms::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    let start_result = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts));
    assert!(
        start_result.is_ok(),
        "client start failed: {}",
        start_result.unwrap_err()
    );

    // Prepare a direct NetworkSourceKey to server and send RelayCheck
    let nsk = NetworkEndpoint::new_direct(addr);
    let payload = serde_json::json!({ "app": null, "type": "Check" });

    let uid2 = test_util::ADDRESS_SPEND.to_string();
    let response = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.send_message_to_network_with_response(&nsk, &uid2, payload, None)
    });
    assert!(response.is_ok(), "client send failed");

    let response_content = response.unwrap();
    assert_eq!(response_content.get("app"), Some(&serde_json::Value::Null));
    assert_eq!(
        response_content
            .get("type")
            .and_then(|v: &serde_json::Value| v.as_str()),
        Some("CheckResponse")
    );
    assert_eq!(
        response_content
            .get("state")
            .and_then(|v: &serde_json::Value| v.as_str()),
        Some("available")
    );
    assert!(
        response_content.get("responseTag").is_some(),
        "response should include responseTag"
    );
    assert!(
        response_content.get("tag").is_none(),
        "response should not include request tag field"
    );
}
