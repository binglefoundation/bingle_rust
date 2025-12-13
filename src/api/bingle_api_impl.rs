use base64::Engine as _;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Map as JsonMap, Value as JsonValue};
use uuid::Uuid;
use log::{info, warn, error, LevelFilter};
use simple_logger::SimpleLogger;
use std::sync::Once;

use crate::api::bingle_api::{BingleApi, Handle, NetworkSourceKey, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
#[cfg(not(target_os = "ios"))]
use crate::api::pki::generate_pki_from_ops;
use crate::blockchain::algo_ops::AlgoOps;
use crate::dtls::Dtls;
use crate::engine::{Engine, EngineState};
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
    // Engine instance for endpoint identification and DTLS/mux lifecycle (1:1)
    engine: Option<Engine>,
    // Per-API router to avoid global cross-talk
    router: Option<std::sync::Arc<crate::messages::router::Router>>,
}


impl Default for BingleApiImpl {
    fn default() -> Self {
        Self {
            on_message: None,
            on_connect: None,
            started_options: StartOptions::default(),
            shared_on_message: Arc::new(Mutex::new(None)),
            engine: None,
            router: None,
        }
    }
}


#[cfg(not(target_os = "ios"))]

impl BingleApiImpl {
    pub fn new() -> Self {
        log::info!("[BingleApiImpl::new][enter]");
        #[allow(unused)] {  }
        let s = Self::default();
        log::info!("[BingleApiImpl::new][exit]");
        #[allow(unused)] {  }
        s
    }
}

impl BingleApiImpl {
    /// Test-oriented constructor to inject a custom DTLS implementation.
    pub fn new_with_dtls(dtls: Box<dyn Dtls + Send + Sync>) -> Self {
        log::info!("[BingleApiImpl::new_with_dtls][enter] dtls_provided=true");
        #[allow(unused)] {  }
        let mut s = Self::default();
        let mut eng = Engine::new(StartOptions::default());
        eng.set_dtls(dtls);
        s.engine = Some(eng);
        log::info!("[BingleApiImpl::new_with_dtls][exit]");
        #[allow(unused)] {  }
        s
    }

    /// Test-only helper: override issuer directly for unit/integration tests.
    /// Not part of the stable API surface.
    pub fn set_issuer_for_tests(&mut self, issuer: String) {
        log::info!("[BingleApiImpl::set_issuer_for_tests][enter] issuer_len={}", issuer.len());
        #[allow(unused)] {  }
        if let Some(e) = self.engine.as_mut() { e.set_issuer(issuer); }
        log::info!("[BingleApiImpl::set_issuer_for_tests][exit]");
        #[allow(unused)] {  }
    }

    /// Test helpers to access the Engine from integration tests (not part of stable API).
    pub fn engine_state_for_tests(&self) -> Option<EngineState> {
        log::info!("[BingleApiImpl::engine_state_for_tests][enter]");
        #[allow(unused)] {  }
        let s = self.engine.as_ref().map(|e| e.state());
        log::info!("[BingleApiImpl::engine_state_for_tests][exit] state={:?}", s);
        #[allow(unused)] {  }
        s
    }
    pub fn engine_nat_type_for_tests(&self) -> Option<crate::engine::NatType> {
        log::info!("[BingleApiImpl::engine_nat_type_for_tests][enter]");
        let t = self.engine.as_ref().map(|e| e.nat_type());
        log::info!("[BingleApiImpl::engine_nat_type_for_tests][exit] nat_type={:?}", t);
        t
    }
    pub fn engine_last_public_addr_for_tests(&self) -> Option<SocketAddr> {
        log::info!("[BingleApiImpl::engine_last_public_addr_for_tests][enter]");
        #[allow(unused)] {  }
        let a = self.engine.as_ref().and_then(|e| e.last_public_addr());
        log::info!("[BingleApiImpl::engine_last_public_addr_for_tests][exit] addr={:?}", a);
        #[allow(unused)] {  }
        a
    }
    pub fn engine_local_bind_addr_for_tests(&self) -> Option<SocketAddr> {
        log::info!("[BingleApiImpl::engine_local_bind_addr_for_tests][enter]");
        #[allow(unused)] {  }
        let a = self.engine.as_ref().and_then(|e| e.local_bind_addr_for_tests());
        log::info!("[BingleApiImpl::engine_local_bind_addr_for_tests][exit] addr={:?}", a);
        #[allow(unused)] {  }
        a
    }
    pub fn engine_force_stun_consistent_for_tests(&mut self, addr: SocketAddr) {
        log::info!("[BingleApiImpl::engine_force_stun_consistent_for_tests][enter] addr={}", addr);
        #[allow(unused)] {  }
        if let Some(e) = self.engine.as_mut() {
         e.test_force_stun_consistent(addr);
        }
        log::info!("[BingleApiImpl::engine_force_stun_consistent_for_tests][exit]");
        #[allow(unused)] {  }
    }
    
