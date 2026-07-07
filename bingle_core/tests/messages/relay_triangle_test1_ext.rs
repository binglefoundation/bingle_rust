use bingle_core::messages::marshal::{from_json_str, to_json_string};
use bingle_core::messages::types::{Message, RelayMessage, RelayTriangleTest1};
use std::net::SocketAddr;

#[test]
#[cfg(not(target_os = "ios"))]
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
        tag: None,
    };
    let out_json = to_json_string(&Message::Relay(RelayMessage::TriangleTest1(t1)));
    let v: serde_json::Value = serde_json::from_str(&out_json).expect("json parse");

    assert_eq!(
        v.get("type").and_then(|vv| vv.as_str()),
        Some("TriangleTest1")
    );
    let ex = v
        .get("doNotUseEndpoints")
        .and_then(|vv| vv.as_array())
        .expect("doNotUseEndpoints is an array");
    assert_eq!(ex.len(), 2);
    assert_eq!(
        ex[0].get("host").and_then(|vv| vv.as_str()),
        Some("5.6.7.8")
    );
    assert_eq!(ex[0].get("port").and_then(|vv| vv.as_u64()), Some(5678));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_relay_triangle_test1_json_no_exclusions() {
    let t1 = RelayTriangleTest1 {
        app: None,
        checking_endpoint: "1.2.3.4:1234".parse().unwrap(),
        do_not_use_endpoints: Vec::new(),
        tag: None,
    };
    let out_json = to_json_string(&Message::Relay(RelayMessage::TriangleTest1(t1)));
    let v: serde_json::Value = serde_json::from_str(&out_json).expect("json parse");

    // Should NOT have doNotUseEndpoints because of skip_serializing_if = "Vec::is_empty"
    assert!(v.get("doNotUseEndpoints").is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_relay_ping_handler_honors_exclusions() {
    use bingle_core::api::bingle_api::{
        BingleApiBoth, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler,
        ProgressCallback, StartOptions, UserId,
    };
    use bingle_core::dtls::{
        Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult, UdpNetworkMux,
    };
    use bingle_core::messages::handlers::MessageHandler;
    use bingle_core::messages::relay_ping_handler::RelayPingHandler;
    use std::sync::Arc;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct MockDtls {
        sends: Arc<Mutex<Vec<SocketAddr>>>,
    }
    impl Dtls for MockDtls {
        fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> DtlsResult<()> {
            Ok(())
        }
        fn stop(&mut self) -> DtlsResult<()> {
            Ok(())
        }
        fn send(&self, to: &NetworkEndpoint, _data: &[u8]) -> DtlsResult<()> {
            self.sends
                .lock()
                .unwrap()
                .push(to.inet_socket_address().unwrap());
            Ok(())
        }
        fn get_handle_message(&self) -> Option<HandleMessage> {
            None
        }
        fn set_handle_message(&mut self, _h: Option<HandleMessage>) {}
        fn set_handle_new_session(
            &mut self,
            _handler: Option<bingle_core::dtls::dtls_trait::HandleNewSession>,
        ) {
        }
        fn with_handle_message(self, _h: HandleMessage) -> Self {
            self
        }
        fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> {
            None
        }
        fn set_handle_peer_certificate(&mut self, _h: Option<HandlePeerCertificate>) {}
        fn with_handle_peer_certificate(self, _h: HandlePeerCertificate) -> Self {
            self
        }
        fn get_ca_cert(&self) -> Option<&[u8]> {
            None
        }
        fn set_ca_cert(&mut self, _p: Option<Vec<u8>>) {}
        fn with_ca_cert(self, _p: Vec<u8>) -> Self {
            self
        }
        fn get_client_cert(&self) -> Option<&[u8]> {
            None
        }
        fn set_client_cert(&mut self, _p: Option<Vec<u8>>) {}
        fn with_client_cert(self, _p: Vec<u8>) -> Self {
            self
        }
        fn get_client_private_key(&self) -> Option<&[u8]> {
            None
        }
        fn set_client_private_key(&mut self, _p: Option<Vec<u8>>) {}
        fn with_client_private_key(self, _p: Vec<u8>) -> Self {
            self
        }
        fn get_server_signing_cert(&self) -> Option<&[u8]> {
            None
        }
        fn set_server_signing_cert(&mut self, _p: Option<Vec<u8>>) {}
        fn with_server_signing_cert(self, _p: Vec<u8>) -> Self {
            self
        }
        fn get_server_signing_private_key(&self) -> Option<&[u8]> {
            None
        }
        fn set_server_signing_private_key(&mut self, _p: Option<Vec<u8>>) {}
        fn with_server_signing_private_key(self, _p: Vec<u8>) -> Self {
            self
        }
        fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
        fn with_app_layer_only_verification(self, _enabled: bool) -> Self
        where
            Self: Sized,
        {
            self
        }
        fn set_dangerous_debug(&mut self, _enabled: bool) {}
        fn with_dangerous_debug(self, _enabled: bool) -> Self
        where
            Self: Sized,
        {
            self
        }
        fn set_null_encryption(&mut self, _enabled: bool) {}
        fn with_null_encryption(self, _enabled: bool) -> Self
        where
            Self: Sized,
        {
            self
        }
        fn get_cipher_suite(&self, _endpoint: &NetworkEndpoint) -> Option<String> {
            None
        }
        fn forget_peers(&self) {}
    }

    struct MockApi;
    impl bingle_core::api::bingle_api::BingleApiInternal for MockApi {
        fn get_relay_state(&self) -> String {
            "off".to_string()
        }
    }
    impl bingle_core::api::bingle_api::BingleApi for MockApi {
        fn list_all_relays(
            &self,
            _include_self: bool,
        ) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
            Vec::new()
        }
        fn get_handle(&self) -> Option<String> {
            None
        }
        fn set_on_listening(
            &mut self,
            _h: Option<Arc<bingle_core::api::bingle_api::OnListeningHandler>>,
        ) {
        }
        fn get_algo_provider_config(
            &self,
        ) -> Option<bingle_core::blockchain::algo_ops::AlgoChainConfig> {
            None
        }
        fn get_user_id(&self) -> Option<String> {
            None
        }
        fn debug_print_options(&self) {}
        fn get_my_id(&self) -> Option<String> {
            Some("MYID".into())
        }
        fn get_app_id(&self) -> Option<u64> {
            None
        }
        fn start(
            &mut self,
            _o: &StartOptions,
        ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
            Ok(())
        }
        fn stop(&mut self) {}
        fn network_change(&mut self) {}
        fn handle_lookup(
            &self,
            _h: &Handle,
        ) -> Result<Option<UserId>, bingle_core::api::bingle_api::BingleError> {
            Ok(None)
        }
        fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> {
            None
        }
        fn send_message_to_id(
            &self,
            _u: &UserId,
            _m: serde_json::Value,
            _p: Option<Arc<ProgressCallback>>,
        ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
            Ok(false)
        }
        fn send_message_to_handle(
            &self,
            _h: &Handle,
            _m: serde_json::Value,
            _p: Option<Arc<ProgressCallback>>,
        ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
            Ok(false)
        }
        fn send_message_to_network(
            &self,
            _n: &NetworkEndpoint,
            _u: &UserId,
            _m: serde_json::Value,
            _p: Option<Arc<ProgressCallback>>,
        ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
            Ok(false)
        }
        fn send_message_to_id_with_response(
            &self,
            _u: &UserId,
            _m: serde_json::Value,
            _p: Option<Arc<ProgressCallback>>,
        ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
            Err(bingle_core::api::bingle_api::BingleError::Other("ni".into()))
        }
        fn send_message_to_handle_with_response(
            &self,
            _h: &Handle,
            _m: serde_json::Value,
            _p: Option<Arc<ProgressCallback>>,
        ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
            Err(bingle_core::api::bingle_api::BingleError::Other("ni".into()))
        }
        fn send_message_to_network_with_response(
            &self,
            _n: &NetworkEndpoint,
            _u: &UserId,
            _m: serde_json::Value,
            _p: Option<Arc<ProgressCallback>>,
        ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
            Err(bingle_core::api::bingle_api::BingleError::Other("ni".into()))
        }
        fn set_on_message(&mut self, _h: Option<Arc<OnMessageHandler>>) {}
        fn set_on_connect(&mut self, _h: Option<Arc<OnConnectHandler>>) {}
    }

    let sends = Arc::new(Mutex::new(vec![]));
    let mock_dtls = MockDtls {
        sends: sends.clone(),
    };
    let peer_addr: SocketAddr = "1.2.3.4:5678".parse().unwrap();
    let handler = RelayPingHandler::new(Arc::new(mock_dtls), Some(peer_addr));
    let api: Arc<dyn BingleApiBoth> = Arc::new(MockApi);
    let router = std::sync::Arc::new(bingle_core::messages::router::Router::new(
        std::sync::Arc::downgrade(&api),
    ));
    let from = bingle_core::messages::handlers::FromStruct::new(
        "OTHER".into(),
        NetworkEndpoint::new_direct("127.0.0.1:1".parse().unwrap()),
        router,
    );

    // Case 1: peer is NOT excluded
    let t1_ok = RelayTriangleTest1 {
        app: None,
        checking_endpoint: "10.0.0.1:1000".parse().unwrap(),
        do_not_use_endpoints: vec!["5.6.7.8:5678".parse().unwrap()],
        tag: None,
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
        tag: None,
    };
    handler.on_triangle_test1(api, &from, &t1_ex);
    assert_eq!(
        sends.lock().unwrap().len(),
        0,
        "should have skipped send because peer is excluded"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_relay_finder_honors_exclusions() {
    use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth};
    use crate::util::test_util::signed_root_relay;
    use bingle_core::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId};
    use bingle_core::relay::relay_finder::{RelayFinder, RelayFinderTrait};
    use std::sync::Arc;

    let id1 = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let id2 = "BAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let id3 = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let id3_str = id3.to_string();

    struct GetRelaysMockApi {
        id3: String,
        addr3: std::net::SocketAddr,
    }
    impl InnerBingleApi for GetRelaysMockApi {
        fn send_message_to_network_with_response(
            &self,
            _nsk: &NetworkEndpoint,
            _user_id: &UserId,
            msg: serde_json::Value,
            _p: Option<Arc<ProgressCallback>>,
        ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
            let ty = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if ty == "getRelaysStatus" {
                return Ok(serde_json::json!({
                    "app": "ddb",
                    "type": "relaysStatusResponse",
                    "epochId": -1,
                    "treeOrder": 2,
                    "responderState": "available",
                    "relayIds": [self.id3],
                    "relayEndpoints": [{"host": self.addr3.ip().to_string(), "port": self.addr3.port()}],
                    "relayStates": ["available"],
                }));
            }
            Err(bingle_core::api::bingle_api::BingleError::Other(
                "unexpected message".into(),
            ))
        }
    }

    let addr3: std::net::SocketAddr = "3.3.3.3:3333".parse().unwrap();
    let api_inner = Arc::new(MockApiBoth::new_with_api_override(Arc::new(
        GetRelaysMockApi {
            id3: id3_str,
            addr3,
        },
    )));
    let api: Arc<dyn bingle_core::api::bingle_api::BingleApiBoth> = api_inner;

    // Create 3 relays
    let r1 = signed_root_relay(id1, "1.1.1.1:1111".parse().unwrap());
    let r2 = signed_root_relay(id2, "2.2.2.2:2222".parse().unwrap());
    let r3 = signed_root_relay(id3, addr3);
    let all_relays = vec![r1.clone(), r2.clone(), r3.clone()];

    let discover = Arc::new(move || all_relays.clone());
    let finder = RelayFinder::new(Arc::downgrade(&api), discover);

    // Test 1: Exclude R1 and R2 -> relay_select_and_query returns R3 (only Available relay in response)
    let exclude = vec![r1.address(), r2.address()];
    let chosen = finder
        .find_relay_excluding("ME", &exclude)
        .expect("find relay");
    assert_eq!(chosen.id(), id3);

    // Test 2: Exclude all -> must return error
    let exclude_all = vec![r1.address(), r2.address(), r3.address()];
    let err = finder.find_relay_excluding("ME", &exclude_all);
    assert!(err.is_err());
}
