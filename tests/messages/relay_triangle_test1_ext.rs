use std::net::SocketAddr;
use rust_comms::messages::types::{RelayTriangleTest1, Message, RelayMessage};
use rust_comms::messages::marshal::{from_json_str, to_json_string};

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_relay_triangle_test1_json_with_exclusions() {
    let json = r#"{
        "app": null,
        "type": "TriangleTest1",
        "checkingEndpoint": {"host": "1.2.3.4", "port": 1234},
        "doNotUseEndpoints": [
            {"host": "5.6.7.8", "port": 5678},
            {"host": "9.10.11.12", "port": 9012}
        ]
    }"#;
    
    let msg = from_json_str(json).expect("decode");
    match msg {
        Message::Relay(RelayMessage::TriangleTest1(m)) => {
            assert_eq!(m.checking_endpoint.host, "1.2.3.4");
            assert_eq!(m.checking_endpoint.port, 1234);
            assert_eq!(m.do_not_use_endpoints.len(), 2);
            assert_eq!(m.do_not_use_endpoints[0].host, "5.6.7.8");
            assert_eq!(m.do_not_use_endpoints[0].port, 5678);
            assert_eq!(m.do_not_use_endpoints[1].host, "9.10.11.12");
            assert_eq!(m.do_not_use_endpoints[1].port, 9012);
        }
        _ => panic!("expected Relay TriangleTest1"),
    }
    
    // Check serialization
    let t1 = RelayTriangleTest1 {
        app: None,
        checking_endpoint: "1.2.3.4:1234".parse().unwrap(),
        do_not_use_endpoints: vec![
            "5.6.7.8:5678".parse().unwrap(),
            "9.10.11.12:9012".parse().unwrap(),
        ],
    };
    let out_json = to_json_string(&Message::Relay(RelayMessage::TriangleTest1(t1)));
    let v: serde_json::Value = serde_json::from_str(&out_json).expect("json parse");
    
    assert_eq!(v.get("type").and_then(|vv| vv.as_str()), Some("TriangleTest1"));
    let ex = v.get("doNotUseEndpoints").and_then(|vv| vv.as_array()).expect("doNotUseEndpoints is an array");
    assert_eq!(ex.len(), 2);
    assert_eq!(ex[0].get("host").and_then(|vv| vv.as_str()), Some("5.6.7.8"));
    assert_eq!(ex[0].get("port").and_then(|vv| vv.as_u64()), Some(5678));
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_relay_triangle_test1_json_no_exclusions() {
    let t1 = RelayTriangleTest1 {
        app: None,
        checking_endpoint: "1.2.3.4:1234".parse().unwrap(),
        do_not_use_endpoints: Vec::new(),
    };
    let out_json = to_json_string(&Message::Relay(RelayMessage::TriangleTest1(t1)));
    let v: serde_json::Value = serde_json::from_str(&out_json).expect("json parse");
    
    // Should NOT have doNotUseEndpoints because of skip_serializing_if = "Vec::is_empty"
    assert!(v.get("doNotUseEndpoints").is_none());
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_relay_ping_handler_honors_exclusions() {
    use std::sync::Arc;
    use std::sync::Mutex;
    use rust_comms::messages::relay_ping_handler::RelayPingHandler;
    use rust_comms::messages::handlers::MessageHandler;
    use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult, UdpNetworkMux};
    use rust_comms::api::bingle_api::{BingleApiBoth, NetworkEndpoint, UserId, ProgressCallback, StartOptions, Handle, OnMessageHandler, OnConnectHandler};

    #[derive(Clone)]
    struct MockDtls { sends: Arc<Mutex<Vec<SocketAddr>>> }
    impl Dtls for MockDtls {
        fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
        fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
        fn send(&self, to: &NetworkEndpoint, _data: &[u8]) -> DtlsResult<()> {
            self.sends.lock().unwrap().push(to.inet_socket_address().unwrap());
            Ok(())
        }
        fn get_handle_message(&self) -> Option<HandleMessage> { None }
        fn set_handle_message(&mut self, _h: Option<HandleMessage>) {}
        fn with_handle_message(self, _h: HandleMessage) -> Self { self }
        fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> { None }
        fn set_handle_peer_certificate(&mut self, _h: Option<HandlePeerCertificate>) {}
        fn with_handle_peer_certificate(self, _h: HandlePeerCertificate) -> Self { self }
        fn get_ca_cert(&self) -> Option<&[u8]> { None }
        fn set_ca_cert(&mut self, _p: Option<Vec<u8>>) {}
        fn with_ca_cert(self, _p: Vec<u8>) -> Self { self }
        fn get_client_cert(&self) -> Option<&[u8]> { None }
        fn set_client_cert(&mut self, _p: Option<Vec<u8>>) {}
        fn with_client_cert(self, _p: Vec<u8>) -> Self { self }
        fn get_client_private_key(&self) -> Option<&[u8]> { None }
        fn set_client_private_key(&mut self, _p: Option<Vec<u8>>) {}
        fn with_client_private_key(self, _p: Vec<u8>) -> Self { self }
        fn get_server_signing_cert(&self) -> Option<&[u8]> { None }
        fn set_server_signing_cert(&mut self, _p: Option<Vec<u8>>) {}
        fn with_server_signing_cert(self, _p: Vec<u8>) -> Self { self }
        fn get_server_signing_private_key(&self) -> Option<&[u8]> { None }
        fn set_server_signing_private_key(&mut self, _p: Option<Vec<u8>>) {}
        fn with_server_signing_private_key(self, _p: Vec<u8>) -> Self { self }
    }

    struct MockApi;
    impl rust_comms::api::bingle_api::BingleApiInternal for MockApi {
        fn get_relay_state(&self) -> String { "off".to_string() }
        fn set_state(&self, _s: rust_comms::engine::EngineState) {}
        fn get_state(&self) -> rust_comms::engine::EngineState { rust_comms::engine::EngineState::StunIdentify }
        fn set_nat_type(&self, _n: rust_comms::engine::NatType) {}
        fn get_last_public_addr(&self) -> Option<SocketAddr> { None }
        fn ddb_register_ip(&self, _e: SocketAddr, _am_relay: bool) -> Result<(), String> { Err("ni".into()) }
        fn ddb_register_relay(&self, _r: String, _s: Option<String>) -> Result<(), String> { Err("ni".into()) }
        fn update_turn_listener_relay(&self, _r: String, _a: SocketAddr) -> Result<(), String> { Err("ni".into()) }
        fn turn_client_handle_listen_response(&self, _a: SocketAddr, _r: String) { }
        fn turn_lookup_addr_by_id(&self, _i: String) -> Option<SocketAddr> { None }
        fn turn_handle_call(&self, _s: SocketAddr, _d: SocketAddr) -> i32 { -1 }
        fn turn_handle_listen(&self, _i: String, _s: SocketAddr) -> bool { false }
        fn turn_handle_called(&self, _s: SocketAddr, _d: SocketAddr, _c: u16) {}
        fn notify_listening(&self, _l: bool) {}
    }
    impl rust_comms::api::bingle_api::BingleApi for MockApi {
        fn list_all_relays(&self, _include_self: bool) -> Vec<rust_comms::relay::relay_finder::RelayInfo> { Vec::new() }
        fn get_handle(&self) -> Option<String> { None }
        fn set_on_listening(&mut self, _h: Option<Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {}
        fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None }
        fn get_user_id(&self) -> Option<String> { None }
        fn debug_print_options(&self) {}
        fn get_my_id(&self) -> Option<String> { Some("MYID".into()) }
        fn get_app_id(&self) -> Option<u64> { None }
        fn start(&mut self, _o: &StartOptions) -> Result<(), String> { Ok(()) }
        fn stop(&mut self) {}
        fn network_change(&mut self) {}
        fn handle_lookup(&self, _h: &Handle) -> Result<Option<UserId>, String> { Ok(None) }
        fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> { None }
        fn send_message_to_id(&self, _u: &UserId, _m: serde_json::Value, _p: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_handle(&self, _h: &Handle, _m: serde_json::Value, _p: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_network(&self, _n: &NetworkEndpoint, _u: &UserId, _m: serde_json::Value, _p: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_id_with_response(&self, _u: &UserId, _m: serde_json::Value, _p: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
        fn send_message_to_handle_with_response(&self, _h: &Handle, _m: serde_json::Value, _p: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
        fn send_message_to_network_with_response(&self, _n: &NetworkEndpoint, _u: &UserId, _m: serde_json::Value, _p: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
        fn set_on_message(&mut self, _h: Option<Arc<OnMessageHandler>>) {}
        fn set_on_connect(&mut self, _h: Option<Arc<OnConnectHandler>>) {}
    }

    let sends = Arc::new(Mutex::new(vec![]));
    let mock_dtls = MockDtls { sends: sends.clone() };
    let peer_addr: SocketAddr = "1.2.3.4:5678".parse().unwrap();
    let handler = RelayPingHandler::new(Arc::new(mock_dtls), Some(peer_addr));
    let api: Arc<dyn BingleApiBoth> = Arc::new(MockApi);
    let from = rust_comms::messages::handlers::FromStruct { id: "OTHER".into(), network_source_key: NetworkEndpoint::new_direct("127.0.0.1:1".parse().unwrap()) };

    // Case 1: peer is NOT excluded
    let t1_ok = RelayTriangleTest1 {
        app: None,
        checking_endpoint: "10.0.0.1:1000".parse().unwrap(),
        do_not_use_endpoints: vec!["5.6.7.8:5678".parse().unwrap()],
    };
    handler.on_triangle_test1(api.clone(), &from, &t1_ok);
    assert_eq!(sends.lock().unwrap().len(), 1);
    assert_eq!(sends.lock().unwrap()[0], peer_addr);

    // Case 2: peer IS excluded
    sends.lock().unwrap().clear();
    let t1_ex = RelayTriangleTest1 {
        app: None,
        checking_endpoint: "10.0.0.1:1000".parse().unwrap(),
        do_not_use_endpoints: vec![peer_addr.into()],
    };
    handler.on_triangle_test1(api, &from, &t1_ex);
    assert_eq!(sends.lock().unwrap().len(), 0, "should have skipped send because peer is excluded");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_relay_finder_honors_exclusions() {
    use std::sync::Arc;
    use std::time::Duration;
    use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo};
    use crate::util::reusable_mock_api::{MockApiBoth, InnerBingleApi};
    use rust_comms::api::bingle_api::{NetworkEndpoint, UserId, ProgressCallback};

    struct CheckMockApi;
    impl InnerBingleApi for CheckMockApi {
        fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _msg: serde_json::Value, _p: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> {
            Ok(serde_json::json!({
                "type": "CheckResponse",
                "state": "available"
            }))
        }
    }

    let api_inner = Arc::new(MockApiBoth::new_with_api_override(Arc::new(CheckMockApi)));
    let api: Arc<dyn rust_comms::api::bingle_api::BingleApiBoth> = api_inner;

    // Use 58-char base32 strings to pass the 36-byte decode check in relay_check
    // The last character must have its trailing 2 bits zeroed for 288-bit (36-byte) data.
    // 'A' (0, 00000) works.
    let id1 = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let id2 = "BAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let id3 = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    // Create 3 relays
    let r1 = RelayInfo { id: id1.into(), address: "1.1.1.1:1111".parse().unwrap(), state: None };
    let r2 = RelayInfo { id: id2.into(), address: "2.2.2.2:2222".parse().unwrap(), state: None };
    let r3 = RelayInfo { id: id3.into(), address: "3.3.3.3:3333".parse().unwrap(), state: None };
    let all_relays = vec![r1.clone(), r2.clone(), r3.clone()];
    
    let discover = Arc::new(move || all_relays.clone());
    let finder = RelayFinder::new(Arc::downgrade(&api), Duration::from_secs(60), discover);
    
    // Test 1: Exclude R1 and R2 -> must pick R3
    let exclude = vec![r1.address, r2.address];
    let chosen = finder.find_relay_excluding("ME", &exclude).expect("find relay");
    assert_eq!(chosen.id, id3);
    
    // Test 2: Exclude all -> must return error
    let exclude_all = vec![r1.address, r2.address, r3.address];
    let err = finder.find_relay_excluding("ME", &exclude_all);
    assert!(err.is_err());
}
