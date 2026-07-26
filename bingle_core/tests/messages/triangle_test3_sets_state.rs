use bingle_core::engine::BingleAccessUnsafeForTests;

use std::net::SocketAddr;
use std::sync::Arc;

use bingle_core::api::bingle_api::{BingleApi, NetworkEndpoint, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result, UdpNetworkMux};
use bingle_core::engine::EngineState;

#[derive(Clone)]
struct MockDtls {
    handler: Arc<std::sync::Mutex<Option<HandleMessage>>>,
}
impl MockDtls {
    fn new() -> Self {
        Self {
            handler: Arc::new(std::sync::Mutex::new(None)),
        }
    }
}
impl Dtls for MockDtls {
    fn start(&self, _mux: Arc<UdpNetworkMux>) -> Result<()> {
        Ok(())
    }
    fn stop(&self) -> Result<()> {
        Ok(())
    }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> Result<()> {
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> {
        self.handler.lock().unwrap().clone()
    }
    fn set_handle_message(&self, handler: Option<HandleMessage>) {
        *self.handler.lock().unwrap() = handler;
    }
    fn set_handle_new_session(
        &self,
        _handler: Option<bingle_core::dtls::dtls_trait::HandleNewSession>,
    ) {
    }
    fn with_handle_message(self, handler: HandleMessage) -> Self
    where
        Self: Sized,
    {
        let s = self;
        s.set_handle_message(Some(handler));
        s
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
    fn get_cipher_suite(
        &self,
        _endpoint: &bingle_core::api::bingle_api::NetworkEndpoint,
    ) -> Option<String> {
        None
    }
    fn forget_peers(&self) {}
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn triangle_test3_sets_engine_state_via_internal_api() {
    // Build API with injected DTLS so Engine exists and router is configured during start
    let mock = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock.clone()));

    // Start with static IP so Engine installs DTLS handler without STUN
    let opts = StartOptions {
        handle: "client".into(),
        algo_passphrase: None,
        static_ip: Some("127.0.0.1:0".parse().unwrap()),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    let _ = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts));

    // Ensure handler was installed
    let handler = mock
        .get_handle_message()
        .expect("DTLS handler not installed");

    // Simulate receiving TriangleTest3 JSON from a peer
    let from: SocketAddr = "127.0.0.1:55555".parse().unwrap();
    let nsk = NetworkEndpoint::new_direct(from);
    let payload = serde_json::json!({"app": null, "type": "TriangleTest3"});
    let bytes = serde_json::to_vec(&payload).unwrap();

    // Call the handler; issuer string is arbitrary here
    handler(&mock, &nsk, "SOME-ISSUER", &bytes);

    // The message handler should have used the internal API to mark EndpointAvailable (or it might have already reached Registered)
    let st = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_state_for_tests());
    assert!(
        matches!(
            st,
            Some(EngineState::EndpointAvailable) | Some(EngineState::Registered)
        ),
        "state not EndpointAvailable or Registered: {:?}",
        st
    );
}

/// Integration for #41 (reproduces the client-side scratch_6 sequence at the real Engine level).
///
/// After a STUN public-address change resets the node (nat_type back to Unknown, not listening) the
/// `registered` flag can remain set, so `state()` still reports `Registered`. A TriangleTest1Response
/// with `noCornerNode=true` arriving in that stale state previously logged "ignoring due to
/// state=Registered" and the client stayed Unknown forever. It must instead re-classify to a
/// concrete NAT type (NATRestricted). This drives the real BingleApiImpl → Engine → Router →
/// DefaultPrintingHandler path (exercising the real state()/nat_type() atomics and the router's
/// LockingApiWrapper NAT forwarding).
#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_corner_node_reclassifies_stale_registered_via_real_engine() {
    use bingle_core::api::bingle_api::BingleApiBoth;
    use bingle_core::engine::NatType;
    use bingle_core::messages::handlers::DefaultPrintingHandler;
    use bingle_core::messages::router::Router;
    use bingle_core::messages::types::{Message, RelayMessage, RelayTriangleTest1Response};

    // A real BingleApiImpl owns a live Engine (no start() needed). Route a no-corner response
    // through a Router backed by that real engine so the fix is exercised end-to-end against the
    // real state()/nat_type() atomics and the router's LockingApiWrapper NAT forwarding.
    let api_impl = BingleApiImpl::new_with_dtls(Box::new(MockDtls::new()));
    let api_dyn: Arc<dyn BingleApiBoth> = api_impl.clone();
    let router = Arc::new(Router::new(Arc::downgrade(&api_dyn)));

    // Force the stale post-reset condition: Registered flag set, but NAT back to Unknown
    // (as happens after a STUN public-address change reset — see scratch_6 / issue #41).
    api_dyn.set_state(EngineState::Registered);
    api_dyn.set_nat_type(NatType::Unknown);
    assert_eq!(
        api_dyn.get_state(),
        EngineState::Registered,
        "precondition: engine should report a (stale) Registered state"
    );
    assert_eq!(api_dyn.get_nat_type(), NatType::Unknown);

    // Deliver a no-corner TriangleTest1Response.
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(
        RelayTriangleTest1Response {
            app: None,
            no_corner_node: true,
            response_tag: None,
        },
    ));
    Router::with_current_router(router.clone(), || {
        router.route(&DefaultPrintingHandler, &msg, "FROMID");
    });

    // The fallback runs on a spawned thread; poll for re-classification to a concrete NAT type.
    let start = std::time::Instant::now();
    let mut nat = NatType::Unknown;
    while start.elapsed() < std::time::Duration::from_secs(5) {
        nat = api_dyn.get_nat_type();
        if nat == NatType::Restricted {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(
        nat,
        NatType::Restricted,
        "a stale Registered node with Unknown NAT must re-classify to Restricted (issue #41), got {:?}",
        nat
    );
}
