use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use data_encoding::BASE32_NOPAD;

use serde_json::{Map as JsonMap, Value as JsonValue};
use uuid::Uuid;
use log::{info, warn, error, LevelFilter};
use simple_logger::SimpleLogger;
use std::sync::Once;

use crate::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
#[cfg(not(target_os = "ios"))]
use crate::api::pki::generate_pki_from_ops;
use crate::blockchain::algo_ops::AlgoOps;
use crate::dtls::Dtls;
use crate::engine::{Engine, EngineState};
use crate::turn::turn_handler::TurnHandler;
use crate::protocol::ISSUER_SUFFIX;

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
    // Engine instance for endpoint identification and DTLS/mux lifecycle (1:1). Boxed to ensure stable address across moves.
    engine: Box<Engine>,
    // Per-API router to avoid global cross-talk
    router: Option<std::sync::Arc<crate::messages::router::Router>>,
}


impl Default for BingleApiImpl {
    fn default() -> Self {
        // Create an unbound Engine; we'll bind API/router during start().
        let engine = Engine::new_unbound(&StartOptions::default());
        Self {
            on_message: None,
            on_connect: None,
            started_options: StartOptions::default(),
            shared_on_message: Arc::new(Mutex::new(None)),
            engine: Box::new(engine),
            router: None,
        }
    }
}


impl BingleApiImpl {
    pub fn new(options: &StartOptions) -> Self {
        log::info!("[BingleApiImpl::new][enter]");
        let mut s = Self::default();
        s.started_options = options.clone();

        // Create a temporary engine with a placeholder API handle that we'll update later
        let eng_ptr_arc = Arc::new(std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()));
        let api_handle: Arc<dyn BingleApi> = Arc::new(crate::engine::EngineBingleApiHandle(eng_ptr_arc.clone()));

        // Create the engine with the placeholder handle
        s.engine = Box::new(Engine::new(&options, api_handle));

        // Update the atomic pointer to point to our engine now that it's created
        eng_ptr_arc.store(&mut *s.engine as *mut Engine, std::sync::atomic::Ordering::SeqCst);

        log::info!("[BingleApiImpl::new][exit]");
        s
    }
}

impl BingleApiImpl {
    /// Test-oriented constructor to inject a custom DTLS implementation.
    pub fn new_with_dtls(dtls: Box<dyn Dtls + Send + Sync>) -> Self {
        log::info!("[BingleApiImpl::new_with_dtls][enter] dtls_provided=true");
        #[allow(unused)] {  }
        let mut s = Self::default();
        s.engine.set_dtls(dtls);
        log::info!("[BingleApiImpl::new_with_dtls][exit]");
        #[allow(unused)] {  }
        s
    }

    /// Test-only helper: override issuer directly for unit/integration tests.
    /// Not part of the stable API surface.
    pub fn set_issuer_for_tests(&mut self, issuer: String) {
        log::info!("[BingleApiImpl::set_issuer_for_tests][enter] issuer_len={}", issuer.len());
        #[allow(unused)] {  }
        self.engine.set_issuer(issuer);
        log::info!("[BingleApiImpl::set_issuer_for_tests][exit]");
        #[allow(unused)] {  }
    }

    /// Test helpers to access the Engine from integration tests (not part of stable API).
    pub fn engine_state_for_tests(&self) -> Option<EngineState> {
        log::trace!("[BingleApiImpl::engine_state_for_tests][enter]");
        #[allow(unused)] {  }
        let s = Some(self.engine.state());
        log::trace!("[BingleApiImpl::engine_state_for_tests][exit] state={:?}", s);
        #[allow(unused)] {  }
        s
    }
    pub fn engine_nat_type_for_tests(&self) -> Option<crate::engine::NatType> {
        log::info!("[BingleApiImpl::engine_nat_type_for_tests][enter]");
        let t = Some(self.engine.nat_type());
        log::info!("[BingleApiImpl::engine_nat_type_for_tests][exit] nat_type={:?}", t);
        t
    }
    pub fn engine_last_public_addr_for_tests(&self) -> Option<SocketAddr> {
        log::info!("[BingleApiImpl::engine_last_public_addr_for_tests][enter]");
        #[allow(unused)] {  }
        let a = self.engine.last_public_addr();
        log::info!("[BingleApiImpl::engine_last_public_addr_for_tests][exit] addr={:?}", a);
        #[allow(unused)] {  }
        a
    }
    pub fn engine_local_bind_addr_for_tests(&self) -> Option<SocketAddr> {
        log::info!("[BingleApiImpl::engine_local_bind_addr_for_tests][enter]");
        #[allow(unused)] {  }
        let a = self.engine.local_bind_addr_for_tests();
        log::info!("[BingleApiImpl::engine_local_bind_addr_for_tests][exit] addr={:?}", a);
        #[allow(unused)] {  }
        a
    }
    pub fn engine_ddb_lookup_for_tests(&self, id: &str) -> Result<NetworkEndpoint, String> {
        log::info!("[BingleApiImpl::engine_ddb_lookup_for_tests][enter] id={}", id);
        let res = self.engine.ddb_client().lookup(id);
        log::info!("[BingleApiImpl::engine_ddb_lookup_for_tests][exit] res={:?}", res.as_ref().ok());
        res
    }
    pub fn engine_force_stun_consistent_for_tests(&mut self, addr: SocketAddr) {
        log::info!("[BingleApiImpl::engine_force_stun_consistent_for_tests][enter] addr={}", addr);
        #[allow(unused)] {  }
        self.engine.test_force_stun_consistent(addr);
        log::info!("[BingleApiImpl::engine_force_stun_consistent_for_tests][exit]");
        #[allow(unused)] {  }
    }

