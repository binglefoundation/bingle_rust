use data_encoding::BASE32_NOPAD;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tracing::warn;
use crate::themes;
use serde_json::{Map as JsonMap, Value as JsonValue};
use uuid::Uuid;

use crate::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use crate::api::pki::generate_pki_from_ops;
use crate::blockchain::algo_ops::AlgoOps;
use crate::dtls::Dtls;
use crate::engine::{BingleAccess, BingleAccessUnsafeForTests, Engine, EngineState, EngineType};
use crate::protocol::ISSUER_SUFFIX;

// Simple bidirectional cache for handle <-> user_id with per-entry timestamps
// Not exposed publicly; guarded by the encompassing Mutex in BingleApiImpl
struct HandleCacheBi {
    handle_to_id: std::collections::HashMap<Handle, (UserId, Instant)>,
    id_to_handle: std::collections::HashMap<UserId, (Handle, Instant)>,
}

impl HandleCacheBi {
    fn new() -> Self {
        Self {
            handle_to_id: std::collections::HashMap::new(),
            id_to_handle: std::collections::HashMap::new(),
        }
    }

    fn insert(&mut self, handle: Handle, user_id: UserId, now: Instant) {
        // Remove any previous reverse mapping for this handle
        if let Some((old_uid, _)) = self.handle_to_id.remove(&handle) {
            if let Some((h2, _)) = self.id_to_handle.get(&old_uid) {
                if *h2 == handle { self.id_to_handle.remove(&old_uid); }
            }
        }
        // If this user_id was mapped to a different handle, remove that handle mapping
        if let Some((old_handle, _)) = self.id_to_handle.remove(&user_id) {
            if let Some((h_uid, _)) = self.handle_to_id.get(&old_handle) {
                if *h_uid == user_id { self.handle_to_id.remove(&old_handle); }
            }
        }
        self.handle_to_id.insert(handle.clone(), (user_id.clone(), now));
        self.id_to_handle.insert(user_id, (handle, now));
    }

    fn get_id_by_handle(&mut self, handle: &Handle, expiry: Duration) -> Option<UserId> {
        if let Some((uid, ts)) = self.handle_to_id.get(handle) {
            if ts.elapsed() < expiry { return Some(uid.clone()); }
            // expired: remove both directions
            let uid = uid.clone();
            self.handle_to_id.remove(handle);
            if let Some((h, _)) = self.id_to_handle.get(&uid) {
                if h == handle { self.id_to_handle.remove(&uid); }
            }
        }
        None
    }

    fn get_handle_by_id(&mut self, user_id: &UserId, expiry: Duration) -> Option<Handle> {
        if let Some((handle, ts)) = self.id_to_handle.get(user_id) {
            if ts.elapsed() < expiry { return Some(handle.clone()); }
            // expired: remove both directions
            let handle = handle.clone();
            self.id_to_handle.remove(user_id);
            if let Some((uid, _)) = self.handle_to_id.get(&handle) {
                if uid == user_id { self.handle_to_id.remove(&handle); }
            }
        }
        None
    }
}

/// Concrete implementation of the BingleApi trait.
///
/// Minimal functionality implemented per task requirements:
/// - start: instantiate a DTLS implementation (DtlsOpenSsl on non-iOS) but do not start the accept loop (no address yet).
/// - send_message_to_network: when given a direct socket address, call DTLS send with the JSON message bytes.
pub struct BingleApiImpl {
    on_message: Option<Arc<OnMessageHandler>>,
    on_connect: Option<Arc<OnConnectHandler>>,
    started_options: StartOptions,
    // Shared on_message handler accessible from Engine/DTLS callback without needing &self
    shared_on_message: Arc<Mutex<Option<Arc<OnMessageHandler>>>>,
    // Optional on_listening handler
    on_listening: Option<Arc<crate::api::bingle_api::OnListeningHandler>>,
    // Engine instance for endpoint identification and DTLS/mux lifecycle (1:1).
    engine: crate::engine::EngineType,

    // Per-API router to avoid global cross-talk
    router: Option<std::sync::Arc<crate::messages::router::Router>>,
    // Weak reference to ourselves for passing to components
    this: crate::api::bingle_api::BingleApiBothType,
    handle_lookup_mock: Mutex<Option<Box<dyn Fn(&Handle) -> Result<Option<UserId>, String> + Send + Sync>>>,
    // Test seam for reverse lookup (id -> handle) without network
    id_to_handle_lookup_mock: Mutex<Option<Box<dyn Fn(&UserId) -> Result<Option<Handle>, String> + Send + Sync>>>,
    handle_cache: Mutex<HandleCacheBi>,
    span: tracing::Span,
}

impl BingleApiImpl {
    pub fn new(options: &StartOptions) -> Arc<Self> {
        tracing::info!("[BingleApiImpl::new][enter]");
        let initial_options = options.clone();
        Arc::<Self>::new_cyclic(|me| {
            let me_both = me.clone();
            let engine = Arc::new(Engine::new(&initial_options, me_both.clone()));
            Self {
                on_message: None,
                on_connect: None,
                started_options: initial_options,
                shared_on_message: Arc::new(Mutex::new(None)),
                on_listening: None,
                engine,
                router: None,
                this: me_both,
                handle_lookup_mock: Mutex::new(None),
                id_to_handle_lookup_mock: Mutex::new(None),
                handle_cache: Mutex::new(HandleCacheBi::new()),
                span: tracing::Span::none(),
            }
        })
    }
}

