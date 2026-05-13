use rust_comms::engine::BingleAccessUnsafeForTests;


use std::net::SocketAddr;
use std::sync::Arc;

use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::api::bingle_api::{StartOptions, BingleApi, NetworkEndpoint};
use rust_comms::engine::EngineState;
use rust_comms::dtls::{Dtls, Result, UdpNetworkMux, HandleMessage, HandlePeerCertificate};

#[derive(Clone)]
struct MockDtls {
    handler: Arc<std::sync::Mutex<Option<HandleMessage>>>,
}
impl MockDtls { fn new() -> Self { Self { handler: Arc::new(std::sync::Mutex::new(None)) } } }
impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> Result<()> { Ok(()) }
    fn stop(&mut self) -> Result<()> { Ok(()) }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> Result<()> { Ok(()) }
    fn get_handle_message(&self) -> Option<HandleMessage> { self.handler.lock().unwrap().clone() }
    fn set_handle_message(&mut self, handler: Option<HandleMessage>) { *self.handler.lock().unwrap() = handler; }
    fn with_handle_message(self, handler: HandleMessage) -> Self where Self: Sized { let mut s = self; s.set_handle_message(Some(handler)); s }
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
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn triangle_test3_sets_engine_state_via_internal_api() {
    // Build API with injected DTLS so Engine exists and router is configured during start
    let mock = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock.clone()));

    // Start with static IP so Engine installs DTLS handler without STUN
    let opts = StartOptions { handle: "client".into(), algo_passphrase: None, static_ip: Some("127.0.0.1:0".parse().unwrap()), am_relay: false, stun_servers: None, algo_provider_config: None, algo_network: None, app_id: None, asset_id: None, log_level: None, handle_cache_expiry: None , dangerous_debug: true, log_mode: rust_comms::util::logging::LogMode::Plain };
    let _ = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts));

    // Ensure handler was installed
    let handler = mock.get_handle_message().expect("DTLS handler not installed");

    // Simulate receiving TriangleTest3 JSON from a peer
    let from: SocketAddr = "127.0.0.1:55555".parse().unwrap();
    let nsk = NetworkEndpoint::new_direct(from);
    let payload = serde_json::json!({"app": null, "type": "TriangleTest3"});
    let bytes = serde_json::to_vec(&payload).unwrap();

    // Call the handler; issuer string is arbitrary here
    handler(&mock, &nsk, "SOME-ISSUER", &bytes);

    // The message handler should have used the internal API to mark EndpointAvailable (or it might have already reached Registered)
    let st = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_state_for_tests());
    assert!(matches!(st, Some(EngineState::EndpointAvailable) | Some(EngineState::Registered)), "state not EndpointAvailable or Registered: {:?}", st);
}