    /// Test-only accessor to the Engine's TURN handler (for white-box integration tests)
    pub fn engine_turn_handler_for_tests(&self) -> std::sync::Arc<crate::turn::turn_handler::TurnHandlerImpl> {
        self.engine.turn_handler_for_tests()
    }
    
    /// Exposed for integration tests: whether a DTLS instance has been created.
    pub fn has_dtls(&self) -> bool {
        log::info!("[BingleApiImpl::has_dtls][enter]");
        #[allow(unused)] {  }
        let b = self.engine.dtls().is_some();
        log::info!("[BingleApiImpl::has_dtls][exit] return={}", b);
        #[allow(unused)] {  }
        b
    }

    fn ensure_dtls(&mut self) {
        // Only available on non-iOS targets.
        #[cfg(not(target_os = "ios"))]
        {
            let need = self.engine.dtls().is_none();
            if need {
                let dtls = crate::dtls::DtlsOpenSsl::new();
                self.engine.set_dtls(Box::new(dtls));
            }
        }
        #[cfg(target_os = "ios")]
        {
            // Placeholder for iOS where OpenSSL-backed DTLS is not available in this crate.
        }
    }

    fn send_over_dtls(&self, nsk: &NetworkEndpoint, message: JsonValue) -> bool {
        let bytes = serde_json::to_vec(&message).expect("Failed to serialize message to JSON bytes");
        match self.engine.send_to_peer(nsk, &bytes) {
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
        log::info!("[BingleApiImpl::debug_print_options] started_options={:?}", self.started_options);
        #[allow(unused)] {  }
    }
    fn get_my_id(&self) -> Option<String> {
        // Prefer issuer from Engine (issuer = id + ISSUER_SUFFIX). Trim suffix to return pure id.
        self.engine
            .issuer()
            .map(|iss| iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string())
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
    fn start(&mut self, options: &StartOptions) -> Result<(), String> {
        // Initialize logging once (stderr + timestamps), respect options.log_level if provided.
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let default_level = if cfg!(debug_assertions) { LevelFilter::Debug } else { LevelFilter::Warn };
            let level = options.log_level.as_deref().map(|s| match s.to_ascii_lowercase().as_str() {
                "trace" => LevelFilter::Trace,
                "debug" => LevelFilter::Debug,
                "info" => LevelFilter::Info,
                "warn" | "warning" => LevelFilter::Warn,
                "error" => LevelFilter::Error,
                _ => default_level,
            }).unwrap_or(default_level);
            let _ = SimpleLogger::new().with_level(level).init();
            // Panic hook that logs at error! and then defers to default behavior
            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |pi| {
                error!("panic: {}", pi);
                default_hook(pi);
            }));
        });
        info!("[BingleApiImpl::start][enter] options={:?}", options);
        #[allow(unused)] {  }
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
            self.engine.set_issuer(issuer.clone());

            // Generate certificates: CA = Ed25519 self-signed using Algorand key; server/client = RSA-2048 signed by CA (SHA-512).
            match generate_pki_from_ops(&ops, &issuer) {
                Ok((ca_pem, server_cert_pem, server_key_pem, client_cert_pem, client_key_pem)) => {
                    self.engine.with_dtls_mut(|dtls| {
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
                Err(e) => {
                    return Err(format!("PKI initialization failed: {}", e));
                }
            }
        }

        // Engine will handle incoming DTLS messages; no API-level DTLS handler required

        // Start Engine using the provided StartOptions and propagate any errors
        // Build a BingleApi handle backed by this Engine so handlers can call back without a delegator.
        let eng_ptr_arc = Arc::new(std::sync::atomic::AtomicPtr::new(&mut *self.engine as *mut Engine));

        // Create a per-API Router instance and bind engine-backed API handle, sender, and internal controls
        let router_arc: std::sync::Arc<crate::messages::router::Router> = {
            let api_handle: std::sync::Arc<dyn crate::api::bingle_api::BingleApi> = std::sync::Arc::new(crate::engine::EngineBingleApiHandle(eng_ptr_arc.clone()));
            let router = std::sync::Arc::new(crate::messages::router::Router::new(api_handle.clone()));
            // Sender closure routes via the engine-backed API handle
            let api_for_sender = api_handle.clone();
            let sender_cb: Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync> = Arc::new(move |nsk, uid, msg| {
                log::info!("[BingleApiImpl::start][sender_cb] nsk={} uid={} msg={}", nsk, uid, msg);
                let progress_cb = Arc::new(|percent: u8, message: String| {
                    log::info!("[BingleApiImpl::start][router sender] Send progress: {}% - {}", percent, message);
                });
                api_for_sender.send_message_to_network(nsk, uid, msg, Some(progress_cb))
            });
            router.set_sender(Some(sender_cb));
            // Bind internal control adapter for engine state updates
            let internal = crate::engine::EngineInternalPtr(eng_ptr_arc.clone());
            router.set_bingle_api_internal(Some(std::sync::Arc::new(internal)));
            router
        };
        self.router = Some(router_arc.clone());
        // If an on_message handler was set prior to start(), propagate it to the newly created router now
        if let Some(h) = self.on_message.clone() { router_arc.set_on_message(Some(h)); }

        // Install Engine callback to send via Bingle protocol using the engine-backed API handle
        let api_for_engine_send: std::sync::Arc<dyn crate::api::bingle_api::BingleApi> = std::sync::Arc::new(crate::engine::EngineBingleApiHandle(eng_ptr_arc.clone()));
        self.engine.set_send_via_bingle(Some(Arc::new(move |nsk, uid, msg| {
            log::info!("[BingleApiImpl::start][engine set send] nsk={} uid={} msg={}", nsk, uid, msg);
            let progress_cb = Arc::new(|percent: u8, message: String| {
                log::info!("[BingleApiImpl::start][engine sender] Send progress: {}% - {}", percent, message);
            });
            api_for_engine_send.send_message_to_network(nsk, uid, msg, Some(progress_cb))
        })));
        // Provide the BingleApi handle to Engine for handlers and DDB client
        let api_for_engine: std::sync::Arc<dyn crate::api::bingle_api::BingleApi> = std::sync::Arc::new(crate::engine::EngineBingleApiHandle(eng_ptr_arc.clone()));
        self.engine.set_bingle_api(api_for_engine);
        // Provide per-API router to the Engine for routing context
        self.engine.set_router(router_arc.clone());
        self.engine.start(options)?;

        log::info!("[BingleApiImpl::start][exit] Ok(())");
        #[allow(unused)] {  }
        Ok(())
    }

    fn stop(&mut self) {
        log::info!("[BingleApiImpl::stop][enter]");
        #[allow(unused)] {  }
        // Stop Engine
        self.engine.stop();
        log::info!("[BingleApiImpl::stop][exit]");
        #[allow(unused)] {  }
    }

    fn network_change(&mut self) {
        log::info!("[BingleApiImpl::network_change][enter]");
        #[allow(unused)] {  }
        // Placeholder: in a full implementation, we would rescan STUN/static IP and update listeners.
        log::info!("[BingleApiImpl::network_change][exit]");
        #[allow(unused)] {  }
    }

    fn send_message_to_id(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool {
        log::info!("[BingleApiImpl::send_message_to_id][enter] user_id={} msg={} progress={}", _user_id, _message, _progress.is_some());
        if let Some(cb) = _progress.as_ref() { cb(5, "Starting DDB lookup".to_string()); }
        // Validate DDB client is available
        if let Some(cb) = _progress.as_ref() { cb(10, "Engine ready".to_string()); }
        let ddb = self.engine.ddb_client();
        if let Some(cb) = _progress.as_ref() { cb(20, "Looking up recipient".to_string()); }
        match ddb.lookup(_user_id) {
            Ok(nsk) => {
                if let Some(cb) = _progress.as_ref() { cb(40, format!("DDB lookup ok: {}", nsk)); }
                let ok = self.send_message_to_network(&nsk, _user_id, _message, _progress.clone());
                log::info!("[BingleApiImpl::send_message_to_id][exit] return={}", ok);
                ok
            }
            Err(err) => {
                warn!("[BingleApiImpl::send_message_to_id] DDB lookup failed: {}", err);
                if let Some(cb) = _progress.as_ref() { cb(100, format!("DDB lookup failed: {}", err)); }
                log::info!("[BingleApiImpl::send_message_to_id][exit] return=false");
                false
            }
        }
    }

    fn send_message_to_handle(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool {
        log::info!("[BingleApiImpl::send_message_to_handle][enter] handle={} msg={} progress={}", _handle, _message, _progress.is_some());
        #[allow(unused)] {  }
        // Not implemented yet
        let __ret = false;
        log::info!("[BingleApiImpl::send_message_to_handle][exit] return={}", __ret);
        #[allow(unused)] {  }
        __ret
    }

    fn send_message_to_network(
        &self,
        network_source_key: &NetworkEndpoint,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> bool {
        log::info!("[BingleApiImpl::send_message_to_network][enter] nsk={} user_id={} msg={} progress={}", network_source_key, user_id, message, progress.is_some());
        #[allow(unused)] {  }
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
                log::info!("[BingleApiImpl::send_message_to_network] relay endpoint without channel detected; allocating via RelayClient");
                if let Some(cb) = progress.as_ref() { cb(15, "Allocating relay channel".to_string()); }

                // Build a temporary Engine-backed BingleApi handle and construct RelayClient with Engine DDB client
                let eng_ptr_arc = std::sync::Arc::new(std::sync::atomic::AtomicPtr::new((&*self.engine) as *const crate::engine::Engine as *mut crate::engine::Engine));
                let api_handle: std::sync::Arc<dyn crate::api::bingle_api::BingleApi> = std::sync::Arc::new(crate::engine::EngineBingleApiHandle(eng_ptr_arc.clone()));
                let ddb = self.engine.ddb_client();
                let relay_client = crate::relay::relay_client::RelayClient::new(api_handle, ddb);
                match relay_client.call(&effective_nsk, user_id) {
                    Ok(updated) => {
                        effective_nsk = updated;
                        if let Some(cb) = progress.as_ref() { cb(30, "Relay channel allocated".to_string()); }
                    }
                    Err(err) => {
                        log::warn!("[BingleApiImpl::send_message_to_network] relay call failed: {}", err);
                        if let Some(cb) = progress.as_ref() { cb(100, format!("Relay allocation failed: {}", err)); }
                        return false;
                    }
                }
            }
            self.send_over_dtls(&effective_nsk, message)
        } else { false };

        if let Some(cb) = progress.as_ref() { cb(100, if ok { "Sent" } else { "Failed to send" }.to_string()); }
        log::info!("[BingleApiImpl::send_message_to_network][exit] return={}", ok);
        ok
    }

    fn send_message_to_id_with_response(&self, user_id: &UserId, message: JsonValue, progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        log::info!("[BingleApiImpl::send_message_to_id_with_response][enter] user_id={} msg={} progress={}", user_id, message, progress.is_some());
        // 1) Use the Engine-bound DDB client to resolve the destination NetworkSourceKey
        let cli = self.engine.ddb_client();
        let nsk = cli.lookup(user_id)?;
        // 2) Delegate to send_message_to_network_with_response for the actual send + wait
        let res = self.send_message_to_network_with_response(&nsk, user_id, message, progress);
        log::info!("[BingleApiImpl::send_message_to_id_with_response][exit] result={:?}", res.as_ref().ok());
        res
    }

    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        log::info!("[BingleApiImpl::send_message_to_handle_with_response][enter] handle={} msg={} progress={}", _handle, _message, _progress.is_some());
        #[allow(unused)] {  }
        let err = "not implemented".to_string();
        log::info!("[BingleApiImpl::send_message_to_handle_with_response][exit] Err({})", err);
        #[allow(unused)] {  }
        Err(err)
    }

    fn send_message_to_network_with_response(
        &self,
        network_source_key: &NetworkEndpoint,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, String> {
        log::info!("[BingleApiImpl::send_message_to_network_with_response][enter] nsk={} user_id={} msg={} progress={}", network_source_key, user_id, message, progress.is_some());
        #[allow(unused)] {  }
        // Create a unique tag and register a pending waiter with the Engine
        let tag = Uuid::new_v4();
        self.engine.register_pending(tag);

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
        if let Some(resp) = self.engine.wait_for_response(&tag, timeout) {
            if let Some(cb) = progress.as_ref() { cb(100, "Received response".to_string()); }
            log::info!("[BingleApiImpl::send_message_to_network_with_response][exit] Ok(response)");
            #[allow(unused)] {  }
            Ok(resp)
        } else {
            if let Some(cb) = progress.as_ref() { cb(100, "Timed out waiting for response".to_string()); }
            let err = if sent_ok { "timeout waiting for response".to_string() } else { "send failed".to_string() };
            log::info!("[BingleApiImpl::send_message_to_network_with_response][exit] Err({})", err);
            #[allow(unused)] {  }
            Err(err)
        }
    }

    fn set_on_message(&mut self, handler: Option<Arc<OnMessageHandler>>) {
            log::info!("[BingleApiImpl::set_on_message][enter] handler_is_some={}", handler.is_some());
            #[allow(unused)] {  }

            // Store the handler and register it with the per-API router and global fallback
            self.on_message = handler.clone();
            if let Ok(mut g) = self.shared_on_message.lock() { *g = handler.clone(); }
            if let Some(r) = &self.router { r.set_on_message(handler.clone()); }

            log::info!("[BingleApiImpl::set_on_message][exit]");
            #[allow(unused)] {  }
        }

    fn set_on_connect(&mut self, handler: Option<Arc<OnConnectHandler>>) { 
            log::info!("[BingleApiImpl::set_on_connect][enter] handler_is_some={}", handler.is_some());
            #[allow(unused)] {  }
            self.on_connect = handler; 
            log::info!("[BingleApiImpl::set_on_connect][exit]");
            #[allow(unused)] {  }
        }
}

impl BingleApiImpl {
    /// Public entry from the networking layer for inbound messages.
    /// If message contains a responseTag, it is treated as a response and routed to waiter; otherwise it is dispatched to on_message.
    pub fn handle_incoming_network_message(&self, sender: UserId, sender_handle: Handle, message: JsonValue) {
        log::info!("[BingleApiImpl::handle_incoming_network_message][enter] sender={} handle={} msg={}", sender, sender_handle, message);
        #[allow(unused)] {  }
        // Engine now fulfills tagged responses; just forward application messages.
        if let Some(cb) = &self.on_message {
            cb(sender, sender_handle, message);
        }
        log::info!("[BingleApiImpl::handle_incoming_network_message][exit]");
        #[allow(unused)] {  }
    }
}


impl crate::api::bingle_api::BingleApiInternal for BingleApiImpl {
    fn set_state(&self, state: EngineState) {
        log::info!("[BingleApiImpl::set_state][enter] state={:?}", state);
        #[allow(unused)] {  }
        let _ = self.engine.set_state_internal(state);
        log::info!("[BingleApiImpl::set_state][exit]");
        #[allow(unused)] {  }
    }
    fn get_state(&self) -> EngineState {
        self.engine.state()
    }
    fn set_nat_type(&self, nat: crate::engine::NatType) {
        log::info!("[BingleApiImpl::set_nat_type][enter] nat_type={:?}", nat);
        self.engine.set_nat_type(nat);
        log::info!("[BingleApiImpl::set_nat_type][exit]");
    }
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> {
        self.engine.last_public_addr()
    }
    fn ddb_register_ip(&self, endpoint: std::net::SocketAddr) -> Result<(), String> {
        let cli = self.engine.ddb_client();
        log::info!("[BingleApiImpl::ddb_register_ip] registering IP: {:?}", endpoint);
        cli.register_ip(endpoint)
    }
    fn ddb_register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), String> {
        let cli = self.engine.ddb_client();
        log::info!("[BingleApiImpl::ddb_register_relay] registering relay: id={}", relay_id);
        cli.register_relay(relay_id, relay_sig)
    }
    fn update_turn_listener_relay(&self, relay_id: String, relay_addr: std::net::SocketAddr) -> Result<(), String> {
        log::info!("[BingleApiImpl::update_turn_listener_relay][enter] id={} addr={}", relay_id, relay_addr);
        // Access the Engine's TURN handler and register the mapping id -> addr
        let th = self.engine.turn_handler_for_tests();
        let ok = th.handle_listen(&relay_id, &relay_addr);
        if ok {
            log::info!("[BingleApiImpl::update_turn_listener_relay][exit] Ok(())");
            Ok(())
        } else {
            let err = format!("failed to update TURN listener mapping for {} -> {}", relay_id, relay_addr);
            log::warn!("[BingleApiImpl::update_turn_listener_relay][exit] Err({})", err);
            Err(err)
        }
    }
}