impl BingleApiImpl {
    /// Apply a closure to the Engine instance (test-only).
    pub fn with_engine_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Engine) -> R,
    {
        self.engine.access_unsafe_for_tests(f)
    }

    /// Test-oriented constructor to inject a custom DTLS implementation.
    pub fn new_with_dtls(dtls: Box<dyn Dtls + Send + Sync>) -> Arc<Self> {
        Self::new_with_dtls_and_options(dtls, StartOptions::default())
    }

    /// Test-oriented constructor to inject custom DTLS and options.
    pub fn new_with_dtls_and_options(dtls: Box<dyn Dtls + Send + Sync>, options: StartOptions) -> Arc<Self> {
        tracing::info!("[BingleApiImpl::new_with_dtls_and_options][enter] dtls_provided=true am_relay={}", options.am_relay);
        Arc::<Self>::new_cyclic(|me| {
            let me_both = me.clone();
            let engine = Arc::new(Engine::new_with_dtls(&options, me_both.clone(), dtls));
            Self {
                on_message: None,
                on_connect: None,
                started_options: options,
                shared_on_message: Arc::new(Mutex::new(None)),
                on_listening: None,
                engine,
                router: None,
                this: me_both,
                handle_lookup_mock: Mutex::new(None),
                id_to_handle_lookup_mock: Mutex::new(None),
                handle_cache: Mutex::new(HandleCacheBi::new()),
                span: tracing::Span::none(),
            }
        })
    }

    /// Test-only helper: override issuer directly for unit/integration tests.
    /// Not part of the stable API surface.
    pub fn set_issuer_for_tests(&self, issuer: String) {
        tracing::info!("[BingleApiImpl::set_issuer_for_tests][enter] issuer_len={}", issuer.len());
        unsafe {
            let engine_ptr = Arc::as_ptr(&self.engine) as *mut Engine;
            (*engine_ptr).set_issuer(issuer);
        }
        tracing::info!("[BingleApiImpl::set_issuer_for_tests][exit]");
    }

    /// Test helpers to access the Engine from integration tests (not part of stable API).
    pub fn engine_state_for_tests(&self) -> Option<EngineState> {
        tracing::trace!("[BingleApiImpl::engine_state_for_tests][enter]");
        let s = Some(self.engine.access(|e| e.state()));
        tracing::trace!("[BingleApiImpl::engine_state_for_tests][exit] state={:?}", s);
        s
    }
    pub fn engine_nat_type_for_tests(&self) -> Option<crate::engine::NatType> {
        tracing::info!("[BingleApiImpl::engine_nat_type_for_tests][enter]");
        let t = Some(self.engine.access(|e| e.nat_type()));
        tracing::info!("[BingleApiImpl::engine_nat_type_for_tests][exit] nat_type={:?}", t);
        t
    }
    pub fn engine_last_public_addr_for_tests(&self) -> Option<SocketAddr> {
        tracing::info!("[BingleApiImpl::engine_last_public_addr_for_tests][enter]");
        let a = self.engine.access(|e| e.last_public_addr());
        tracing::info!("[BingleApiImpl::engine_last_public_addr_for_tests][exit] addr={:?}", a);
        a
    }
    pub fn engine_local_bind_addr_for_tests(&self) -> Option<SocketAddr> {
        tracing::info!("[BingleApiImpl::engine_local_bind_addr_for_tests][enter]");
        let a = self.engine.access(|e| e.local_bind_addr_for_tests());
        tracing::info!("[BingleApiImpl::engine_local_bind_addr_for_tests][exit] addr={:?}", a);
        a
    }
    pub fn engine_mux_for_tests(&self) -> Option<Arc<crate::dtls::UdpNetworkMux>> {
        self.engine.access(|e| e.mux_for_tests())
    }
    pub fn engine_receive_message_for_tests(&self, from_ep: &NetworkEndpoint, data: &[u8]) {
        self.engine.access_unsafe_for_tests(|e: &mut Engine| e.receive_message_for_tests(from_ep, data));
    }
    pub fn engine_ddb_lookup_for_tests(&self, id: &str) -> Result<NetworkEndpoint, String> {
        tracing::info!("[BingleApiImpl::engine_ddb_lookup_for_tests][enter] id={}", id);
        let res = self.engine.access(|e| e.ddb_client().lookup(id));
        tracing::info!("[BingleApiImpl::engine_ddb_lookup_for_tests][exit] res={:?}", res.as_ref().ok());
        res
    }

    pub fn engine_set_ddb_client_for_tests(&self, ddb: Arc<dyn crate::ddb::DdbClient>) {
        tracing::info!("[BingleApiImpl::engine_set_ddb_client_for_tests][enter]");
        unsafe {
            let engine_ptr = Arc::as_ptr(&self.engine) as *mut Engine;
            (*engine_ptr).set_ddb_client_for_tests(ddb);
        }
    }

    pub fn set_handle_lookup_mock_for_tests(&self, mock: Box<dyn Fn(&Handle) -> Result<Option<UserId>, String> + Send + Sync>) {
        let mut m = self.handle_lookup_mock.lock().unwrap();
        *m = Some(mock);
    }

    pub fn set_id_to_handle_lookup_mock_for_tests(&self, mock: Box<dyn Fn(&UserId) -> Result<Option<Handle>, String> + Send + Sync>) {
        let mut m = self.id_to_handle_lookup_mock.lock().unwrap();
        *m = Some(mock);
    }

    /// Test-only accessor to the Engine (for white-box integration tests)
    pub fn engine_for_tests(&self) -> EngineType {
        self.engine.clone()
    }
    pub fn engine_force_stun_consistent_for_tests(&mut self, addr: SocketAddr) {
        tracing::info!("[BingleApiImpl::engine_force_stun_consistent_for_tests][enter] addr={}", addr);
        unsafe {
            let engine_ptr = Arc::as_ptr(&self.engine) as *mut Engine;
            (*engine_ptr).test_force_stun_consistent(addr);
        }
        tracing::info!("[BingleApiImpl::engine_force_stun_consistent_for_tests][exit]");
    }

    /// Test-only accessor to the Engine's TURN handler (for white-box integration tests)
    pub fn engine_turn_client_handler_for_tests(&self) -> std::sync::Arc<crate::turn::turn_client_handler_impl::TurnClientHandlerImpl> {
        self.engine.access(|e| e.turn_client_handler_for_tests())
    }
    pub fn engine_turn_relay_handler_for_tests(&self) -> std::sync::Arc<crate::turn::turn_relay_handler_impl::TurnRelayHandlerImpl> {
        self.engine.access(|e| e.turn_relay_handler_for_tests())
    }

    /// Test-only: set the engine's last public address (for self-send guard tests).
    pub fn engine_set_public_addr_for_tests(&mut self, addr: Option<std::net::SocketAddr>) {
        unsafe {
            let engine_ptr = Arc::as_ptr(&self.engine) as *mut crate::engine::Engine;
            (*engine_ptr).set_last_public_addr(addr);
        }
    }
    
    /// Exposed for integration tests: whether a DTLS instance has been created.
    pub fn has_dtls(&self) -> bool {
        tracing::info!("[BingleApiImpl::has_dtls][enter]");
        // Engine now always has a DTLS instance initialized in new()
        tracing::info!("[BingleApiImpl::has_dtls][exit] return=true");
        true
    }

    fn ensure_dtls(&mut self) {
        // No longer needed as Engine always has a DTLS instance.
    }

    fn send_over_dtls(&self, nsk: &NetworkEndpoint, message: JsonValue) -> bool {
        // Guard: reject incomplete relay endpoints (missing channel); fully-configured
        // relay endpoints (with channel+address) are handled by the TURN layer in DTLS.
        if nsk.is_relay() && nsk.relay_channel().is_none() {
            warn!("[BingleApiImpl::send_over_dtls] rejecting incomplete relay endpoint (no channel): {}", nsk);
            return false;
        }
        // Guard: do not send to ourselves
        if let Some(target_addr) = nsk.inet_socket_address() {
            let my_addr = self.engine.access(|e| e.last_public_addr());
            if my_addr == Some(target_addr) {
                warn!("[BingleApiImpl::send_over_dtls] rejecting send to self: {}", target_addr);
                return false;
            }
        }
        let bytes = serde_json::to_vec(&message).expect("Failed to serialize message to JSON bytes");
        match self.engine.access(|e| e.send_to_peer(nsk, &bytes)) {
            Ok(_) => true,
            Err(err) => {
                warn!("[BingleApiImpl] Engine send_to_peer failed: {}", err);
                false
            }
        }
    }
}


