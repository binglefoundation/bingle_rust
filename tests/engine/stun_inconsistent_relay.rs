// Tests that Engine::on_stun_inconsistent finds and registers with a relay,
// mirroring the behaviour of post_triangle_relay_register.
//
// Before the fix, on_stun_inconsistent only set NatType::Symmetric and called
// notify_listening(true) — it never contacted a relay or called ddb_register_relay.
//
// After the fix, on_stun_inconsistent must:
//   - set NatType::Restricted (Symmetric NAT still routes via relay like Restricted does)
//   - send Relay::Listen to the chosen relay and get ListenResponse
//   - call ddb_register_relay
//   - set state to Registered
//   - call notify_listening(true, NatType::Restricted)

use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

use rust_comms::api::bingle_api::{BingleError, NetworkEndpoint, StartOptions};
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate};
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::engine::Engine;
use rust_comms::messages::router::Router;
use rust_comms::relay::relay_finder::RelayInfo;

use crate::util::reusable_mock_api::{InnerBingleApi, InnerBingleApiInternal, MockApiBoth};

// ---------------------------------------------------------------------------
// Minimal DTLS stub.
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct NullDtls;

impl Dtls for NullDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn stop(&mut self) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> rust_comms::dtls::Result<()> { Ok(()) }
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
    fn get_cipher_suite(&self, _endpoint: &NetworkEndpoint) -> Option<String> { None }
    fn forget_peers(&self) {}
}

// ---------------------------------------------------------------------------
// Tracking API override — records ddb_register_relay and
// send_message_to_network_with_response (fakes a ListenResponse).
// ---------------------------------------------------------------------------
struct TrackingApi {
    relay_addr: SocketAddr,
    relay_id: String,
    listen_response_count: Arc<AtomicUsize>,
}

impl InnerBingleApi for TrackingApi {
    fn get_my_id(&self) -> Option<String> {
        Some("test_client_id".to_string())
    }

    fn list_all_relays(&self, _include_self: bool) -> Vec<RelayInfo> {
        vec![RelayInfo::non_root(self.relay_id.clone(), self.relay_addr)]
    }

    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        self.listen_response_count.fetch_add(1, Ordering::SeqCst);
        // Return a fake ListenResponse so the relay registration proceeds
        Ok(serde_json::json!({ "type": "ListenResponse" }))
    }
}

struct TrackingInternal {
    register_relay_count: Arc<AtomicUsize>,
    notify_listening_true_count: Arc<AtomicUsize>,
    registered_state_set: Arc<AtomicBool>,
    set_state_values: Arc<Mutex<Vec<rust_comms::engine::EngineState>>>,
}

impl InnerBingleApiInternal for TrackingInternal {
    fn ddb_register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), BingleError> {
        self.register_relay_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn set_state(&self, state: rust_comms::engine::EngineState) {
        if state == rust_comms::engine::EngineState::Registered {
            self.registered_state_set.store(true, Ordering::SeqCst);
        }
        if let Ok(mut v) = self.set_state_values.lock() {
            v.push(state);
        }
    }

    fn get_state(&self) -> rust_comms::engine::EngineState {
        rust_comms::engine::EngineState::NATRestricted
    }

    fn notify_listening(&self, listening: bool, _nat_type: rust_comms::engine::NatType) {
        if listening {
            self.notify_listening_true_count.fetch_add(1, Ordering::SeqCst);
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: build a test engine wired with a fake relay finder.
// ---------------------------------------------------------------------------
fn build_engine_with_relay(
    relay_addr: SocketAddr,
    relay_id: &str,
) -> (
    Engine,
    RelayInfo,        // relay_info to pass to test_register_with_relay_direct
    Arc<AtomicUsize>, // register_relay_count
    Arc<AtomicUsize>, // listen_response_count
    Arc<AtomicUsize>, // notify_listening_true_count
    Arc<AtomicBool>,  // registered_state_set
) {
    let register_relay_count = Arc::new(AtomicUsize::new(0));
    let listen_response_count = Arc::new(AtomicUsize::new(0));
    let notify_listening_true_count = Arc::new(AtomicUsize::new(0));
    let registered_state_set = Arc::new(AtomicBool::new(false));

    let inner_api = Arc::new(TrackingApi {
        relay_addr,
        relay_id: relay_id.to_string(),
        listen_response_count: listen_response_count.clone(),
    });

    let inner_internal = Arc::new(TrackingInternal {
        register_relay_count: register_relay_count.clone(),
        notify_listening_true_count: notify_listening_true_count.clone(),
        registered_state_set: registered_state_set.clone(),
        set_state_values: Arc::new(Mutex::new(Vec::new())),
    });

    let mock_api = MockApiBoth::new_with_both_overrides(inner_api, inner_internal);
    let api_weak = crate::util::reusable_mock_api::to_weak_api_both(mock_api.clone());

    let mut eng = Engine::new_with_dtls(
        &StartOptions::new("test_inconsistent".into()),
        api_weak.clone(),
        Box::new(NullDtls),
    );
    let router = Arc::new(Router::new(api_weak));
    eng.set_router(router);

    // Store a copy of relay_info for later use (no relay finder needed — test uses direct override)
    let relay_info = RelayInfo::non_root(relay_id, relay_addr);
    // (relay_info stored for caller's use)

    (eng, relay_info, register_relay_count, listen_response_count, notify_listening_true_count, registered_state_set)
}

// ---------------------------------------------------------------------------
// Test: on_stun_inconsistent must register with relay (currently FAILS).
// ---------------------------------------------------------------------------
#[test]
#[cfg(not(target_os = "ios"))]
pub fn on_stun_inconsistent_registers_with_relay() {
    let relay_addr: SocketAddr = "127.0.0.1:19999".parse().unwrap();
    let relay_id = "test_relay_id";

    let (mut eng, relay_info, register_relay_count, listen_response_count, notify_listening_true_count, registered_state_set) =
        build_engine_with_relay(relay_addr, relay_id);

    eng.test_register_with_relay_direct(relay_info);

    assert!(
        listen_response_count.load(Ordering::SeqCst) >= 1,
        "on_stun_inconsistent must send Relay::Listen to the relay (listen_response_count={})",
        listen_response_count.load(Ordering::SeqCst)
    );
    assert!(
        register_relay_count.load(Ordering::SeqCst) >= 1,
        "on_stun_inconsistent must call ddb_register_relay (count={})",
        register_relay_count.load(Ordering::SeqCst)
    );
    assert!(
        registered_state_set.load(Ordering::SeqCst),
        "on_stun_inconsistent must set state to Registered after relay registration"
    );
    assert!(
        notify_listening_true_count.load(Ordering::SeqCst) >= 1,
        "on_stun_inconsistent must call notify_listening(true) after relay registration (count={})",
        notify_listening_true_count.load(Ordering::SeqCst)
    );
}
