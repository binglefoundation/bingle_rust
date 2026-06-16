/// Integration tests verifying that `Engine::route_incoming` correctly gates message routing
/// based on the sender's blockchain registration status.
///
/// These tests complement the unit tests in `tests/engine/sender_auth.rs` (which use mock APIs)
/// by driving the full path: a real `BingleApiImpl` backed by algokit localnet, so that
/// `handle_lookup_by_id` performs actual on-chain lookups.
///
/// The key assertions:
/// - When the sender is registered on-chain, the message reaches the custom handler.
/// - When the sender is not registered on-chain, the message is silently dropped.
///
/// Run with:
///   cargo test --test integration route_incoming_sender_auth_localnet
///
/// Prerequisites: `algokit localnet start` must be running.
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{
    BingleApiBoth, NetworkEndpoint, StartOptions,
};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate};
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::engine::Engine;
use rust_comms::messages::handlers::{FromStruct, MessageHandler};
use rust_comms::messages::router::Router;
use rust_comms::messages::types::{
    DdbDeleteResolve, DdbDumpResolve, DdbGetRelaysStatus, DdbInitResolve,
    DdbQueryResolve, DdbRelaysStatusResponse, DdbSignon, DdbSignonResponse,
    DdbUpsertResolve, Message, PingPing, PingResponse, PlainTextMessage,
    RelayCall, RelayCallResponse, RelayCalled, RelayCheck, RelayCheckResponse, RelayKeepAlive,
    RelayListen, RelayListenResponse, RelayResponse, RelayTriangleTest1,
    RelayTriangleTest1Response, RelayTriangleTest2, RelayTriangleTest3, ReportFailMessage,
};
use rust_comms::protocol::ISSUER_SUFFIX;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;

// ---------------------------------------------------------------------------
// Minimal fake DTLS — stores the installed handler so we can invoke it directly.
// ---------------------------------------------------------------------------

struct FakeDtls {
    handler: Mutex<Option<HandleMessage>>,
}

impl FakeDtls {
    fn new() -> Self {
        Self { handler: Mutex::new(None) }
    }
}

impl Dtls for FakeDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn stop(&mut self) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn get_handle_message(&self) -> Option<HandleMessage> {
        self.handler.lock().expect("handler lock").clone()
    }
    fn set_handle_message(&mut self, handler: Option<HandleMessage>) {
        *self.handler.lock().expect("handler lock") = handler;
    }
    fn with_handle_message(self, handler: HandleMessage) -> Self where Self: Sized {
        *self.handler.lock().expect("handler lock") = Some(handler);
        self
    }
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
    fn set_handle_new_session(&mut self, _handler: Option<rust_comms::dtls::dtls_trait::HandleNewSession>) {}
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self where Self: Sized { self }
    fn get_cipher_suite(&self, _endpoint: &NetworkEndpoint) -> Option<String> { None }
    fn forget_peers(&self) {}
}

// ---------------------------------------------------------------------------
// Capturing handler — records whether any on_* method fired.
// ---------------------------------------------------------------------------

struct CapturingHandler {
    called: Arc<AtomicBool>,
}