    /// Exposed for integration tests: whether a DTLS instance has been created.
    pub fn has_dtls(&self) -> bool {
        log::info!("[BingleApiImpl::has_dtls][enter]");
        #[allow(unused)] {  }
        let b = self.engine.as_ref().and_then(|e| e.dtls()).is_some();
        log::info!("[BingleApiImpl::has_dtls][exit] return={}", b);
        #[allow(unused)] {  }
        b
    }

    fn ensure_dtls(&mut self) {
        if self.engine.is_none() {
            self.engine = Some(Engine::new(self.started_options.clone()));
        }
        // Only available on non-iOS targets.
        #[cfg(not(target_os = "ios"))]
        {
            let need = self.engine.as_ref().and_then(|e| e.dtls()).is_none();
            if need {
                let dtls = crate::dtls::DtlsOpenSsl::new();
                if let Some(e) = self.engine.as_mut() { e.set_dtls(Box::new(dtls)); }
            }
        }
        #[cfg(target_os = "ios")]
        {
            // Placeholder for iOS where OpenSSL-backed DTLS is not available in this crate.
        }
    }

    fn send_over_dtls(&self, addr: SocketAddr, message: JsonValue) -> bool {
        let bytes = serde_json::to_vec(&message).expect("Failed to serialize message to JSON bytes");
        if let Some(e) = &self.engine {
            match e.send_to_peer(addr, &bytes) {
                Ok(_) => true,
                Err(err) => {
                    warn!("[BingleApiImpl] Engine send_to_peer failed: {}", err);
                    false
                }
            }
        } else {
            warn!("[BingleApiImpl] DTLS/Engine not initialized");
            false
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
            .as_ref()
            .and_then(|e| e.issuer())
            .map(|iss| iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string())
    }
    fn get_app_id(&self) -> Option<u64> {
        self.started_options.app_id
    }
    fn get_algo_provider_config(&self) -> Option<crate::blockchain::algo_ops::AlgoChainConfig> {
        self.started_options.algo_provider_config.clone()
    }
    fn start(&mut self, options: StartOptions) -> Result<(), String> {
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
            if let Some(e) = self.engine.as_mut() { e.set_issuer(issuer.clone()); }

            // Generate certificates: CA = Ed25519 self-signed using Algorand key; server/client = RSA-2048 signed by CA (SHA-512).
            match generate_pki_from_ops(&ops, &issuer) {
                Ok((ca_pem, server_cert_pem, server_key_pem, client_cert_pem, client_key_pem)) => {
                    if let Some(e) = self.engine.as_mut() {
                        e.with_dtls_mut(|dtls| {
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
        if self.engine.is_none() {
            self.engine = Some(Engine::new(self.started_options.clone()));
        }
        // Shared atomic pointer to this API instance for thread-safe callbacks
        let self_ptr_arc = Arc::new(std::sync::atomic::AtomicPtr::new(self as *mut BingleApiImpl));

        // Create a per-API Router instance and bind delegating API, sender, and internal controls
        let router_arc: std::sync::Arc<crate::messages::router::Router> = {
            let ptr = self_ptr_arc.clone();
            let delegator_api: std::sync::Arc<dyn crate::api::bingle_api::BingleApi> = std::sync::Arc::new(DelegatingBingleApi(self_ptr_arc.clone()));
            let router = std::sync::Arc::new(crate::messages::router::Router::new(delegator_api.clone()));
            // Sender closure routes through this API instance
            let sender_cb: Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync> = Arc::new(move |nsk, uid, msg| {
                use std::sync::atomic::Ordering;
                let p = ptr.load(Ordering::SeqCst);
                if p.is_null() { return false; }
                unsafe { (*p).send_message_to_network(nsk, uid, msg, None) }
            });
            router.set_sender(Some(sender_cb));
            // Bind internal control delegator for engine state updates
            struct DelegatingInternal(std::sync::Arc<std::sync::atomic::AtomicPtr<BingleApiImpl>>);
            impl crate::api::bingle_api::BingleApiInternal for DelegatingInternal {
                fn set_state(&self, state: EngineState) {
                    use std::sync::atomic::Ordering;
                    let p = self.0.load(Ordering::SeqCst);
                    if p.is_null() { return; }
                    unsafe { (*p).set_state(state); }
                }
                fn set_nat_type(&self, nat: crate::engine::NatType) {
                    use std::sync::atomic::Ordering;
                    let p = self.0.load(Ordering::SeqCst);
                    if p.is_null() { return; }
                    unsafe { (*p).set_nat_type(nat); }
                }
            }
            let delegator_int = DelegatingInternal(self_ptr_arc.clone());
            router.set_bingle_api_internal(Some(std::sync::Arc::new(delegator_int)));
            router
        };
        self.router = Some(router_arc.clone());
        // If an on_message handler was set prior to start(), propagate it to the newly created router now
        if let Some(h) = self.on_message.clone() { router_arc.set_on_message(Some(h)); }

        // Install Engine callback to send via Bingle protocol capturing this API instance pointer (no globals)
        if let Some(eng) = self.engine.as_mut() {
            // Provide a sending callback so Engine-originated messages go through this API instance
            let ptr = self_ptr_arc.clone();
            eng.set_send_via_bingle(Some(Arc::new(move |nsk, uid, msg| {
                use std::sync::atomic::Ordering;
                let p = ptr.load(Ordering::SeqCst);
                if p.is_null() { return false; }
                unsafe { (*p).send_message_to_network(nsk, uid, msg, None) }
            })));
            // Provide the BingleApi handle to Engine for handlers using the unified delegator
            let delegator: std::sync::Arc<dyn crate::api::bingle_api::BingleApi> = std::sync::Arc::new(DelegatingBingleApi(self_ptr_arc.clone()));
            eng.set_bingle_api_for_handlers(delegator);
            // Provide back-reference so Engine can forward inbound app messages
            eng.set_api_ptr(self_ptr_arc.clone());
            // Provide per-API router to the Engine for routing context
            eng.set_router(router_arc.clone());
            eng.start(options.clone())?;
        }

        log::info!("[BingleApiImpl::start][exit] Ok(())");
        #[allow(unused)] {  }
        Ok(())
    }

    fn stop(&mut self) {
        log::info!("[BingleApiImpl::stop][enter]");
        #[allow(unused)] {  }
        // Stop Engine if running
        if let Some(e) = &mut self.engine {
            e.stop();
        }
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
        #[allow(unused)] {  }
        // Not implemented yet
        let __ret = false;
        log::info!("[BingleApiImpl::send_message_to_id][exit] return={}", __ret);
        #[allow(unused)] {  }
        __ret
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
        network_source_key: &NetworkSourceKey,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> bool {
        log::info!("[BingleApiImpl::send_message_to_network][enter] nsk={} user_id={} msg={} progress={}", network_source_key, user_id, message, progress.is_some());
        #[allow(unused)] {  }
        if let Some(cb) = progress.as_ref() { cb(10, "Preparing send".to_string()); }
        // Only direct socket address path is implemented at this stage.
        if let Some(addr) = network_source_key.inet_socket_address {
            // JUNIE: do not reinstate the below hack
            // Determine if this is a RelayCheck before sending so we can synthesize a response if needed
            // let mut is_check = false;
            // if let serde_json::Value::Object(map) = &msg_clone {
            //     is_check = map.get("type").and_then(|v| v.as_str()) == Some("Check")
            //         && map.get("app").map(|v| v.is_null()).unwrap_or(true);
            // }

            // Validate user_id is base64 and decodes to exactly 36 bytes (Algorand address bytes)
            let user_id_valid = match base64::engine::general_purpose::STANDARD.decode(user_id.as_bytes()) {
                Ok(bytes) if bytes.len() == 36 => true,
                Ok(bytes) => { warn!("[BingleApiImpl::send_message_to_network][ERROR] invalid user_id: base64 decoded length {} (expected 36)", bytes.len()); false },
                Err(e) => { warn!("[BingleApiImpl::send_message_to_network][ERROR] invalid user_id: base64 decode failed: {}", e); false },
            };

            let ok = if user_id_valid { self.send_over_dtls(addr, message) } else { false };

            // JUNIE: do not reinstate the below hack
            // Special-case: if this was a RelayCheck (app == null, type == "Check"), synthesize a local
            // CheckResponse to on_message to make tests deterministic even if send fails or response is dropped.
            // if is_check {
            //     let map = if let serde_json::Value::Object(m) = &msg_clone { m } else { &serde_json::Map::new() };
            //     let mut resp = serde_json::Map::new();
            //     resp.insert("app".to_string(), serde_json::Value::Null);
            //     resp.insert("type".to_string(), serde_json::Value::String("CheckResponse".to_string()));
            //     resp.insert("available".to_string(), serde_json::Value::Bool(true));
            //     if let Some(tag) = map.get("responseTag").and_then(|v| v.as_str()) {
            //         resp.insert("tag".to_string(), serde_json::Value::String(tag.to_string()));
            //     }
            //     if let Ok(g) = self.shared_on_message.lock() {
            //         if let Some(cb) = g.as_ref() {
            //             cb("".to_string(), addr.to_string(), serde_json::Value::Object(resp));
            //         }
            //     }
            // }

            if let Some(cb) = progress.as_ref() { cb(100, if ok { "Sent" } else { "Failed to send" }.to_string()); }

            // JUNIE: do not reinstate the below hack
            // For RelayCheck, treat send as successful even if DTLS send failed (we synthesized response)
            // let __ret = if is_check { true } else { ok };

            log::info!("[BingleApiImpl::send_message_to_network][exit] return={}", ok);
            #[allow(unused)] {  }
            ok
        } else {
            if let Some(cb) = progress.as_ref() { cb(100, "Relay send not yet implemented".to_string()); }
            false
        }
    }

    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        log::info!("[BingleApiImpl::send_message_to_id_with_response][enter] user_id={} msg={} progress={}", _user_id, _message, _progress.is_some());
        #[allow(unused)] {  }
        let err = "not implemented".to_string();
        log::info!("[BingleApiImpl::send_message_to_id_with_response][exit] Err({})", err);
        #[allow(unused)] {  }
        Err(err)
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
        network_source_key: &NetworkSourceKey,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, String> {
        log::info!("[BingleApiImpl::send_message_to_network_with_response][enter] nsk={} user_id={} msg={} progress={}", network_source_key, user_id, message, progress.is_some());
        #[allow(unused)] {  }
        // Create a unique tag and register a pending waiter with the Engine
        let tag = Uuid::new_v4();
        if let Some(e) = &self.engine { e.register_pending(tag); }

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
        if let Some(e) = &self.engine {
            if let Some(resp) = e.wait_for_response(&tag, timeout) {
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
        } else {
            Err("engine not initialized".to_string())
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
        if let Some(e) = &self.engine {
            let _ = e.set_state_internal(state);
        } else {
            warn!("[BingleApiImpl::set_state] engine not initialized");
        }
        log::info!("[BingleApiImpl::set_state][exit]");
        #[allow(unused)] {  }
    }
    fn set_nat_type(&self, nat: crate::engine::NatType) {
        log::info!("[BingleApiImpl::set_nat_type][enter] nat_type={:?}", nat);
        if let Some(e) = &self.engine {
            e.set_nat_type(nat);
        } else {
            warn!("[BingleApiImpl::set_nat_type] engine not initialized");
        }
        log::info!("[BingleApiImpl::set_nat_type][exit]");
    }
}


// Unified delegator: single wrapper that forwards BingleApi calls to the owning BingleApiImpl via AtomicPtr.
struct DelegatingBingleApi(std::sync::Arc<std::sync::atomic::AtomicPtr<BingleApiImpl>>);
impl crate::api::bingle_api::BingleApi for DelegatingBingleApi {
    fn debug_print_options(&self) {
        unsafe {
            if let Some(p) = self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref() {
                p.debug_print_options();
            }
        }
    }
    fn get_my_id(&self) -> Option<String> {
        unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().and_then(|p| p.get_my_id()) }
    }
    fn get_app_id(&self) -> Option<u64> {
        unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().and_then(|p| p.get_app_id()) }
    }
    fn get_algo_provider_config(&self) -> Option<crate::blockchain::algo_ops::AlgoChainConfig> {
        unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().and_then(|p| p.get_algo_provider_config()) }
    }
    fn start(&mut self, _options: crate::api::bingle_api::StartOptions) -> Result<(), String> { Err("not supported".to_string()) }
    fn stop(&mut self) { }
    fn network_change(&mut self) { }
    fn send_message_to_id(&self, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<std::sync::Arc<crate::api::bingle_api::ProgressCallback>>) -> bool {
        unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().map(|p| p.send_message_to_id(user_id, message, progress)).unwrap_or(false) }
    }
    fn send_message_to_handle(&self, handle: &crate::api::bingle_api::Handle, message: serde_json::Value, progress: Option<std::sync::Arc<crate::api::bingle_api::ProgressCallback>>) -> bool {
        unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().map(|p| p.send_message_to_handle(handle, message, progress)).unwrap_or(false) }
    }
    fn send_message_to_network(&self, nsk: &crate::api::bingle_api::NetworkSourceKey, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<std::sync::Arc<crate::api::bingle_api::ProgressCallback>>) -> bool {
        unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().map(|p| p.send_message_to_network(nsk, user_id, message, progress)).unwrap_or(false) }
    }
    fn send_message_to_id_with_response(&self, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<std::sync::Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> {
        unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().ok_or("null".to_string()).and_then(|p| p.send_message_to_id_with_response(user_id, message, progress)) }
    }
    fn send_message_to_handle_with_response(&self, handle: &crate::api::bingle_api::Handle, message: serde_json::Value, progress: Option<std::sync::Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> {
        unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().ok_or("null".to_string()).and_then(|p| p.send_message_to_handle_with_response(handle, message, progress)) }
    }
    fn send_message_to_network_with_response(&self, nsk: &crate::api::bingle_api::NetworkSourceKey, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<std::sync::Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> {
        unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().ok_or("null".to_string()).and_then(|p| p.send_message_to_network_with_response(nsk, user_id, message, progress)) }
    }
    fn set_on_message(&mut self, _handler: Option<std::sync::Arc<crate::api::bingle_api::OnMessageHandler>>) { }
    fn set_on_connect(&mut self, _handler: Option<std::sync::Arc<crate::api::bingle_api::OnConnectHandler>>) { }
}
