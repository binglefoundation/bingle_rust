use std::net::SocketAddr;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use std::time::{Duration, Instant};

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, StartOptions, UserId};
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use rust_comms::engine::Engine;
use rust_comms::messages::router::Router;
use crate::engine::ddb_upsert::test_util::init_test_logging;

// Minimal DTLS mock: starts/stops successfully and never sends/receives anything.
#[derive(Clone)]
struct MockDtls;
impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<rust_comms::dtls::UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
    fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
    fn send(&self, _to: &rust_comms::api::bingle_api::NetworkEndpoint, _data: &[u8]) -> DtlsResult<()> { Ok(()) }
    fn get_handle_message(&self) -> Option<HandleMessage> { None }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
    fn set_handle_new_session(&mut self, _handler: Option<rust_comms::dtls::dtls_trait::HandleNewSession>) {}
    fn with_handle_message(self, _handler: HandleMessage) -> Self where Self: Sized { self }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> { None }
    fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self where Self: Sized { self }
    fn get_ca_cert(&self) -> Option<&[u8]> { None }
    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_client_cert(&self) -> Option<&[u8]> { None }
    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_client_private_key(&self) -> Option<&[u8]> { None }
    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_server_signing_cert(&self) -> Option<&[u8]> { None }
    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> { None }
    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_dangerous_debug(&mut self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self where Self: Sized { self }
    fn get_cipher_suite(&self, _endpoint: &rust_comms::api::bingle_api::NetworkEndpoint) -> Option<String> { None }
    fn forget_peers(&self) {}
}

// Simple mock API required by Router.
#[derive(Clone)]
struct MockApi;
impl BingleApi for MockApi {
    fn list_all_relays(&self, _include_self: bool) -> Vec<rust_comms::relay::relay_finder::RelayInfo> { Vec::new() }
    fn set_on_listening(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {}
    fn get_user_id(&self) -> Option<String> { None }
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_handle(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), rust_comms::api::bingle_api::BingleError> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn handle_lookup(&self, _handle: &Handle) -> Result<Option<UserId>, rust_comms::api::bingle_api::BingleError> { Ok(None) }
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> { None }
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<bool, rust_comms::api::bingle_api::BingleError> { Ok(false) }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<bool, rust_comms::api::bingle_api::BingleError> { Ok(false) }
    fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<bool, rust_comms::api::bingle_api::BingleError> { Ok(false) }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> { Err(rust_comms::api::bingle_api::BingleError::Other("ni".into())) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> { Err(rust_comms::api::bingle_api::BingleError::Other("ni".into())) }
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> { Err(rust_comms::api::bingle_api::BingleError::Other("ni".into())) }
    fn set_on_message(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

impl rust_comms::api::bingle_api::BingleApiInternal for MockApi {
    fn get_relay_state(&self) -> String { "off".to_string() }
}

// Verifies that when starting with a static address, the engine invokes the listening handler with true.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn start_with_addr_notifies_listening_true() {
    init_test_logging();

    // Prepare start options with a static external address; local bind uses 0.0.0.0:<port>
    let static_addr: SocketAddr = "127.0.0.1:0".parse().expect("parse static addr");
    let opts = StartOptions {
        handle: "user_static".to_string(),
        algo_passphrase: None,
        static_ip: Some(static_addr),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None, handle_cache_expiry: None, dangerous_debug: true, log_mode: rust_comms::util::logging::LogMode::Plain, wait_response_timeout: None,
    };

    // Build Engine unbound and inject DTLS + Router with MockApi
    let flag = Arc::new(AtomicBool::new(false));
    let mock_api = MockApi;
    let mut eng = Engine::new(&opts, crate::util::mock_bingle_api::to_weak(mock_api.clone()));
    eng.set_dtls(Box::new(MockDtls));
    let router = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(mock_api)));
    eng.set_router(router);

    let listening_flag = flag.clone();
    eng.set_on_listening_handler(Some(Arc::new(move |listening, _nat_type: rust_comms::engine::NatType| {
        if listening {
            listening_flag.store(true, Ordering::SeqCst);
        }
    })));

    // Act
    eng.start(&opts).expect("engine.start should succeed");

    // Assert: the handler should be invoked quickly
    let start = Instant::now();
    let timeout = Duration::from_secs(2);
    while !flag.load(Ordering::SeqCst) && start.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(flag.load(Ordering::SeqCst), "OnListening handler should be called with true for static-ip start");

    // Validate Option-returning helpers where applicable
    let local = eng.local_bind_addr_for_tests();
    assert!(local.is_some(), "local_bind_addr_for_tests should return Some");
}
