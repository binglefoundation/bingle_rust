#![cfg(not(target_os = "ios"))]

use std::net::SocketAddr;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, StartOptions, UserId};
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use rust_comms::engine::Engine;
use rust_comms::messages::router::Router;

// Minimal DTLS mock: starts/stops successfully and never sends/receives anything.
#[derive(Clone)]
struct MockDtls;
impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<rust_comms::dtls::UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
    fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
    fn send(&self, _to: &rust_comms::api::bingle_api::NetworkEndpoint, _data: &[u8]) -> DtlsResult<()> { Ok(()) }
    fn get_handle_message(&self) -> Option<HandleMessage> { None }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
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
}

// Simple mock API required by Router; not used directly in this test.
#[derive(Clone)]
struct MockApi;
impl BingleApi for MockApi {
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_handle(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<dyn rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<dyn rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<dyn rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<dyn rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<dyn rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<dyn rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<dyn rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<dyn rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

// Internal capture to assert notify_listening(true) gets called.
struct CaptureInternal { pub notified: Arc<AtomicBool> }
impl rust_comms::api::bingle_api::BingleApiInternal for CaptureInternal {
    fn notify_listening(&self, listening: bool) { if listening { self.notified.store(true, Ordering::SeqCst); } }
}

// Verifies that when starting with a static address, the engine notifies listening=true as soon as DTLS accept loop is started.
#[test]
fn start_with_addr_notifies_listening_true() {
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
        log_level: None,
    };

    // Build Engine unbound and inject DTLS + Router with CaptureInternal
    let mut eng = Engine::new_unbound(&opts);
    eng.set_dtls(Box::new(MockDtls));
    let router = Arc::new(Router::new(Arc::new(MockApi)));
    let flag = Arc::new(AtomicBool::new(false));
    router.set_bingle_api_internal(Some(Arc::new(CaptureInternal { notified: flag.clone() }) as Arc<dyn rust_comms::api::bingle_api::BingleApiInternal>));
    eng.set_router(router);

    // Act
    eng.start(&opts).expect("engine.start should succeed");

    // Assert: the notification should arrive quickly
    let start = Instant::now();
    let timeout = Duration::from_secs(2);
    while !flag.load(Ordering::SeqCst) && start.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(flag.load(Ordering::SeqCst), "notify_listening(true) should be called for static-ip start");

    // Validate Option-returning helpers where applicable
    let local = eng.local_bind_addr_for_tests();
    assert!(local.is_some(), "local_bind_addr_for_tests should return Some");
}
