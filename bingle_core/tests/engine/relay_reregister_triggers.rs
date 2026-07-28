// Tests for the reconnect re-registration funnel (issue #50): after background/idle a client's
// TURN listener registration goes stale, so the engine refreshes it immediately on a reconnect
// signal (here, a fresh outbound DTLS session to the home relay) instead of waiting for the next
// periodic keep-alive tick. Each refresh re-sends Relay::Listen, counted below.
//
// A minimal DTLS stub and a FakeListenApi that counts Listen sends mirror relay_keep_alive_engine.rs.

use bingle_core::api::bingle_api::{BingleError, NetworkEndpoint, StartOptions};
use bingle_core::dtls::network_mux_udp::UdpNetworkMux;
use bingle_core::dtls::{Dtls, HandleMessage, HandlePeerCertificate};
use bingle_core::engine::{Engine, EngineState};
use bingle_core::messages::router::Router;
use bingle_core::relay::relay_finder::RelayInfo;
use std::net::SocketAddr;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use crate::util::reusable_mock_api::{InnerBingleApi, InnerBingleApiInternal, MockApiBoth};

#[derive(Clone)]
struct NullDtls;

impl Dtls for NullDtls {
    fn start(&self, _mux: Arc<UdpNetworkMux>) -> bingle_core::dtls::Result<()> {
        Ok(())
    }
    fn stop(&self) -> bingle_core::dtls::Result<()> {
        Ok(())
    }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> bingle_core::dtls::Result<()> {
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
    fn with_handle_message(self, _handler: HandleMessage) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> {
        None
    }
    fn set_handle_peer_certificate(&self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_ca_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_ca_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_private_key(&self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_private_key(&self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
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
    fn set_null_encryption(&self, _enabled: bool) {}
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

// Counts Relay::Listen sends (each successful registration / refresh sends one).
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
        _user_id: &bingle_core::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
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

fn build_engine() -> (Engine, Arc<AtomicUsize>) {
    let listen_count = Arc::new(AtomicUsize::new(0));
    let inner_api = Arc::new(FakeListenApi {
        listen_count: listen_count.clone(),
    });
    let mock_api = MockApiBoth::new_with_both_overrides(inner_api, Arc::new(RegisteringInternal));
    let api_weak = crate::util::reusable_mock_api::to_weak_api_both(mock_api);

    let eng = Engine::new_with_dtls(
        &StartOptions::new("test_reregister".into()),
        api_weak.clone(),
        Box::new(NullDtls),
    );
    eng.set_router(Arc::new(Router::new(api_weak)));
    // Long interval so the periodic keep-alive never fires a Listen during these tests.
    eng.test_set_relay_keep_alive_interval(std::time::Duration::from_secs(3600));
    (eng, listen_count)
}

fn relay_info(id: &str, addr: SocketAddr) -> RelayInfo {
    crate::util::test_util::signed_non_root_relay(id, addr)
}

// The mock api.set_state is a no-op, so drive the engine's `registered` atomic directly to mirror
// the production state after a successful registration.
fn mark_registered(eng: &Engine) {
    eng.set_state_internal(EngineState::Registered);
}

// A fresh outbound DTLS session to the home relay while registered must refresh the listener
// registration (re-send Listen) immediately — the core #50 fix.
#[ntest::timeout(20_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn outbound_session_to_home_relay_refreshes_registration() {
    let (eng, listen_count) = build_engine();
    let relay_addr: SocketAddr = "127.0.0.1:19960".parse().unwrap();

    eng.test_register_with_relay_direct(relay_info("relay_a", relay_addr));
    mark_registered(&eng);
    assert_eq!(
        listen_count.load(Ordering::SeqCst),
        1,
        "initial registration sends one Listen"
    );

    // Simulate the DTLS pipe to the home relay being rebuilt on resume.
    eng.test_on_outbound_session_established(&NetworkEndpoint::new_direct(relay_addr));

    assert_eq!(
        listen_count.load(Ordering::SeqCst),
        2,
        "a rebuilt session to the home relay must trigger an immediate re-Listen"
    );
    eng.stop();
}

// A fresh outbound session to some *other* peer/relay must NOT trigger a re-registration.
#[ntest::timeout(20_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn outbound_session_to_other_peer_does_not_refresh() {
    let (eng, listen_count) = build_engine();
    let relay_addr: SocketAddr = "127.0.0.1:19961".parse().unwrap();
    let other_addr: SocketAddr = "127.0.0.1:19962".parse().unwrap();

    eng.test_register_with_relay_direct(relay_info("relay_a", relay_addr));
    mark_registered(&eng);
    assert_eq!(listen_count.load(Ordering::SeqCst), 1);

    eng.test_on_outbound_session_established(&NetworkEndpoint::new_direct(other_addr));

    assert_eq!(
        listen_count.load(Ordering::SeqCst),
        1,
        "a rebuilt session to a non-home peer must not re-register"
    );
    eng.stop();
}

// Overlapping triggers within the debounce window collapse to a single refresh.
#[ntest::timeout(20_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn overlapping_triggers_are_debounced() {
    let (eng, listen_count) = build_engine();
    let relay_addr: SocketAddr = "127.0.0.1:19963".parse().unwrap();

    eng.test_register_with_relay_direct(relay_info("relay_a", relay_addr));
    mark_registered(&eng);
    assert_eq!(listen_count.load(Ordering::SeqCst), 1);

    // Two reconnect signals in quick succession (e.g. foreground immediately followed by the
    // send-driven DTLS rebuild) must yield only one extra Listen.
    eng.test_refresh_relay_registration("first");
    eng.test_refresh_relay_registration("second");

    assert_eq!(
        listen_count.load(Ordering::SeqCst),
        2,
        "refreshes within REREGISTER_DEBOUNCE must collapse to a single re-Listen"
    );
    eng.stop();
}

// The funnel is a no-op when we are not registered (no relay to refresh yet).
#[ntest::timeout(20_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn refresh_is_noop_when_not_registered() {
    let (eng, listen_count) = build_engine();
    let relay_addr: SocketAddr = "127.0.0.1:19964".parse().unwrap();

    // Registered with a relay but state is not Registered (e.g. torn down / mid-reconnect).
    eng.test_register_with_relay_direct(relay_info("relay_a", relay_addr));
    assert_eq!(listen_count.load(Ordering::SeqCst), 1);
    // Note: mark_registered intentionally NOT called — state() is not Registered.

    eng.test_refresh_relay_registration("resume");

    assert_eq!(
        listen_count.load(Ordering::SeqCst),
        1,
        "refresh must not re-register while the engine is not in the Registered state"
    );
    eng.stop();
}

// Returning to the foreground refreshes the registration immediately (the primary #50 trigger).
#[ntest::timeout(20_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn foreground_refreshes_registration() {
    let (eng, listen_count) = build_engine();
    let relay_addr: SocketAddr = "127.0.0.1:19965".parse().unwrap();