impl Drop for BingleApiImpl {
    fn drop(&mut self) {
        // Ensure background threads and network mux are stopped to avoid use-after-free across tests
        <BingleApiImpl as crate::api::bingle_api::BingleApi>::stop(self);
    }
}

impl BingleApi for BingleApiImpl {
    fn debug_print_options(&self) {
        let span = self.span.clone();
        let _guard = span.enter();
        info_theme!(themes::API, "[BingleApiImpl::debug_print_options] started_options={:?}", self.started_options);
    }
    fn list_all_relays(&self, include_self: bool) -> Vec<crate::relay::relay_finder::RelayInfo> {
        let span = self.span.clone();
        let _guard = span.enter();
        info_theme!(themes::API, "[BingleApiImpl::list_all_relays] include_self={}", include_self);
        // Delegate to Engine's relay_finder-backed implementation
        let res = self.engine.access(|e| e.list_all_relays(include_self));
        info_theme!(themes::API, "[BingleApiImpl::list_all_relays] return={:?}", res);
        res
    }
    fn get_my_id(&self) -> Option<String> {
        let span = self.span.clone();
        let _guard = span.enter();
        // Prefer issuer from Engine (issuer = id + ISSUER_SUFFIX). Trim suffix to return pure id.
        match self.engine.access(|e| e.issuer().map(|iss| iss.to_string())) {
            Ok(iss) => Some(iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string()),
            Err(e) => {
                warn_theme!(themes::API, "[BingleApiImpl::get_my_id] {}", e);
                None
            }
        }
    }
    fn get_user_id(&self) -> Option<String> {
        self.get_my_id()
    }
    fn get_handle(&self) -> Option<String> {
        let h = self.started_options.handle.clone();
        if h.is_empty() { None } else { Some(h) }
    }
    fn get_app_id(&self) -> Option<u64> {
        self.started_options.app_id
    }
    fn get_algo_provider_config(&self) -> Option<crate::blockchain::algo_ops::AlgoChainConfig> {
        self.started_options.algo_provider_config.clone()
    }
    fn handle_lookup_by_id(&self, user_id: &UserId) -> Option<Handle> {
        // Delegate to inherent reverse-lookup with caching/blockchain fallback
        BingleApiImpl::handle_lookup_by_id(self, user_id)
    }
    fn start(&mut self, options: &StartOptions) -> Result<(), String> {
        let span = tracing::info_span!("BingleApi", handle = %options.handle);
        self.span = span.clone();
        let _guard = span.enter();
        unsafe {
            let engine_ptr = Arc::as_ptr(&self.engine) as *mut Engine;
            (*engine_ptr).span = self.span.clone();
        }

        info_theme!(themes::API, "[BingleApiImpl::start][enter] options={:?}", options);
        // Persist options and create a DTLS instance (not starting acceptor yet), then initialize PKI.
        self.started_options = options.clone();
        self.ensure_dtls();

         // Initialize AlgoOps from provided algoPassphrase if available.
        if let Some(pass) = options.algo_passphrase.clone() {
            // Build AlgoOps with passphrase; it will derive and populate the address.
            let ops = AlgoOps::new(Some(pass), None, options.algo_provider_config.clone());
            // Ensure we have an address; if not, force an early error consistent with private_key_bytes failure.
            let addr = match ops.address.clone() {
                Some(a) => a,
                None => {
                    let err = ops.private_key_bytes().err().map(|e| e.to_string()).unwrap_or_else(|| "unknown error".to_string());
                    return Err(format!("Failed to get private key bytes from passphrase: {}", err));
                }
            };
            let issuer = format!("{}{}", addr, ISSUER_SUFFIX);
            unsafe {
                let engine_ptr = Arc::as_ptr(&self.engine) as *mut Engine;
                (*engine_ptr).set_issuer(issuer.clone());
            }

            // Generate certificates: CA = Ed25519 self-signed using Algorand key; server/client = RSA-2048 signed by CA (SHA-512).
            match generate_pki_from_ops(&ops) {
                Ok((ca_pem, server_cert_pem, server_key_pem, client_cert_pem, client_key_pem)) => {
                    unsafe {
                        let engine_ptr = Arc::as_ptr(&self.engine) as *mut Engine;
                        (*engine_ptr).with_dtls_mut(|dtls: &mut (dyn Dtls + Send + Sync)| {
                            dtls.set_ca_cert(Some(ca_pem));
                            dtls.set_server_signing_cert(Some(server_cert_pem));
                            dtls.set_server_signing_private_key(Some(server_key_pem));
                            dtls.set_client_cert(Some(client_cert_pem));
                            dtls.set_client_private_key(Some(client_key_pem));
                            // Install a peer certificate handler in all cases.
                            dtls.set_handle_peer_certificate(Some(crate::protocol::cert_verify::peer_certificate_handler()));
                            // Accept during handshake and validate at the application layer for API flows
                            dtls.set_app_layer_only_verification(false);
                        });
                    }
                }
                Err(e) => {
                    return Err(format!("PKI initialization failed: {}", e));
                }
            }
        }

        // Engine will handle incoming DTLS messages; no API-level DTLS handler required

        // Start Engine using the provided StartOptions and propagate any errors
        // Create a per-API Router instance and bind delegating API handle, sender, and internal controls
        let router_arc: std::sync::Arc<crate::messages::router::Router> = {
            let router = std::sync::Arc::new(crate::messages::router::Router::new(self.this.clone()));
            // Sender closure routes via the delegating API handle
            let this_weak = self.this.clone();
            let sender_cb: Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync + 'static> = Arc::new(move |nsk, uid, msg| {
                tracing::info!("[BingleApiImpl::start][sender_cb] nsk={} uid={} msg={}", nsk, uid, msg);
                let progress_cb = Arc::new(|percent: u8, message: String| {
                    tracing::info!("[BingleApiImpl::start][router sender] Send progress: {}% - {}", percent, message);
                });
                if let Some(api) = this_weak.upgrade() {
                    api.access(|a| a.send_message_to_network(nsk, uid, msg, Some(progress_cb)))
                } else {
                    false
                }
            });
            router.set_sender(Some(sender_cb));
            router
        };
        self.router = Some(router_arc.clone());
        // If an on_message handler was set prior to start(), propagate it to the newly created router now
        if let Some(h) = self.on_message.clone() { router_arc.set_on_message(Some(h)); }

        // Install Engine callback to send via Bingle protocol using the delegating API handle
        let this_weak_for_engine = self.this.clone();
        unsafe {
            let engine_ptr = Arc::as_ptr(&self.engine) as *mut Engine;
            (*engine_ptr).set_send_via_bingle(Some(Arc::new(move |nsk, uid, msg| {
                tracing::info!("[BingleApiImpl::start][engine set send] nsk={} uid={} msg={}", nsk, uid, msg);
                let progress_cb = Arc::new(|percent: u8, message: String| {
                    tracing::info!("[BingleApiImpl::start][engine sender] Send progress: {}% - {}", percent, message);
                });
                if let Some(api) = this_weak_for_engine.upgrade() {
                    api.access(|a| a.send_message_to_network(nsk, uid, msg, Some(progress_cb)))
                } else {
                    false
                }
            })));
            // Provide the BingleApi handle to Engine for handlers and DDB client
            (*engine_ptr).set_bingle_api(self.this.clone());
            // Provide per-API router to the Engine for routing context
            (*engine_ptr).set_router(router_arc.clone());
            (*engine_ptr).start(options)?;
        }

        tracing::info!("[BingleApiImpl::start][exit] Ok(())");
        Ok(())
    }

    fn stop(&mut self) {
        tracing::info!("[BingleApiImpl::stop][enter] {:?}:{:?}", self.engine.issuer(), self.engine.last_public_addr());
        // Notify listeners that we are no longer listening
        if let Some(cb) = &self.on_listening { cb(false, crate::engine::NatType::Unknown); }
        // Stop Engine
        unsafe {
            let engine_ptr = Arc::as_ptr(&self.engine) as *mut Engine;
            (*engine_ptr).stop();
        }
        tracing::info!("[BingleApiImpl::stop][exit] {:?}:{:?}", self.engine.issuer(), self.engine.last_public_addr());
    }

    fn network_change(&mut self) {
        tracing::info!("[BingleApiImpl::network_change][enter]");
        // Placeholder: in a full implementation, we would rescan STUN/static IP and update listeners.
        tracing::info!("[BingleApiImpl::network_change][exit]");
    }

    fn handle_lookup(&self, handle: &Handle) -> Result<Option<UserId>, String> {
        let span = self.span.clone();
        let _guard = span.enter();
        info_theme!(themes::API, "[BingleApiImpl::handle_lookup][enter] handle={}", handle);

        let expiry_duration = self.started_options.handle_cache_expiry.unwrap_or(Duration::from_secs(600)); // Default 10 minutes

        // Check cache
        if let Ok(mut cache) = self.handle_cache.lock() {
            if let Some(user_id) = cache.get_id_by_handle(handle, expiry_duration) {
                tracing::info!("[BingleApiImpl::handle_lookup] cache hit for handle {}: {}", handle, user_id);
                return Ok(Some(user_id));
            }
        }

        {
            let mock = self.handle_lookup_mock.lock().unwrap();
            if let Some(m) = mock.as_ref() {
                let res = m(handle);
                // Update cache on success from mock too
                if let Ok(Some(ref user_id)) = res {
                    if let Ok(mut cache) = self.handle_cache.lock() {
                        cache.insert(handle.clone(), user_id.clone(), Instant::now());
                        tracing::info!("[BingleApiImpl::handle_lookup] cache updated from mock for handle {}: {}", handle, user_id);
                    }
                }
                return res;
            }
        }

        let app_id = self.get_app_id().ok_or("app_id not configured")?;
        let config = self.get_algo_provider_config().ok_or("algo_provider_config not configured")?;

        // Indexer queries are public; use self id or a dummy if not available yet to satisfy AlgoOps::new requirements
        let addr = self.get_my_id().or_else(|| Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5HFKQ".to_string()));
        let ops = AlgoOps::new(self.started_options.algo_passphrase.clone(), addr, Some(config));
        let ab = crate::blockchain::algo_bingle::AlgoBingle::new(ops, app_id, 0);
        let res = ab.handle_lookup(handle).map_err(|e| e.to_string());
        
        // Update cache on success
        if let Ok(Some(ref user_id)) = res {
            if let Ok(mut cache) = self.handle_cache.lock() {
                cache.insert(handle.clone(), user_id.clone(), Instant::now());
                info_theme!(themes::API, "[BingleApiImpl::handle_lookup] cache updated for handle {}: {}", handle, user_id);
            }
        }

        info_theme!(themes::API, "[BingleApiImpl::handle_lookup][exit] return={:?}", res);
        res
    }

    fn send_message_to_id(&self, user_id: &UserId, message: JsonValue, progress: Option<Arc<ProgressCallback>>) -> bool {
        warn_theme!(themes::API, "[BingleApiImpl::send_message_to_id][enter] user_id={} msg={} progress={}", user_id, message, progress.is_some());
        if let Some(cb) = progress.as_ref() { cb(5, "Starting lookup".to_string()); }
        // Engine is ready
        if let Some(cb) = progress.as_ref() { cb(10, "Engine ready".to_string()); }

        // 1) Try to resolve as a known root relay via RelayFinder::lookup_root_id first
        {
            if let Some(cb) = progress.as_ref() { cb(15, "Checking root relays".to_string()); }
            let app_id_opt = self
                .get_app_id()
                .or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
            if let Some(app_id) = app_id_opt {
                let cfg = self.get_algo_provider_config();
                let discover = crate::relay::discovery::indexer_discover_closure(app_id, cfg);
                // Use self.this for RelayFinder
                let finder = crate::relay::relay_finder::RelayFinder::new(self.this.clone(), Duration::from_secs(60), discover);
                if let Some(nsk) = finder.lookup_root_id(user_id) {
                    if let Some(cb) = progress.as_ref() { cb(30, format!("Resolved via root: {}", nsk)); }
                    let ok = self.send_message_to_network(&nsk, user_id, message, progress.clone());
                    info_theme!(themes::API, "[BingleApiImpl::send_message_to_id][exit] return={}", ok);
                    return ok;
                } else {
                    if let Some(cb) = progress.as_ref() { cb(20, "Root not known; falling back to DDB".to_string()); }
                }
            }
        }

        // 2) Fallback to DDB lookup as previously
        let ddb = self.engine.access(|e| e.ddb_client());
        if let Some(cb) = progress.as_ref() { cb(20, "Looking up recipient via DDB".to_string()); }
        match ddb.lookup(user_id) {
            Ok(nsk) => {
                if let Some(cb) = progress.as_ref() { cb(40, format!("DDB lookup ok: {}", nsk)); }
                let ok = self.send_message_to_network(&nsk, user_id, message, progress.clone());
                info_theme!(themes::API, "[BingleApiImpl::send_message_to_id][exit] return={}", ok);
                ok
            }
            Err(err) => {
                warn!("[BingleApiImpl::send_message_to_id] DDB lookup failed: {}", err);
                if let Some(cb) = progress.as_ref() { cb(100, format!("DDB lookup failed: {}", err)); }
                tracing::info!("[BingleApiImpl::send_message_to_id][exit] return=false");
                false
            }
        }
    }

    fn send_message_to_handle(&self, handle: &Handle, message: JsonValue, progress: Option<Arc<ProgressCallback>>) -> bool {
        let span = self.span.clone();
        let _guard = span.enter();
        info_theme!(themes::API, "[BingleApiImpl::send_message_to_handle][enter] handle={} msg={} progress={}", handle, message, progress.is_some());
        let user_id_opt = match self.handle_lookup(handle) {
            Ok(uid) => uid,
            Err(e) => {
                warn!("[BingleApiImpl::send_message_to_handle] handle lookup failed for handle {}: {}", handle, e);
                if let Some(cb) = progress.as_ref() { cb(100, format!("Handle lookup failed: {}", e)); }
                return false;
            }
        };

        if let Some(user_id) = user_id_opt {
            if let Some(cb) = progress.as_ref() { cb(10, format!("Handle {} resolved to {}", handle, user_id)); }
            let ok = self.send_message_to_id(&user_id, message, progress);
            tracing::info!("[BingleApiImpl::send_message_to_handle][exit] return={}", ok);
            ok
        } else {
            warn!("[BingleApiImpl::send_message_to_handle] handle not found: {}", handle);
            if let Some(cb) = progress.as_ref() { cb(100, format!("Handle not found: {}", handle)); }
            tracing::info!("[BingleApiImpl::send_message_to_handle][exit] return=false");
            false
        }
    }

    fn send_message_to_network(
        &self,
        network_source_key: &NetworkEndpoint,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> bool {
        let span = self.span.clone();
        let _guard = span.enter();
        info_theme!(themes::API, "[BingleApiImpl::send_message_to_network][enter] nsk={} user_id={} msg={} progress={}", network_source_key, user_id, message, progress.is_some());
        if let Some(cb) = progress.as_ref() { cb(10, "Preparing send".to_string()); }
        // Validate user_id is an Algorand address (base32 without padding) that decodes to 36 bytes
        let user_id_valid = match BASE32_NOPAD.decode(user_id.as_bytes()) {
            Ok(bytes) if bytes.len() == 36 => true,
            Ok(bytes) => { warn!("[BingleApiImpl::send_message_to_network][ERROR] invalid user_id: base32 decoded length {} (expected 36)", bytes.len()); false },
            Err(e) => { warn!("[BingleApiImpl::send_message_to_network][ERROR] invalid user_id: base32 decode failed: {}", e); false },
        };

        let ok = if user_id_valid {
            // If this is a relay endpoint missing a channel, allocate one via RelayClient::call
            let mut effective_nsk = network_source_key.clone();
            if effective_nsk.relay_id().is_some() && effective_nsk.relay_channel().is_none() {
                // Detect self-relay: if the relay_id is our own id, bypass the relay Call
                // and send directly using the relay address (we ARE the relay for this node).
                let my_id = self.get_my_id();
                let is_self_relay = my_id.as_deref() == effective_nsk.relay_id();

                if is_self_relay {
                    tracing::info!("[BingleApiImpl::send_message_to_network] target's relay is self; bypassing relay Call and sending directly");
                    if let Some(relay_addr) = effective_nsk.relay_address() {
                        effective_nsk = NetworkEndpoint::new_direct(relay_addr);
                    } else if let Some(client_addr) = self.engine.access(|e| e.turn_relay_lookup_addr_by_id(user_id)) {
                        tracing::info!("[BingleApiImpl::send_message_to_network] self-relay: looked up client addr {} for user_id {}", client_addr, user_id);
                        effective_nsk = NetworkEndpoint::new_direct(client_addr);
                    } else {
                        tracing::warn!("[BingleApiImpl::send_message_to_network] self-relay but no relay_address and TurnHandler lookup failed for user_id {}; cannot convert to direct endpoint", user_id);
                        if let Some(cb) = progress.as_ref() { cb(100, "Self-relay with no relay address".to_string()); }
                        return false;
                    }
                } else {
                    tracing::info!("[BingleApiImpl::send_message_to_network] relay endpoint without channel detected; allocating via RelayClient");
                    if let Some(cb) = progress.as_ref() { cb(15, "Allocating relay channel".to_string()); }

                    // Construct RelayClient with API handle
                    let ddb = self.engine.access(|e| e.ddb_client());
                    let relay_client = crate::relay::relay_client::RelayClient::new(self.this.clone(), ddb);
                    match relay_client.call(&effective_nsk, user_id) {
                        Ok(updated) => {
                            effective_nsk = updated;

                            // Register TURN client mapping upon successful CallResponse
                            if let (Some(channel), Some(relay_addr)) = (effective_nsk.relay_channel(), effective_nsk.relay_address()) {
                                let source_addr = effective_nsk.inet_socket_address().unwrap_or(relay_addr);
                                self.engine.access(|e| e.turn_handle_call_response(source_addr, relay_addr, channel, effective_nsk.relay_id().expect("relay_id() should be set by RelayClient::call()")));
                            }

                            if let Some(cb) = progress.as_ref() { cb(30, "Relay channel allocated".to_string()); }
                        }
                        Err(err) => {
                            tracing::warn!("[BingleApiImpl::send_message_to_network] relay Call failed: {}", err);
                            if let Some(cb) = progress.as_ref() { cb(100, format!("Relay allocation failed: {}", err)); }
                            return false;
                        }
                    }
                }
            }
            self.send_over_dtls(&effective_nsk, message)
        } else { false };

        if let Some(cb) = progress.as_ref() { cb(100, if ok { "Sent" } else { "Failed to send" }.to_string()); }
        tracing::info!("[BingleApiImpl::send_message_to_network][exit] return={}", ok);
        ok
    }

    fn send_message_to_id_with_response(&self, user_id: &UserId, message: JsonValue, progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        tracing::info!("[BingleApiImpl::send_message_to_id_with_response][enter] user_id={} msg={} progress={}", user_id, message, progress.is_some());
        // 1) Use the Engine-bound DDB client to resolve the destination NetworkSourceKey
        let cli = self.engine.access(|e| e.ddb_client());
        tracing::debug!("[BingleApiImpl::send_message_to_id_with_response][enter] got ddb_client");
        let nsk = cli.lookup(user_id).map_err(|e| format!("DDB lookup failed: {}", e))?;
        // 2) Delegate to send_message_to_network_with_response for the actual send + wait
        tracing::debug!("[BingleApiImpl::send_message_to_id_with_response] calling with nsk={}", nsk);
        let res = self.send_message_to_network_with_response(&nsk, user_id, message, progress);
        tracing::info!("[BingleApiImpl::send_message_to_id_with_response][exit] result={:?}", res.as_ref().ok());
        res
    }

    fn send_message_to_handle_with_response(&self, handle: &Handle, message: JsonValue, progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        tracing::info!("[BingleApiImpl::send_message_to_handle_with_response][enter] handle={} msg={} progress={}", handle, message, progress.is_some());
        let user_id = self.handle_lookup(handle)?
            .ok_or_else(|| format!("Handle not found: {}", handle))?;
        if let Some(cb) = progress.as_ref() { cb(10, format!("Handle {} resolved to {}", handle, user_id)); }
        let res = self.send_message_to_id_with_response(&user_id, message, progress);
        tracing::info!("[BingleApiImpl::send_message_to_handle_with_response][exit] result={:?}", res.as_ref().ok());
        res
    }

    fn send_message_to_network_with_response(
        &self,
        network_source_key: &NetworkEndpoint,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, String> {
        tracing::info!("[BingleApiImpl::send_message_to_network_with_response][enter] nsk={} user_id={} msg={} progress={}", network_source_key, user_id, message, progress.is_some());
        #[allow(unused)] {  }
        // Create a unique tag and register a pending waiter with the Engine
        let (tag, pending) = self.engine.access(|eng| {
            let tag = Uuid::new_v4();
            eng.register_pending(tag);
            (tag, eng.pending_responses_arc())
        });

        // Ensure message has the responseTag field
        let msg_with_tag = match message {
            JsonValue::Object(mut m) => {
                m.insert("responseTag".to_string(), JsonValue::String(tag.to_string()));
                JsonValue::Object(m)
            }
            other => {
                let mut m = JsonMap::new();
                m.insert("payload".to_string(), other);
                m.insert("responseTag".to_string(), JsonValue::String(tag.to_string()));
                JsonValue::Object(m)
            }
        };

        // Send the request synchronously before waiting to avoid races and ensure handshake starts
        if let Some(cb) = progress.as_ref() { cb(5, "Sending request".to_string()); }
        let sent_ok = self.send_message_to_network(network_source_key, user_id, msg_with_tag, progress.clone());
        if let Some(cb) = progress.as_ref() { cb(20, if sent_ok { "Request sent" } else { "Failed to send request" }.to_string()); }

        // Now wait for a response tagged with our UUID using the Engine's pending map
        let timeout = Duration::from_secs(10);
        if let Some(resp) = Engine::wait_for_response_static(pending, &tag, timeout) {
            if let Some(cb) = progress.as_ref() { cb(100, "Received response".to_string()); }
            tracing::info!("[BingleApiImpl::send_message_to_network_with_response][exit] Ok(response)");
            #[allow(unused)] {  }
            Ok(resp)
        } else {
            if let Some(cb) = progress.as_ref() { cb(100, "Timed out waiting for response".to_string()); }
            let err = if sent_ok { "timeout waiting for response".to_string() } else { "send failed".to_string() };
            tracing::info!("[BingleApiImpl::send_message_to_network_with_response][exit] Err({})", err);
            #[allow(unused)] {  }
            Err(err)
        }
    }

    fn set_on_message(&mut self, handler: Option<Arc<OnMessageHandler>>) {
            tracing::info!("[BingleApiImpl::set_on_message][enter] handler_is_some={}", handler.is_some());
            #[allow(unused)] {  }

            // Store the handler and register it with the per-API router and global fallback
            self.on_message = handler.clone();
            if let Ok(mut g) = self.shared_on_message.lock() { *g = handler.clone(); }
            if let Some(r) = &self.router { r.set_on_message(handler.clone()); }

            tracing::info!("[BingleApiImpl::set_on_message][exit]");
            #[allow(unused)] {  }
        }

    fn set_on_connect(&mut self, handler: Option<Arc<OnConnectHandler>>) { 
            tracing::info!("[BingleApiImpl::set_on_connect][enter] handler_is_some={}", handler.is_some());
            self.on_connect = handler; 
            tracing::info!("[BingleApiImpl::set_on_connect][exit]");
        }

    fn set_on_listening(&mut self, handler: Option<Arc<crate::api::bingle_api::OnListeningHandler>>) {
            tracing::info!("[BingleApiImpl::set_on_listening][enter] handler_is_some={}", handler.is_some());
            // Store locally
            self.on_listening = handler.clone();
            // Propagate to Engine so internal notifications can reach the application
            unsafe {
                let engine_ptr = Arc::as_ptr(&self.engine) as *mut Engine;
                (*engine_ptr).set_on_listening_handler(handler);
            }
            tracing::info!("[BingleApiImpl::set_on_listening][exit]");
        }
}

impl BingleApiImpl {
    /// Public entry from the networking layer for inbound messages.
    /// If message contains a responseTag, it is treated as a response and routed to waiter; otherwise it is dispatched to on_message.
    pub fn handle_incoming_network_message(&self, sender: UserId, sender_handle: Handle, message: JsonValue) {
        tracing::info!("[BingleApiImpl::handle_incoming_network_message][enter] sender={} handle={} msg={}", sender, sender_handle, message);
        #[allow(unused)] {  }
        // Opportunistically update the cache mapping for reverse lookups
        if let Ok(mut cache) = self.handle_cache.lock() {
            cache.insert(sender_handle.clone(), sender.clone(), Instant::now());
        }
        // Engine now fulfills tagged responses; just forward application messages.
        if let Some(cb) = &self.on_message {
            cb(sender, sender_handle, message);
        }
        tracing::info!("[BingleApiImpl::handle_incoming_network_message][exit]");
        #[allow(unused)] {  }
    }
}

impl BingleApiImpl {
    /// Lookup a handle by user id using the local cache. Returns None if not present or expired.
    pub fn handle_lookup_by_id(&self, user_id: &UserId) -> Option<Handle> {
        let expiry_duration = self.started_options.handle_cache_expiry.unwrap_or(Duration::from_secs(600));
        // 1) Check cache first
        if let Ok(mut cache) = self.handle_cache.lock() {
            if let Some(h) = cache.get_handle_by_id(user_id, expiry_duration) {
                return Some(h);
            }
        }

        // 2) Test seam: allow injection for unit/integration tests to avoid network
        if let Ok(m) = self.id_to_handle_lookup_mock.lock() {
            if let Some(cb) = m.as_ref() {
                match cb(user_id) {
                    Ok(Some(handle)) => {
                        if let Ok(mut cache) = self.handle_cache.lock() {
                            cache.insert(handle.clone(), user_id.clone(), Instant::now());
                        }
                        return Some(handle);
                    }
                    Ok(None) => { /* fall through to blockchain (or ultimately None) */ }
                    Err(e) => {
                        tracing::warn!("[BingleApiImpl::handle_lookup_by_id] test seam error: {}", e);
                        // fall through
                    }
                }
            }
        }

        // 3) Fallback: query blockchain local storage via AlgoOps/AlgoBingle to extract 'Handle'
        let app_id = match self.get_app_id() {
            Some(a) => a,
            None => { tracing::warn!("[BingleApiImpl::handle_lookup_by_id] app_id not configured"); return None; }
        };
        let config = match self.get_algo_provider_config() {
            Some(c) => c,
            None => { tracing::warn!("[BingleApiImpl::handle_lookup_by_id] algo_provider_config not configured"); return None; }
        };

        // Build AlgoOps with provided config
        let ops = AlgoOps::new(self.started_options.algo_passphrase.clone(), None, Some(config));
        match ops.local_state_for_account(app_id, user_id) {
            Ok(Some(entries)) => {
                if let Some((_k, h)) = entries.into_iter().find(|(k, _)| k == "Handle") {
                    // Update cache and return
                    if let Ok(mut cache) = self.handle_cache.lock() {
                        cache.insert(h.clone(), user_id.clone(), Instant::now());
                    }
                    return Some(h);
                }
                tracing::info!("[BingleApiImpl::handle_lookup_by_id] no Handle key in local state for {}", user_id);
                None
            }
            Ok(None) => {
                tracing::info!("[BingleApiImpl::handle_lookup_by_id] user not opted in or no local state for {}", user_id);
                None
            }
            Err(e) => {
                tracing::warn!("[BingleApiImpl::handle_lookup_by_id] blockchain query failed: {}", e);
                None
            }
        }
    }
}


impl crate::api::bingle_api::BingleApiInternal for BingleApiImpl {
    fn get_relay_state(&self) -> String { self.engine.access(|e| e.relay_state_str()) }
    fn mutex_handle_request(&self, from_id: String, req: crate::messages::types::MutexRequest) {
        let _ = self.engine.access(|e| e.mutex_handle_request(&from_id, &req));
    }
    fn mutex_handle_response(&self, from_id: String, resp: crate::messages::types::MutexResponse) {
        let _ = self.engine.access(|e| e.mutex_handle_response(&from_id, &resp));
    }
    fn mutex_handle_release(&self, from_id: String, rel: crate::messages::types::MutexRelease) {
        let _ = self.engine.access(|e| e.mutex_handle_release(&from_id, &rel));
    }
    fn set_state(&self, state: EngineState) {
        tracing::info!("[BingleApiImpl::set_state][enter] state={:?}", state);
        let _ = self.engine.access(|e| e.set_state_internal(state));
        tracing::info!("[BingleApiImpl::set_state][exit]");
    }
    fn get_state(&self) -> EngineState {
        self.engine.access(|e| e.state())
    }
    fn set_nat_type(&self, nat: crate::engine::NatType) {
        tracing::info!("[BingleApiImpl::set_nat_type][enter] nat_type={:?}", nat);
        self.engine.access(|e| e.set_nat_type(nat));
        tracing::info!("[BingleApiImpl::set_nat_type][exit]");
    }
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> {
        self.engine.access(|e| e.last_public_addr())
    }
    fn ddb_register_ip(&self, endpoint: std::net::SocketAddr, am_relay: bool) -> Result<(), String> {
        let cli = self.engine.access(|e| e.ddb_client());
        tracing::info!("[BingleApiImpl::ddb_register_ip] registering IP: {:?}, am_relay={}", endpoint, am_relay);
        cli.register_ip(endpoint, am_relay)
    }
    fn ddb_register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), String> {
        let cli = self.engine.access(|e| e.ddb_client());
        tracing::info!("[BingleApiImpl::ddb_register_relay] registering relay: id={}", relay_id);
        cli.register_relay(relay_id, relay_sig)
    }
    fn update_turn_listener_relay(&self, relay_id: String, relay_addr: std::net::SocketAddr) -> Result<(), String> {
        tracing::info!("[BingleApiImpl::update_turn_listener_relay][enter] id={} addr={}", relay_id, relay_addr);
        // Backwards-compatible: delegate to client-side listen response handler
        <BingleApiImpl as crate::api::bingle_api::BingleApiInternal>::turn_client_handle_listen_response(self, relay_addr, relay_id);
        tracing::info!("[BingleApiImpl::update_turn_listener_relay][exit] Ok(())");
        Ok(())
    }
    fn turn_client_handle_listen_response(&self, relay_addr: std::net::SocketAddr, relay_id: String) {
        tracing::info!("[BingleApiImpl::turn_client_handle_listen_response][enter] id={} addr={}", relay_id, relay_addr);
        self.engine.access(|e| e.turn_client_handle_listen_response(relay_addr, &relay_id));
        tracing::info!("[BingleApiImpl::turn_client_handle_listen_response][exit]");
    }
    fn notify_listening(&self, listening: bool, nat_type: crate::engine::NatType) {
        tracing::info!("[BingleApiImpl::notify_listening] listening={} nat_type={:?}", listening, nat_type);
        if let Some(cb) = &self.on_listening { cb(listening, nat_type); }
    }
    fn turn_handle_called(&self, source: std::net::SocketAddr, dest: std::net::SocketAddr, channel: u16) {
        // Forward to the engine's TURN handler client-side interface (non-test API)
        self.engine.access(|e| e.turn_client_handle_called(source, dest, channel));
    }
    fn turn_handle_call_response(&self, source: std::net::SocketAddr, dest: std::net::SocketAddr, channel: u16, relay_id: String) {
        self.engine.access(|e| e.turn_handle_call_response(source, dest, channel, &relay_id));
    }
    fn turn_lookup_addr_by_id(&self, id: String) -> Option<std::net::SocketAddr> {
        self.engine.access(|e| e.turn_relay_lookup_addr_by_id(&id))
    }
    fn turn_handle_call(&self, source_id: String, dest_id: String, source: std::net::SocketAddr, dest: std::net::SocketAddr) -> i32 {
        self.engine.access(|e| e.turn_relay_handle_call(&source_id, &dest_id, source, dest))
    }
    fn turn_handle_listen(&self, id: String, source: std::net::SocketAddr) -> bool {
        self.engine.access(|e| e.turn_relay_handle_listen(&id, &source))
    }
    fn set_relay_state(&self, state: crate::engine::RelayState) {
        tracing::info!("[BingleApiImpl::set_relay_state] state={:?}", state);
        unsafe {
            let engine_ptr = Arc::as_ptr(&self.engine) as *mut Engine;
            (*engine_ptr).set_relay_state(state, "set_relay_state from API internal");
        }
    }
    fn get_peer_ddb_target(&self) -> Option<usize> {
        self.engine.access(|e| e.peer_ddb_records())
    }
    fn ddb_upsert_record(&self, record: crate::ddb::AdvertRecord) {
        self.engine.access(|e| e.ddb_upsert_record(record))
    }
    fn ddb_backend_size(&self) -> usize {
        self.engine.access(|e| e.ddb_backend_size())
    }
    fn initialize_relay(&self) {
        tracing::info!("[BingleApiImpl::initialize_relay]");
        unsafe {
            let engine_ptr = Arc::as_ptr(&self.engine) as *mut Engine;
            (*engine_ptr).initialize_relay_async();
        }
    }
    fn is_relay(&self) -> bool {
        self.started_options.am_relay
    }
    fn signal_signon_complete(&self) {
        tracing::info!("[BingleApiImpl::signal_signon_complete]");
        self.engine.access(|e| e.signal_signon_complete());
    }
    fn reset_signon_complete(&self) {
        tracing::info!("[BingleApiImpl::reset_signon_complete]");
        self.engine.access(|e| e.reset_signon_complete());
    }
    fn ripple_message(&self, message: serde_json::Value, originator_id: String, ddb_backend: &dyn crate::ddb::DdbBackend) {
        tracing::info!("[BingleApiImpl::ripple_message] originator={}", originator_id);
        self.engine.access(|e| e.ripple_message(message, originator_id, ddb_backend));
    }
}



