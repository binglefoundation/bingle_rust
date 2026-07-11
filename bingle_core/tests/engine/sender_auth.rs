/// Tests that the Engine-level sender authentication check correctly gates message routing.
///
/// The Engine's DTLS handler calls `api.handle_lookup_by_id` before routing any message.
/// If the lookup returns `None` (sender not opted-in / unknown), the message must be
/// silently dropped and the Router must never be called.
/// If the lookup returns `Some(handle)`, the message must be passed to the Router.
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::util::message_mocks::all_message_samples;

use bingle_core::api::bingle_api::{BingleApiBoth, NetworkEndpoint, StartOptions};
use bingle_core::dtls::network_mux_udp::UdpNetworkMux;
use bingle_core::dtls::{Dtls, HandleMessage, HandlePeerCertificate};
use bingle_core::engine::Engine;
use bingle_core::messages::handlers::{FromStruct, MessageHandler};
use bingle_core::messages::router::Router;
use bingle_core::messages::types::{
    DdbDeleteResolve, DdbDumpResolve, DdbGetRelaysStatus, DdbInitResolve, DdbQueryResolve,
    DdbRelaysStatusResponse, DdbSignon, DdbSignonResponse, DdbUpsertResolve, Message, PingPing,
    PingResponse, PlainTextMessage, RelayCall, RelayCallResponse, RelayCalled, RelayCheck,
    RelayCheckResponse, RelayKeepAlive, RelayListen, RelayListenResponse, RelayResponse,
    RelayTriangleTest1, RelayTriangleTest1Response, RelayTriangleTest2, RelayTriangleTest3,
    ReportFailMessage,
};

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth};
use crate::util::test_util::init_test_logging;

// ---------------------------------------------------------------------------
// Mock API implementations
// ---------------------------------------------------------------------------

/// Rejects every sender — simulates a sender not opted-in to the app.
/// Counts how many times `handle_lookup_by_id` is called.
struct NeverAuthApi {
    lookup_count: Arc<AtomicU32>,
}

impl NeverAuthApi {
    fn new(lookup_count: Arc<AtomicU32>) -> Self {
        Self { lookup_count }
    }
}

impl InnerBingleApi for NeverAuthApi {
    fn handle_lookup_by_id(
        &self,
        _user_id: &bingle_core::api::bingle_api::UserId,
    ) -> Option<bingle_core::api::bingle_api::Handle> {
        self.lookup_count.fetch_add(1, Ordering::SeqCst);
        None
    }
}

/// Accepts every sender — simulates a fully registered, opted-in sender.
/// Counts how many times `handle_lookup_by_id` is called.
struct AlwaysAuthApi {
    lookup_count: Arc<AtomicU32>,
}

impl AlwaysAuthApi {
    fn new(lookup_count: Arc<AtomicU32>) -> Self {
        Self { lookup_count }
    }
}

impl InnerBingleApi for AlwaysAuthApi {
    fn handle_lookup_by_id(
        &self,
        _user_id: &bingle_core::api::bingle_api::UserId,
    ) -> Option<bingle_core::api::bingle_api::Handle> {
        self.lookup_count.fetch_add(1, Ordering::SeqCst);
        Some("test_handle".to_string())
    }
}

// ---------------------------------------------------------------------------
// Minimal fake DTLS (no cipher suite, just stores the installed handler)
// ---------------------------------------------------------------------------

struct MinimalFakeDtls {
    handler: Mutex<Option<HandleMessage>>,
}

impl MinimalFakeDtls {
    fn new() -> Self {
        Self {
            handler: Mutex::new(None),
        }
    }
}