    eng.test_register_with_relay_direct(relay_info("relay_a", relay_addr));
    mark_registered(&eng);
    assert_eq!(listen_count.load(Ordering::SeqCst), 1);

    eng.on_foreground();

    assert_eq!(
        listen_count.load(Ordering::SeqCst),
        2,
        "foregrounding must re-register (re-send Listen) immediately"
    );
    eng.stop();
}

// Backgrounding pauses the keep-alive; foregrounding resumes it.
#[ntest::timeout(20_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn background_pauses_keepalive_and_foreground_resumes_it() {
    let (eng, _listen_count) = build_engine();
    let relay_addr: SocketAddr = "127.0.0.1:19966".parse().unwrap();

    eng.test_register_with_relay_direct(relay_info("relay_a", relay_addr));
    mark_registered(&eng);
    assert!(
        eng.relay_keep_alive_target_for_tests().is_some(),
        "precondition: registration starts the keep-alive"
    );

    eng.on_background();
    assert_eq!(
        eng.relay_keep_alive_target_for_tests(),
        None,
        "backgrounding must pause the keep-alive"
    );

    eng.on_foreground();
    assert_eq!(
        eng.relay_keep_alive_target_for_tests(),
        Some(("relay_a".to_string(), relay_addr)),
        "foregrounding must resume the keep-alive toward the known relay"
    );
    eng.stop();
}
