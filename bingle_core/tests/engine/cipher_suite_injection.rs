/// Tests that verify the Engine injects `cipher_suite` from the DTLS session
/// into received messages before routing them.
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bingle_core::api::bingle_api::{BingleApiBoth, NetworkEndpoint, StartOptions};
use bingle_core::dtls::network_mux_udp::UdpNetworkMux;
use bingle_core::dtls::{Dtls, HandleMessage, HandlePeerCertificate};
use bingle_core::engine::Engine;
use bingle_core::messages::handlers::MessageHandler;
use bingle_core::messages::router::Router;

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth};

/// A fake DTLS implementation that stores any installed handler and returns a configured cipher suite.
struct CipherFakeDtls {
    handler: Mutex<Option<HandleMessage>>,
    cipher_suite: Option<String>,
}

impl CipherFakeDtls {
    fn new(cipher_suite: Option<String>) -> Self {
        Self {
            handler: Mutex::new(None),
            cipher_suite,
        }
    }
}

impl Dtls for CipherFakeDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> bingle_core::dtls::Result<()> {
        Ok(())
    }
    fn stop(&mut self) -> bingle_core::dtls::Result<()> {
        Ok(())
    }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> bingle_core::dtls::Result<()> {
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> {
        self.handler.lock().expect("handler lock").clone()
    }
    fn set_handle_message(&mut self, handler: Option<HandleMessage>) {
        *self.handler.lock().expect("handler lock") = handler;
    }
    fn with_handle_message(self, handler: HandleMessage) -> Self
    where
        Self: Sized,
    {
        *self.handler.lock().expect("handler lock") = Some(handler);
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
    fn set_handle_new_session(
        &mut self,
        _handler: Option<bingle_core::dtls::dtls_trait::HandleNewSession>,
    ) {
    }
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_cipher_suite(&self, _endpoint: &NetworkEndpoint) -> Option<String> {
        self.cipher_suite.clone()
    }
    fn forget_peers(&self) {}
}

/// A minimal fake DTLS passed as `&dyn Dtls` (the `server` argument) to the installed handler closure.
/// Its `get_cipher_suite` returns the configured value so the engine injects it.
struct FakeServer {
    cipher_suite: Option<String>,
}

impl Dtls for FakeServer {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> bingle_core::dtls::Result<()> {
        Ok(())
    }
    fn stop(&mut self) -> bingle_core::dtls::Result<()> {
        Ok(())
    }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> bingle_core::dtls::Result<()> {
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> {
        None
    }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
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
    fn set_handle_new_session(
        &mut self,
        _handler: Option<bingle_core::dtls::dtls_trait::HandleNewSession>,
    ) {
    }
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_cipher_suite(&self, _endpoint: &NetworkEndpoint) -> Option<String> {
        self.cipher_suite.clone()
    }
    fn forget_peers(&self) {}
}

/// A capturing MessageHandler that records all raw JSON values passed to `on_unknown`.
struct CapturingHandler {
    captured: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl MessageHandler for CapturingHandler {
    fn on_unknown(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &bingle_core::messages::handlers::FromStruct,
        raw: &serde_json::Value,
    ) {
        self.captured
            .lock()
            .expect("captured lock")
            .push(raw.clone());
    }
}

/// A mock API that returns a known handle for any user id, allowing engine auth check to pass.
struct AlwaysAuthApi;
impl InnerBingleApi for AlwaysAuthApi {
    fn handle_lookup_by_id(
        &self,
        _user_id: &bingle_core::api::bingle_api::UserId,
    ) -> Option<bingle_core::api::bingle_api::Handle> {
        Some("test_handle".to_string())
    }
}

/// Build an engine with the given DTLS and capturing handler, install the DTLS handler,
/// and return the installed DTLS callback together with the captured-messages buffer.
fn build_engine_and_get_handler(
    cipher_suite: Option<String>,
) -> (
    HandleMessage,
    Arc<Mutex<Vec<serde_json::Value>>>,
    Arc<Router>,
) {
    let captured: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
    let capturing_handler = Arc::new(CapturingHandler {
        captured: captured.clone(),
    });

    let fake_dtls = CipherFakeDtls::new(cipher_suite);
    let api = crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(
        Arc::new(AlwaysAuthApi),
    ));
    let mut engine = Engine::new_with_dtls(&StartOptions::new("".into()), api, Box::new(fake_dtls));

    let router = Arc::new(Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
    ));
    engine.set_router(router.clone());
    engine.set_custom_message_handler(capturing_handler);

    engine
        .install_dtls_handler_for_tests()
        .expect("install_dtls_handler_for_tests failed");

    let handler = {
        let mut h: Option<HandleMessage> = None;
        engine.with_dtls_mut(|dtls| {
            h = dtls.get_handle_message();
        });
        h.expect("DTLS handler should be installed after install_dtls_handler_for_tests")
    };
    (handler, captured, router)
}

/// Send an unknown-typed JSON message through the engine handler and return the captured JSONs.
/// The `server_cipher_suite` is what `FakeServer.get_cipher_suite` returns — this is what the
/// engine reads when deciding whether to inject `cipher_suite`.
fn send_unknown_message(
    handler: &HandleMessage,
    router: Arc<Router>,
    server_cipher_suite: Option<String>,
    msg_json: serde_json::Value,
) {
    let server = FakeServer {
        cipher_suite: server_cipher_suite,
    };
    let from_ep = NetworkEndpoint::new_direct("127.0.0.1:9999".parse::<SocketAddr>().unwrap());
    let msg_bytes = serde_json::to_vec(&msg_json).expect("serialize test message");

    Router::with_current_router(router, || {
        handler(&server as &dyn Dtls, &from_ep, "test_issuer", &msg_bytes);
    });
}

/// Verify that the Engine injects `cipher_suite` from the DTLS session into the JSON before routing.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn cipher_suite_injected_into_routed_message() {
    let cipher = "TLS_AES_256_GCM_SHA384";
    let (handler, captured, router) = build_engine_and_get_handler(Some(cipher.to_string()));

    // An unknown app/type (no text field so it is not mistaken for PlainTextMessage) is routed
    // to on_unknown, which our CapturingHandler records.
    let msg_json = serde_json::json!({ "app": "test_app", "type": "test_type" });
    send_unknown_message(&handler, router, Some(cipher.to_string()), msg_json);

    let msgs = captured.lock().expect("captured lock");
    assert_eq!(
        msgs.len(),
        1,
        "Expected exactly one captured unknown message"
    );
    let delivered = &msgs[0];
    let cs = delivered
        .get("cipher_suite")
        .expect("cipher_suite field should be present in message after engine injection");
    assert_eq!(
        cs.as_str().expect("cipher_suite should be a string"),
        cipher,
        "cipher_suite value should match the one returned by DTLS"
    );
}

