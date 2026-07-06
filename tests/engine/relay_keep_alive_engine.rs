// Tests that relay registration starts the periodic relay keep-alive and that
// engine lifecycle events (re-registration, STUN loss, stop) manage it correctly.
//
// The keep-alive interval is set very long so no send fires during these tests;
// only the sender lifecycle is observed via relay_keep_alive_target_for_tests.

use std::net::SocketAddr;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use rust_comms::api::bingle_api::{BingleError, NetworkEndpoint, StartOptions};
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate};
use rust_comms::engine::Engine;
use rust_comms::messages::router::Router;
use rust_comms::relay::relay_finder::RelayInfo;

use crate::util::reusable_mock_api::{InnerBingleApi, InnerBingleApiInternal, MockApiBoth};

// ---------------------------------------------------------------------------
// Minimal DTLS stub (mirrors tests/engine/stun_inconsistent_relay.rs).
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct NullDtls;

impl Dtls for NullDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> rust_comms::dtls::Result<()> {
        Ok(())
    }
    fn stop(&mut self) -> rust_comms::dtls::Result<()> {
        Ok(())
    }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> rust_comms::dtls::Result<()> {
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> {
        None
    }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
    fn set_handle_new_session(
        &mut self,
        _handler: Option<rust_comms::dtls::dtls_trait::HandleNewSession>,
    ) {
    }
    fn with_handle_message(self, _handler: HandleMessage) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> {
        None
    }
    fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_ca_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
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

// ---------------------------------------------------------------------------
// Mocks: fake ListenResponse so relay registration succeeds without a network.
// ---------------------------------------------------------------------------
struct FakeListenApi {
    listen_count: Arc<AtomicUsize>,
}

impl InnerBingleApi for FakeListenApi {
    fn get_my_id(&self) -> Option<String> {
        Some("test_client_id".to_string())
    }

    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        self.listen_count.fetch_add(1, Ordering::SeqCst);
        Ok(serde_json::json!({ "type": "ListenResponse" }))
    }
}

struct RegisteringInternal;

impl InnerBingleApiInternal for RegisteringInternal {
    fn ddb_register_relay(
        &self,
        _relay_id: String,
        _relay_sig: Option<String>,
    ) -> Result<(), BingleError> {
        Ok(())
    }
}

fn build_engine() -> Engine {
    let inner_api = Arc::new(FakeListenApi {
        listen_count: Arc::new(AtomicUsize::new(0)),
    });
    let mock_api = MockApiBoth::new_with_both_overrides(inner_api, Arc::new(RegisteringInternal));
    let api_weak = crate::util::reusable_mock_api::to_weak_api_both(mock_api);

    let mut eng = Engine::new_with_dtls(
        &StartOptions::new("test_keep_alive".into()),
        api_weak.clone(),
        Box::new(NullDtls),
    );
    eng.set_router(Arc::new(Router::new(api_weak)));
    // Long interval: these tests only observe sender lifecycle, never a send
    eng.test_set_relay_keep_alive_interval(Duration::from_secs(3600));
    eng
}

fn relay_info(id: &str, addr: SocketAddr) -> RelayInfo {
    crate::util::test_util::signed_non_root_relay(id, addr)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn registration_starts_keep_alive_with_relay_target() {
    let mut eng = build_engine();
    assert_eq!(eng.relay_keep_alive_target_for_tests(), None);

    let addr: SocketAddr = "127.0.0.1:19911".parse().unwrap();
    eng.test_register_with_relay_direct(relay_info("relay_a", addr));

    assert_eq!(
        eng.relay_keep_alive_target_for_tests(),
        Some(("relay_a".to_string(), addr)),
        "successful relay registration must start the keep-alive towards that relay"
    );
    eng.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn re_registration_replaces_keep_alive_target() {
    let mut eng = build_engine();

    let addr_a: SocketAddr = "127.0.0.1:19912".parse().unwrap();
    let addr_b: SocketAddr = "127.0.0.1:19913".parse().unwrap();
    eng.test_register_with_relay_direct(relay_info("relay_a", addr_a));
    eng.test_register_with_relay_direct(relay_info("relay_b", addr_b));

    assert_eq!(
        eng.relay_keep_alive_target_for_tests(),
        Some(("relay_b".to_string(), addr_b)),
        "re-registration must replace the previous keep-alive"
    );
    eng.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn stun_none_stops_keep_alive() {
    let mut eng = build_engine();

    let addr: SocketAddr = "127.0.0.1:19914".parse().unwrap();
    eng.test_register_with_relay_direct(relay_info("relay_a", addr));
    assert!(eng.relay_keep_alive_target_for_tests().is_some());

    eng.test_force_stun_none();
    assert_eq!(
        eng.relay_keep_alive_target_for_tests(),
        None,
        "losing STUN (network outage/change) must stop the keep-alive"
    );
    eng.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn engine_stop_stops_keep_alive() {
    let mut eng = build_engine();

    let addr: SocketAddr = "127.0.0.1:19915".parse().unwrap();
    eng.test_register_with_relay_direct(relay_info("relay_a", addr));
    assert!(eng.relay_keep_alive_target_for_tests().is_some());

    eng.stop();
    assert_eq!(
        eng.relay_keep_alive_target_for_tests(),
        None,
        "Engine::stop must stop the keep-alive"
    );
}

// Regression test for the STUN port-change re-registration bug: STUN becomes consistent on
// port P and the node registers with a relay (keep-alive running); the NAT then remaps and
// STUN becomes consistent on a NEW port Q. The node must re-register / reconnect to the relay
// for the new endpoint.
//
// The bug: Engine::stun_consistent_process detected the address change, tore down the relay
// keep-alive and forgot peers, set state = TrianglePing, then hit the
// `if self.state == EngineState::TrianglePing { return; }` guard and early-returned WITHOUT
// re-registering — leaving the keep-alive dead and the node stuck in TrianglePing forever.
//
// The fix: on an address change while registered, re-register with the remembered relay
// (Engine::last_registered_relay) on the new mapping before that guard, which restarts the
// keep-alive.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn stun_port_change_after_register_reconnects_relay() {
    let mut eng = build_engine();

    let port_p: SocketAddr = "203.0.113.7:1111".parse().unwrap();
    let port_q: SocketAddr = "203.0.113.7:2222".parse().unwrap();
    let relay_addr: SocketAddr = "127.0.0.1:19920".parse().unwrap();

    // STUN became consistent on port P and we registered with a relay: keep-alive is running.
    eng.set_last_public_addr(Some(port_p));
    eng.test_register_with_relay_direct(relay_info("relay_p", relay_addr));
    assert_eq!(
        eng.relay_keep_alive_target_for_tests(),
        Some(("relay_p".to_string(), relay_addr)),
        "precondition: registering while consistent on port P must start the relay keep-alive"
    );

    // NAT remaps: STUN is now consistent on a NEW port Q.
    eng.test_stun_consistent_process_with_addr(port_q);

    // The node must re-register / reconnect to the relay for the new endpoint, so the relay
    // keep-alive must be running again and still target the known relay.
    assert_eq!(
        eng.relay_keep_alive_target_for_tests(),
        Some(("relay_p".to_string(), relay_addr)),
        "after the STUN port changed P -> Q the engine must re-register and reconnect to the \
         known relay, restarting the keep-alive"
    );

    eng.stop();
}
