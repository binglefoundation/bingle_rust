// Localnet integration test: when no STUN server responds, the engine must set
// NatType::NoConnection and call on_listening with false.
//
// The engine's STUN path (start() without a static_ip) polls each server every
// search_interval_ms milliseconds and marks the server as "blocked" after 3
// consecutive non-responses.  The STUN finder then sets its own state to Blocked,
// which the engine wires up to on_stun_blocked() → NatType::NoConnection +
// on_listening(false).
//
// Here we point the engine at loopback ports where nothing is bound, so all
// binding requests are silently dropped by the OS (ICMP unreachable may arrive,
// but is not a valid STUN Binding Response).  After 3 search intervals (~6 s)
// the Blocked callback fires.  We wait up to 15 s to accommodate CI variance.

use std::net::SocketAddr;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use crate::engine::ddb_upsert::test_util::init_test_logging;
use bingle_core::api::bingle_api::{
    BingleApi, Handle, NetworkEndpoint, OnListeningHandler, StartOptions, UserId,
};
use bingle_core::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use bingle_core::engine::{Engine, NatType};
use bingle_core::messages::router::Router;

// ---------------------------------------------------------------------------
// Minimal DTLS stub — starts/stops successfully, never sends or receives.
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct NullDtls;

impl Dtls for NullDtls {
    fn start(&mut self, _mux: Arc<bingle_core::dtls::UdpNetworkMux>) -> DtlsResult<()> {
        Ok(())
    }
    fn stop(&mut self) -> DtlsResult<()> {
        Ok(())
    }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> DtlsResult<()> {
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> {
        None
    }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
    fn set_handle_new_session(
        &mut self,
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
// Minimal BingleApi stub — required by Router construction.
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct NullApi;

impl BingleApi for NullApi {
    fn list_all_relays(
        &self,
        _include_self: bool,
    ) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        Vec::new()
    }
    fn set_on_listening(&mut self, _handler: Option<Arc<OnListeningHandler>>) {}
    fn get_user_id(&self) -> Option<String> {
        None
    }
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> {
        None
    }
    fn get_handle(&self) -> Option<String> {
        None
    }
    fn get_app_id(&self) -> Option<u64> {
        None
    }
    fn get_algo_provider_config(
        &self,
    ) -> Option<bingle_core::blockchain::algo_ops::AlgoChainConfig> {
        None
    }
    fn start(
        &mut self,
        _options: &StartOptions,
    ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
        Ok(())
    }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn handle_lookup(
        &self,
        _handle: &Handle,
    ) -> Result<Option<UserId>, bingle_core::api::bingle_api::BingleError> {
        Ok(None)
    }
    fn handle_lookup_partial(
        &self,
        _handle: &Handle,
    ) -> Result<Option<(UserId, Handle)>, bingle_core::api::bingle_api::BingleError> {
        Ok(None)
    }
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> {
        None
    }
    fn send_message_to_id(
        &self,
        _user_id: &UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(false)
    }
    fn send_message_to_handle(
        &self,
        _handle: &Handle,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(false)
    }
    fn send_message_to_network(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(false)
    }
    fn send_message_to_id_with_response(
        &self,
        _user_id: &UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "ni".into(),
        ))
    }
    fn send_message_to_handle_with_response(
        &self,
        _handle: &Handle,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "ni".into(),
        ))
    }
    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "ni".into(),
        ))
    }
    fn set_on_message(
        &mut self,
        _handler: Option<Arc<bingle_core::api::bingle_api::OnMessageHandler>>,
    ) {
    }
    fn set_on_connect(
        &mut self,
        _handler: Option<Arc<bingle_core::api::bingle_api::OnConnectHandler>>,
    ) {
    }
}

impl bingle_core::api::bingle_api::BingleApiInternal for NullApi {
    fn get_relay_state(&self) -> String {
        "off".to_string()
    }
}

// ---------------------------------------------------------------------------
// Helper: bind a UDP socket on 127.0.0.1:0 and immediately close it, returning
// the ephemeral port that the OS allocated.  Nothing ever listens on that port
// again, so STUN binding requests sent there receive no reply.
// ---------------------------------------------------------------------------
fn allocate_silent_stun_addr() -> SocketAddr {
    let sock = std::net::UdpSocket::bind("127.0.0.1:0")
        .expect("failed to bind ephemeral UDP socket for silent STUN addr");
    let addr = sock.local_addr().expect("failed to get local addr");
    // sock drops here, releasing the port — the address is now unbound
    addr
}

// ---------------------------------------------------------------------------
// The actual integration test.
//
// Starts the engine (STUN path, no static_ip) with two STUN server addresses
// that are not bound to any process.  The STUN background thread fires three
// search intervals (each 2 s) without receiving a response, then raises the
// Blocked state.  The engine's state-change callback calls on_stun_blocked()
// which sets NatType::NoConnection and calls on_listening(false).
//
// We wait up to 15 s to accommodate CI variance.
// ---------------------------------------------------------------------------
#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_stun_responses_sets_no_connection_and_calls_on_listening_false() {
    init_test_logging();

    // Two loopback addresses where nothing listens — STUN packets will be silently dropped.
    let stun1 = allocate_silent_stun_addr();
    let stun2 = allocate_silent_stun_addr();

    let opts = StartOptions {
        handle: "test_no_stun".to_string(),
        algo_passphrase: None,
        static_ip: None,
        am_relay: false,
        stun_servers: Some(vec![stun1, stun2]),
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    let null_api = NullApi;
    let eng = Arc::new(Engine::new(
        &opts,
        crate::util::mock_bingle_api::to_weak(null_api.clone()),
    ));
    unsafe {
        let eng_ptr = Arc::as_ptr(&eng) as *mut Engine;
        (*eng_ptr).set_weak_self(Arc::downgrade(&eng));
        (*eng_ptr).set_dtls(Box::new(NullDtls));
    }
    let router = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(null_api)));
    unsafe {
        let eng_ptr = Arc::as_ptr(&eng) as *mut Engine;
        (*eng_ptr).set_router(router);
    }

    // Track on_listening calls
    let called_false = Arc::new(AtomicBool::new(false));
    let called_true = Arc::new(AtomicBool::new(false));

    let flag_false = called_false.clone();
    let flag_true = called_true.clone();
    unsafe {
        let eng_ptr = Arc::as_ptr(&eng) as *mut Engine;
        (*eng_ptr).set_on_listening_handler(Some(Arc::new(move |listening, _nat: NatType| {
            if listening {
                flag_true.store(true, Ordering::SeqCst);
            } else {
                flag_false.store(true, Ordering::SeqCst);
            }
        })));

        (*eng_ptr)
            .start(&opts)
            .expect("engine.start should succeed");
    }

    // Wait for the STUN finder to raise Blocked (3 × 2 s search intervals ≈ 6 s minimum).
    let timeout = Duration::from_secs(30);
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline && !called_false.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(
        called_false.load(Ordering::SeqCst),
        "on_listening should have been called with false after no STUN responses (timeout {}s)",
        timeout.as_secs()
    );
    assert!(
        !called_true.load(Ordering::SeqCst),
        "on_listening should not have been called with true when STUN is blocked"
    );
    assert_eq!(
        eng.nat_type(),
        NatType::NoConnection,
        "nat_type should be NoConnection when no STUN servers respond"
    );

    unsafe {
        let eng_ptr = Arc::as_ptr(&eng) as *mut Engine;
        (*eng_ptr).stop();
    }
}