impl MessageHandler for CapturingHandler {
    fn on_plain_text(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &PlainTextMessage) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ping_ping(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &PingPing) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ping_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &PingResponse) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_call(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayCall) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayResponse) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_triangle_test1(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayTriangleTest1) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_triangle_test2(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayTriangleTest2) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_triangle_test3(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayTriangleTest3) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_triangle_test1_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayTriangleTest1Response) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_listen(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayListen) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_check(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayCheck) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_listen_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayListenResponse) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_check_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayCheckResponse) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_call_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayCallResponse) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_keep_alive(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayKeepAlive) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_relay_called(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayCalled) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_upsert_resolve(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &DdbUpsertResolve) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_delete_resolve(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &DdbDeleteResolve) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_query_resolve(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &DdbQueryResolve) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_init_resolve(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &DdbInitResolve) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_dump_resolve(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &DdbDumpResolve) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_signon(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &DdbSignon) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_signon_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &DdbSignonResponse) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_get_relays_status(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &DdbGetRelaysStatus) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_ddb_relays_status_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &DdbRelaysStatusResponse) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_report_fail(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &ReportFailMessage) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_unknown(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _raw: &serde_json::Value) {
        self.called.store(true, Ordering::SeqCst);
    }
    fn on_unimplemented(&self, _msg: &Message) {
        self.called.store(true, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `StartOptions` that points at the given localnet `app_id`.
fn make_opts(app_id: u64) -> StartOptions {
    StartOptions {
        handle: "route_incoming_test".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()),
        static_ip: None,
        am_relay: false,
        stun_servers: None,
        algo_provider_config: Some(test_util::localnet_config()),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false,
        log_mode: rust_comms::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    }
}

/// Build a `BingleApiImpl` backed by a fake DTLS, set up a capturing handler on its
/// engine, install the DTLS handler, and return `(installed_callback, called_flag, router)`.
fn build_api_and_handler(app_id: u64) -> (Arc<BingleApiImpl>, HandleMessage, Arc<AtomicBool>, Arc<Router>) {
    let called = Arc::new(AtomicBool::new(false));
    let capturing = Arc::new(CapturingHandler { called: called.clone() });

    let opts = make_opts(app_id);
    let api = BingleApiImpl::new_with_dtls_and_options(Box::new(FakeDtls::new()), opts.clone());

    let router = api.with_engine_mut(|engine: &mut Engine| {
        let weak = Arc::downgrade(&api) as std::sync::Weak<dyn BingleApiBoth>;
        let r = Arc::new(Router::new(weak));
        engine.set_router(r.clone());
        engine.set_custom_message_handler(capturing);
        engine.install_dtls_handler_for_tests().expect("install_dtls_handler_for_tests");
        r
    });

    let handler = api.with_engine_mut(|engine: &mut Engine| {
        let mut h: Option<HandleMessage> = None;
        engine.with_dtls_mut(|dtls| { h = dtls.get_handle_message(); });
        h.expect("DTLS handler must be installed")
    });

    (api, handler, called, router)
}

/// Fire a serialised `PlainTextMessage` through the installed DTLS handler closure,
/// simulating a packet arriving from `issuer`.
fn fire_plain_text(handler: &HandleMessage, router: Arc<Router>, issuer: &str) {
    let dtls = FakeDtls::new();
    let from_ep = NetworkEndpoint::new_direct("127.0.0.1:9001".parse::<SocketAddr>().unwrap());
    let msg = PlainTextMessage { text: "hello from localnet integration".into(), app: None, r#type: None, cipher_suite: None };
    let bytes = serde_json::to_vec(&Message::PlainText(msg)).expect("serialize PlainTextMessage");
    Router::with_current_router(router, || {
        handler(&dtls as &dyn Dtls, &from_ep, issuer, &bytes);
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A sender registered on-chain should have their message routed through to the handler.
#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn registered_sender_message_is_routed() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE],
    )
    .expect("fund test accounts");

    let creator =
        test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let (app_id, asset_id) =
        test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    // Register ADDRESS_RECEIVE on-chain so handle_lookup_by_id returns Some.
    test_util::register_client_on_blockchain(
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        "route_test_alice",
        app_id,
        asset_id,
        &creator,
        cfg.clone(),
    );

    let (_api, handler, called, router) = build_api_and_handler(app_id);

    // Issuer is address + ISSUER_SUFFIX — the engine strips the suffix before looking up.
    let issuer = format!("{}{}", test_util::ADDRESS_RECEIVE, ISSUER_SUFFIX);
    fire_plain_text(&handler, router, &issuer);

    assert!(
        called.load(Ordering::SeqCst),
        "message handler must be called when sender is registered on-chain"
    );
}

/// A sender who is NOT registered on-chain should have their message silently dropped.
#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn unregistered_sender_message_is_dropped() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[test_util::ADDRESS_SPEND],
    )
    .expect("fund test accounts");

    let creator =
        test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    // Deploy a fresh app — ADDRESS_RECEIVE has never registered against it.
    let (app_id, _asset_id) =
        test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    let (_api, handler, called, router) = build_api_and_handler(app_id);

    // Use ADDRESS_RECEIVE as issuer even though it has not opted in / registered.
    let issuer = format!("{}{}", test_util::ADDRESS_RECEIVE, ISSUER_SUFFIX);
    fire_plain_text(&handler, router, &issuer);

    assert!(
        !called.load(Ordering::SeqCst),
        "message handler must NOT be called when sender is not registered on-chain"
    );
}