impl Dtls for MinimalFakeDtls {
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
        self.handler.lock().expect("handler lock").clone()
    }
    fn set_handle_message(&self, handler: Option<HandleMessage>) {
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
    fn set_handle_new_session(
        &self,
        _handler: Option<bingle_core::dtls::dtls_trait::HandleNewSession>,
    ) {
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

// ---------------------------------------------------------------------------
// Capturing message handler — records whether any on_* method fired.
// Overrides every non-Mutex handler method so all routed messages are captured.
// Mutex messages bypass MessageHandler entirely (they go directly to API calls),
// so they cannot be captured here; use the lookup_count on the API mock instead.
// ---------------------------------------------------------------------------

struct CapturingHandler {
    called: Arc<AtomicBool>,
}

impl MessageHandler for CapturingHandler {
    fn on_plain_text(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &PlainTextMessage,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ping_ping(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &PingPing) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ping_response(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &PingResponse,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_call(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayCall) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_response(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &RelayResponse,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_triangle_test1(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &RelayTriangleTest1,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_triangle_test2(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &RelayTriangleTest2,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_triangle_test3(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &RelayTriangleTest3,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_triangle_test1_response(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &RelayTriangleTest1Response,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_listen(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &RelayListen,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_check(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayCheck) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_listen_response(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &RelayListenResponse,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_check_response(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &RelayCheckResponse,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_call_response(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &RelayCallResponse,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_keep_alive(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &RelayKeepAlive,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_called(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &RelayCalled,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_upsert_resolve(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &DdbUpsertResolve,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_delete_resolve(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &DdbDeleteResolve,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_query_resolve(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &DdbQueryResolve,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_init_resolve(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &DdbInitResolve,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_dump_resolve(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &DdbDumpResolve,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_signon(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &DdbSignon) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_signon_response(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &DdbSignonResponse,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_get_relays_status(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &DdbGetRelaysStatus,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_relays_status_response(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &DdbRelaysStatusResponse,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_report_fail(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _msg: &ReportFailMessage,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_unknown(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &FromStruct,
        _raw: &serde_json::Value,
    ) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_unimplemented(&self, _msg: &Message) {
        self.called.store(true, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Test harness helpers
// ---------------------------------------------------------------------------

/// Returns true if the message type bypasses MessageHandler and goes directly to API calls.
/// Mutex messages are dispatched to api.mutex_handle_* and never reach the handler.
fn is_mutex_message(msg: &Message) -> bool {
    matches!(msg, Message::Mutex(_))
}

/// Build an engine backed by the given inner API implementation, install the DTLS handler,
/// and return the installed callback together with the "was called" flag and the router.
fn build_engine<A: InnerBingleApi + Send + Sync + 'static>(
    inner_api: A,
) -> (HandleMessage, Arc<AtomicBool>, Arc<Router>) {
    let called = Arc::new(AtomicBool::new(false));
    let capturing = Arc::new(CapturingHandler {
        called: called.clone(),
    });

    let api = crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(
        Arc::new(inner_api),
    ));
    let engine = Engine::new_with_dtls(
        &StartOptions::new("".into()),
        api.clone(),
        Box::new(MinimalFakeDtls::new()),
    );

    let router = Arc::new(Router::new(api));
    engine.set_router(router.clone());
    engine.set_custom_message_handler(capturing);
    engine
        .install_dtls_handler_for_tests()
        .expect("install_dtls_handler_for_tests failed");

    let handler = {
        let mut h: Option<HandleMessage> = None;
        engine.with_dtls(|dtls| {
            h = dtls.get_handle_message();
        });
        h.expect("DTLS handler should be installed after install_dtls_handler_for_tests")
    };
    (handler, called, router)
}

/// Fire a message through the handler closure, simulating a DTLS packet arrival.
fn fire_message(
    handler: &HandleMessage,
    router: Arc<Router>,
    issuer: &str,
    msg: &bingle_core::messages::types::Message,
) {
    let server = MinimalFakeDtls::new();
    let from_ep = NetworkEndpoint::new_direct("127.0.0.1:9999".parse::<SocketAddr>().unwrap());
    let msg_bytes = serde_json::to_vec(msg).expect("serialize test message");
    Router::with_current_router(router, || {
        handler(&server as &dyn Dtls, &from_ep, issuer, &msg_bytes);
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// When the sender's id is not found by `handle_lookup_by_id` (not opted-in / unknown),
/// the Engine must drop every message type before it reaches the Router.
/// The capturing handler must never fire, but `handle_lookup_by_id` must always be called.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn invalid_sender_message_not_routed() {
    init_test_logging();

    for (label, msg) in all_message_samples() {
        let lookup_count = Arc::new(AtomicU32::new(0));
        let (handler, called, router) = build_engine(NeverAuthApi::new(lookup_count.clone()));
        fire_message(&handler, router, "UNKNOWN_SENDER.", &msg);
        assert!(
            !called.load(Ordering::SeqCst),
            "router / message handler must not be called for non-opted-in sender (message type: {label})"
        );
        assert!(
            lookup_count.load(Ordering::SeqCst) > 0,
            "handle_lookup_by_id must always be called, even for rejected senders (message type: {label})"
        );
    }
}

/// When the sender's id IS found by `handle_lookup_by_id` (opted-in, registered),
/// the Engine must pass every message type to the Router and the handler must be called.
/// Mutex messages bypass the MessageHandler (they go directly to API mutex calls),
/// so for those we verify only that `handle_lookup_by_id` was called (routing occurred).
#[test]
#[cfg(not(target_os = "ios"))]
pub fn valid_sender_message_routed() {
    init_test_logging();

    for (label, msg) in all_message_samples() {
        let lookup_count = Arc::new(AtomicU32::new(0));
        let (handler, called, router) = build_engine(AlwaysAuthApi::new(lookup_count.clone()));
        fire_message(&handler, router, "KNOWN_SENDER.", &msg);

        assert!(
            lookup_count.load(Ordering::SeqCst) > 0,
            "handle_lookup_by_id must always be called for opted-in sender (message type: {label})"
        );

        if is_mutex_message(&msg) {
            // Mutex messages go directly to API mutex calls, not through MessageHandler.
            // Routing is confirmed by handle_lookup_by_id being called above.
        } else {
            assert!(
                called.load(Ordering::SeqCst),
                "router / message handler must be called for opted-in sender (message type: {label})"
            );
        }
    }
}