/// Verify that when DTLS returns None for `get_cipher_suite`, the field is NOT injected.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn cipher_suite_absent_when_dtls_returns_none() {
    let (handler, captured, router) = build_engine_and_get_handler(None);

    let msg_json = serde_json::json!({ "app": "test_app", "type": "test_type" });
    send_unknown_message(&handler, router, None, msg_json);

    let msgs = captured.lock().expect("captured lock");
    assert_eq!(
        msgs.len(),
        1,
        "Expected exactly one captured unknown message"
    );
    let delivered = &msgs[0];
    assert!(
        delivered.get("cipher_suite").is_none(),
        "cipher_suite should not be injected when DTLS.get_cipher_suite() returns None, got: {:?}",
        delivered.get("cipher_suite")
    );
}

/// Verify that when two different messages arrive with different cipher suites, each gets
/// the correct cipher_suite injected.  This guards against any accidental sharing of state.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn different_cipher_suites_injected_correctly() {
    let cipher_a = "TLS_AES_256_GCM_SHA384";
    let cipher_b = "TLS_AES_128_GCM_SHA256";

    let (handler_a, captured_a, router_a) =
        build_engine_and_get_handler(Some(cipher_a.to_string()));
    let (handler_b, captured_b, router_b) =
        build_engine_and_get_handler(Some(cipher_b.to_string()));

    let msg = serde_json::json!({ "app": "test_app", "type": "test_type" });
    send_unknown_message(
        &handler_a,
        router_a,
        Some(cipher_a.to_string()),
        msg.clone(),
    );
    send_unknown_message(&handler_b, router_b, Some(cipher_b.to_string()), msg);

    let msgs_a = captured_a.lock().expect("lock a");
    let msgs_b = captured_b.lock().expect("lock b");
    assert_eq!(msgs_a.len(), 1);
    assert_eq!(msgs_b.len(), 1);
    assert_eq!(
        msgs_a[0].get("cipher_suite").and_then(|v| v.as_str()),
        Some(cipher_a)
    );
    assert_eq!(
        msgs_b[0].get("cipher_suite").and_then(|v| v.as_str()),
        Some(cipher_b)
    );
}
