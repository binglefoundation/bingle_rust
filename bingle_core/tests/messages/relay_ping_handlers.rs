use std::net::SocketAddr;
use std::sync::Arc;

use bingle_core::api::bingle_api::{
    BingleApi, BingleApiBoth, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler,
    ProgressCallback, StartOptions, UserId,
};
use bingle_core::dtls::UdpNetworkMux;
use bingle_core::dtls::dtls_trait::{
    Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult,
};
use bingle_core::messages::handlers::MessageHandler;
use bingle_core::messages::*;

#[derive(Default, Clone)]
struct MockDtls {
    pub sent: Arc<std::sync::Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
}

impl Dtls for MockDtls {
    fn start(&self, _mux: Arc<UdpNetworkMux>) -> DtlsResult<()> {
        Ok(())
    }
    fn stop(&self) -> DtlsResult<()> {
        Ok(())
    }
    fn send(
        &self,
        to: &bingle_core::api::bingle_api::NetworkEndpoint,
        data: &[u8],
    ) -> DtlsResult<()> {
        let mut g = self.sent.lock().unwrap();
        let addr = to
            .inet_socket_address()
            .expect("MockDtls::send requires inet_socket_address");
        g.push((addr, data.to_vec()));
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> {
        None
    }
    fn set_handle_message(&self, _handler: Option<HandleMessage>) {}
    fn set_handle_new_session(
        &self,
        _handler: Option<bingle_core::dtls::dtls_trait::HandleNewSession>,
    ) {
    }
    fn with_handle_message(self, _handler: HandleMessage) -> Self {
        self
    }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> {
        None
    }
    fn set_handle_peer_certificate(&self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self {
        self
    }
    fn get_ca_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_ca_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self {
        self
    }
    fn get_client_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self {
        self
    }
    fn get_client_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_private_key(&self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self {
        self
    }
    fn get_server_signing_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self {
        self
    }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_private_key(&self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self {
        self
    }
    fn set_app_layer_only_verification(&self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_dangerous_debug(&self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_cipher_suite(
        &self,
        _endpoint: &bingle_core::api::bingle_api::NetworkEndpoint,
    ) -> Option<String> {
        None
    }
    fn forget_peers(&self) {}
    fn set_null_encryption(&self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn on_triangle_test1_sends_triangle_test2_to_peer() {
    let mock = Arc::new(MockDtls::default());
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let handler = RelayPingHandler::new(mock.clone(), Some(peer));

    let t1 = RelayTriangleTest1 {
        app: None,
        checking_endpoint: "127.0.0.1:12345".parse().unwrap(),
        do_not_use_endpoints: Vec::new(),
        tag: None,
    };
    // Construct minimal API and FromStruct
    struct MockApi;
    impl bingle_core::api::bingle_api::BingleApiInternal for MockApi {
        fn get_relay_state(&self) -> String {
            "off".to_string()
        }
    }
    impl BingleApi for MockApi {
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
            _handler: Option<std::sync::Arc<bingle_core::api::bingle_api::OnListeningHandler>>,
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
            Some("ME".into())
        }
        fn get_app_id(&self) -> Option<u64> {
            None
        }
        fn start(
            &mut self,
            _options: &StartOptions,
        ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
            Ok(())
        }
        fn stop(&mut self) {}
        fn network_change(&mut self) {}
        fn handle_lookup(
            &self,
            _handle: &Handle,
        ) -> Result<Option<UserId>, bingle_core::api::bingle_api::BingleError> {
            Ok(None)
        }
        fn handle_lookup_partial(
            &self,
            _handle: &Handle,
        ) -> Result<Option<(UserId, Handle)>, bingle_core::api::bingle_api::BingleError> {
            Ok(None)
        }
        fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> {
            None
        }
        fn send_message_to_id(
            &self,
            _user_id: &UserId,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
            Ok(false)
        }
        fn send_message_to_handle(
            &self,
            _handle: &Handle,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
            Ok(false)
        }
        fn send_message_to_network(
            &self,
            _nsk: &NetworkEndpoint,
            _user_id: &UserId,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
            Ok(false)
        }
        fn send_message_to_id_with_response(
            &self,
            _user_id: &UserId,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
            Err(bingle_core::api::bingle_api::BingleError::Other(
                "ni".into(),
            ))
        }
        fn send_message_to_handle_with_response(
            &self,
            _handle: &Handle,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
            Err(bingle_core::api::bingle_api::BingleError::Other(
                "ni".into(),
            ))
        }
        fn send_message_to_network_with_response(
            &self,
            _nsk: &NetworkEndpoint,
            _user_id: &UserId,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
            Err(bingle_core::api::bingle_api::BingleError::Other(
                "ni".into(),
            ))
        }
        fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
        fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
    }
    let api: Arc<dyn BingleApiBoth> = Arc::new(MockApi);
    let router = Arc::new(bingle_core::messages::router::Router::new(Arc::downgrade(
        &api,
    )));
    let from = bingle_core::messages::handlers::FromStruct::new(
        "FROM".into(),
        NetworkEndpoint::new_direct("127.0.0.1:1".parse().unwrap()),
        router,
    );
    handler.on_triangle_test1(api, &from, &t1);

    let records = mock.sent.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    let (to, data) = &records[0];
    assert_eq!(*to, peer);

    let text = std::str::from_utf8(&data).expect("utf8");
    let msg = from_json_str(text).expect("decode");
    match msg {
        Message::Relay(RelayMessage::TriangleTest2(m)) => {
            assert_eq!(m.checking_endpoint.to_string(), "127.0.0.1:12345");
        }
        other => panic!("expected TriangleTest2, got {:?}", other),
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn on_triangle_test2_sends_triangle_test3_to_endpoint() {
    let mock = Arc::new(MockDtls::default());
    let handler = RelayPingHandler::new(mock.clone(), None);

    let endpoint: SocketAddr = "127.0.0.1:33333".parse().unwrap();
    let t2 = RelayTriangleTest2 {
        app: None,
        checking_id: "id-abc".into(),
        checking_endpoint: endpoint.into(),
    };
    // Minimal API and FromStruct
    struct MockApi;
    impl bingle_core::api::bingle_api::BingleApiInternal for MockApi {
        fn get_relay_state(&self) -> String {
            "off".to_string()
        }
    }
    impl BingleApi for MockApi {
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
            _handler: Option<std::sync::Arc<bingle_core::api::bingle_api::OnListeningHandler>>,
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
            Some("ME".into())
        }
        fn get_app_id(&self) -> Option<u64> {
            None
        }
        fn start(
            &mut self,
            _options: &StartOptions,
        ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
            Ok(())
        }
        fn stop(&mut self) {}
        fn network_change(&mut self) {}
        fn handle_lookup(
            &self,
            _handle: &Handle,
        ) -> Result<Option<UserId>, bingle_core::api::bingle_api::BingleError> {
            Ok(None)
        }
        fn handle_lookup_partial(
            &self,
            _handle: &Handle,
        ) -> Result<Option<(UserId, Handle)>, bingle_core::api::bingle_api::BingleError> {
            Ok(None)
        }
        fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> {
            None
        }
        fn send_message_to_id(
            &self,
            _user_id: &UserId,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
            Ok(false)
        }
        fn send_message_to_handle(
            &self,
            _handle: &Handle,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
            Ok(false)
        }
        fn send_message_to_network(
            &self,
            _nsk: &NetworkEndpoint,
            _user_id: &UserId,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
            Ok(false)
        }
        fn send_message_to_id_with_response(
            &self,
            _user_id: &UserId,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
            Err(bingle_core::api::bingle_api::BingleError::Other(
                "ni".into(),
            ))
        }
        fn send_message_to_handle_with_response(
            &self,
            _handle: &Handle,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
            Err(bingle_core::api::bingle_api::BingleError::Other(
                "ni".into(),
            ))
        }
        fn send_message_to_network_with_response(
            &self,
            _nsk: &NetworkEndpoint,
            _user_id: &UserId,
            _message: serde_json::Value,
            _progress: Option<Arc<ProgressCallback>>,
        ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
            Err(bingle_core::api::bingle_api::BingleError::Other(
                "ni".into(),
            ))
        }
        fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
        fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
    }
    let api: Arc<dyn BingleApiBoth> = Arc::new(MockApi);
    let router = Arc::new(bingle_core::messages::router::Router::new(Arc::downgrade(
        &api,
    )));
    let from = bingle_core::messages::handlers::FromStruct::new(
        "FROM".into(),
        NetworkEndpoint::new_direct("127.0.0.1:1".parse().unwrap()),
        router,
    );
    handler.on_triangle_test2(api, &from, &t2);

    let records = mock.sent.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    let (to, data) = &records[0];
    assert_eq!(*to, endpoint);

    let text = std::str::from_utf8(&data).expect("utf8");
    let msg = from_json_str(text).expect("decode");
    match msg {
        Message::Relay(RelayMessage::TriangleTest3(_)) => {}
        other => panic!("expected TriangleTest3, got {:?}", other),
    }
}
