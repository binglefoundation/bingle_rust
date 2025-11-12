use base64::Engine as _;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, Condvar};
use std::time::{Duration, Instant};

use serde_json::{Value as JsonValue, Map as JsonMap};
use uuid::Uuid;

use crate::api::bingle_api::{BingleApi, Handle, NetworkSourceKey, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use crate::api::bingle_api::BingleApiInternal;
use crate::dtls::Dtls;
use crate::protocol::ISSUER_SUFFIX;
use crate::blockchain::algo_ops::{AlgoOps, byte_key_to_address};
use crate::engine::{Engine, EngineState};
#[cfg(not(target_os = "ios"))]
use crate::api::pki::generate_pki_from_ops;

/// Concrete implementation of the BingleApi trait.
///
/// Minimal functionality implemented per task requirements:
/// - start: instantiate a DTLS implementation (DtlsOpenSsl on non-iOS) but do not start the accept loop (no address yet).
/// - send_message_to_network: when given a direct socket address, call DTLS send with the JSON message bytes.
pub struct BingleApiImpl {
    on_message: Option<Arc<OnMessageHandler>>,
    on_connect: Option<Arc<OnConnectHandler>>,
    started_options: Option<StartOptions>,
    // Shared on_message handler accessible from Engine/DTLS callback without needing &self
    shared_on_message: Arc<Mutex<Option<Arc<OnMessageHandler>>>>,
    // Engine instance for endpoint identification and DTLS/mux lifecycle (1:1)
    engine: Option<Engine>,
}


impl Default for BingleApiImpl {
    fn default() -> Self {
        Self {
            on_message: None,
            on_connect: None,
            started_options: None,
            shared_on_message: Arc::new(Mutex::new(None)),
            engine: None,
        }
    }
}


#[cfg(not(target_os = "ios"))]

impl BingleApiImpl {
    pub fn new() -> Self {
        println!("[BingleApiImpl::new][enter]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::new][enter]"); }
        let s = Self::default();
        println!("[BingleApiImpl::new][exit]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::new][exit]"); }
        s
    }
}

impl BingleApiImpl {
    /// Test-oriented constructor to inject a custom DTLS implementation.
    pub fn new_with_dtls(dtls: Box<dyn Dtls + Send + Sync>) -> Self {
        println!("[BingleApiImpl::new_with_dtls][enter] dtls_provided=true");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::new_with_dtls][enter] dtls_provided=true"); }
        let mut s = Self::default();
        let mut eng = Engine::new();
        eng.set_dtls(dtls);
        s.engine = Some(eng);
        println!("[BingleApiImpl::new_with_dtls][exit]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::new_with_dtls][exit]"); }
        s
    }

    /// Test-only helper: override issuer directly for unit/integration tests.
    /// Not part of the stable API surface.
    pub fn set_issuer_for_tests(&mut self, issuer: String) {
        println!("[BingleApiImpl::set_issuer_for_tests][enter] issuer_len={}", issuer.len());
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::set_issuer_for_tests][enter] issuer_len={}", issuer.len())); }
        if let Some(e) = self.engine.as_mut() { e.set_issuer(issuer); }
        println!("[BingleApiImpl::set_issuer_for_tests][exit]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::set_issuer_for_tests][exit]"); }
    }

    /// Test helpers to access the Engine from integration tests (not part of stable API).
    pub fn engine_state_for_tests(&self) -> Option<EngineState> {
        println!("[BingleApiImpl::engine_state_for_tests][enter]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::engine_state_for_tests][enter]"); }
        let s = self.engine.as_ref().map(|e| e.state());
        println!("[BingleApiImpl::engine_state_for_tests][exit] state={:?}", s);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::engine_state_for_tests][exit] state={:?}", s)); }
        s
    }
    pub fn engine_last_public_addr_for_tests(&self) -> Option<SocketAddr> {
        println!("[BingleApiImpl::engine_last_public_addr_for_tests][enter]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::engine_last_public_addr_for_tests][enter]"); }
        let a = self.engine.as_ref().and_then(|e| e.last_public_addr());
        println!("[BingleApiImpl::engine_last_public_addr_for_tests][exit] addr={:?}", a);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::engine_last_public_addr_for_tests][exit] addr={:?}", a)); }
        a
    }
    pub fn engine_force_stun_consistent_for_tests(&mut self, addr: SocketAddr) {
        println!("[BingleApiImpl::engine_force_stun_consistent_for_tests][enter] addr={}", addr);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::engine_force_stun_consistent_for_tests][enter] addr={}", addr)); }
        if let Some(e) = self.engine.as_mut() {
         e.test_force_stun_consistent(addr);
        }
        println!("[BingleApiImpl::engine_force_stun_consistent_for_tests][exit]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::engine_force_stun_consistent_for_tests][exit]"); }
    }
    
    /// Exposed for integration tests: whether a DTLS instance has been created.
    pub fn has_dtls(&self) -> bool {
        println!("[BingleApiImpl::has_dtls][enter]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::has_dtls][enter]"); }
        let b = self.engine.as_ref().and_then(|e| e.dtls()).is_some();
        println!("[BingleApiImpl::has_dtls][exit] return={}", b);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::has_dtls][exit] return={}", b)); }
        b
    }

    fn ensure_dtls(&mut self) {
        if self.engine.is_none() {
            self.engine = Some(Engine::new());
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
                    eprintln!("[BingleApiImpl] Engine send_to_peer failed: {}", err);
                    false
                }
            }
        } else {
            eprintln!("[BingleApiImpl] DTLS/Engine not initialized");
            false
        }
    }
}


impl BingleApi for BingleApiImpl {
    fn get_my_id(&self) -> Option<String> {
        // Prefer issuer from Engine (issuer = id + ISSUER_SUFFIX). Trim suffix to return pure id.
        self.engine
            .as_ref()
            .and_then(|e| e.issuer())
            .map(|iss| iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string())
    }
    fn start(&mut self, options: StartOptions) -> Result<(), String> {
        println!("[BingleApiImpl::start][enter] options={:?}", options);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::start][enter] options={:?}", options)); }
        // Persist options and create a DTLS instance (not starting acceptor yet), then initialize PKI.
        self.started_options = Some(options.clone());
        self.ensure_dtls();

         // Initialize AlgoOps from provided algoPassphrase if available.
        if let Some(pass) = options.algo_passphrase.clone() {
            // Build AlgoOps with passphrase and derive our address from it.
            let mut ops = AlgoOps::new(Some(pass), None, options.algo_provider_config.clone());
            // Derive address from the private key bytes and ensure errors propagate (e.g., incorrect passphrase).
            let sk_bytes = ops
                .private_key_bytes()
                .map_err(|e| format!("Failed to get private key bytes from passphrase: {}", e))?;
            if sk_bytes.len() != 32 {
                return Err(format!("Secret key must be 32 bytes, got {}", sk_bytes.len()));
            }
            let arr: [u8; 32] = <[u8; 32]>::try_from(sk_bytes.as_slice())
                .map_err(|_| "Secret key must be 32 bytes".to_string())?;
            let signing = ed25519_dalek::SigningKey::from_bytes(&arr);
            let pk: [u8; 32] = signing.verifying_key().to_bytes();
            let addr = byte_key_to_address(&pk)
                .map_err(|e| format!("Failed to derive Algorand address from key: {}", e))?;
            ops.address = Some(addr.clone());

            // Ensure we have an address; otherwise return an error so callers see the failure.
            let addr = ops.address.clone().ok_or_else(|| "Failed to obtain address from AlgoOps".to_string())?;
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
            self.engine = Some(Engine::new());
        }
        // Shared atomic pointer to this API instance for thread-safe callbacks
        let self_ptr_arc = Arc::new(std::sync::atomic::AtomicPtr::new(self as *mut BingleApiImpl));

        // Expose a sending closure to message handlers via router so they can send replies
        {
            let ptr = self_ptr_arc.clone();
            let sender_cb: Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync> = Arc::new(move |nsk, uid, msg| {
                use std::sync::atomic::Ordering;
                let p = ptr.load(Ordering::SeqCst);
                if p.is_null() { return false; }
                unsafe { (*p).send_message_to_network(nsk, uid, msg, None) }
            });
            crate::messages::router::set_sender(Some(sender_cb));
        }
        // Expose a BingleApi handle to handlers via router so components like RelayFinder can use a real API
        {
            struct DelegatingApi(std::sync::Arc<std::sync::atomic::AtomicPtr<BingleApiImpl>>);
            impl crate::api::bingle_api::BingleApi for DelegatingApi {
                            fn get_my_id(&self) -> Option<String> { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().and_then(|p| p.get_my_id()) } }
                fn start(&mut self, _options: StartOptions) -> Result<(), String> { Err("not supported".to_string()) }
                fn stop(&mut self) { /* not supported */ }
                fn network_change(&mut self) { /* not supported */ }
                fn send_message_to_id(&self, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> bool { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().map(|p| p.send_message_to_id(user_id, message, progress)).unwrap_or(false) } }
                fn send_message_to_handle(&self, handle: &Handle, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> bool { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().map(|p| p.send_message_to_handle(handle, message, progress)).unwrap_or(false) } }
                fn send_message_to_network(&self, nsk: &NetworkSourceKey, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> bool { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().map(|p| p.send_message_to_network(nsk, user_id, message, progress)).unwrap_or(false) } }
                fn send_message_to_id_with_response(&self, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().ok_or("null".to_string()).and_then(|p| p.send_message_to_id_with_response(user_id, message, progress)) } }
                fn send_message_to_handle_with_response(&self, handle: &Handle, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().ok_or("null".to_string()).and_then(|p| p.send_message_to_handle_with_response(handle, message, progress)) } }
                fn send_message_to_network_with_response(&self, nsk: &NetworkSourceKey, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().ok_or("null".to_string()).and_then(|p| p.send_message_to_network_with_response(nsk, user_id, message, progress)) } }
                fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) { /* not supported */ }
                fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) { /* not supported */ }
            }
            let delegator = DelegatingApi(self_ptr_arc.clone());
            crate::messages::router::set_bingle_api(Some(std::sync::Arc::new(delegator)));
        }
        // Expose an internal API handle for engine control (state changes) via router
        {
            struct DelegatingInternal(std::sync::Arc<std::sync::atomic::AtomicPtr<BingleApiImpl>>);
            impl crate::api::bingle_api::BingleApiInternal for DelegatingInternal {
                fn set_state(&self, state: EngineState) {
                    use std::sync::atomic::Ordering;
                    let p = self.0.load(Ordering::SeqCst);
                    if p.is_null() { return; }
                    unsafe { (*p).set_state(state); }
                }
            }
            let delegator_int = DelegatingInternal(self_ptr_arc.clone());
            crate::messages::router::set_bingle_api_internal(Some(std::sync::Arc::new(delegator_int)));
        }

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
            // Provide the BingleApi handle to Engine for handlers
            let delegator = {
                struct DelegatingApi2(std::sync::Arc<std::sync::atomic::AtomicPtr<BingleApiImpl>>);
                impl crate::api::bingle_api::BingleApi for DelegatingApi2 {
                                    fn get_my_id(&self) -> Option<String> { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().and_then(|p| p.get_my_id()) } }
                    fn start(&mut self, _options: StartOptions) -> Result<(), String> { Err("not supported".to_string()) }
                    fn stop(&mut self) { }
                    fn network_change(&mut self) { }
                    fn send_message_to_id(&self, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> bool { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().map(|p| p.send_message_to_id(user_id, message, progress)).unwrap_or(false) } }
                    fn send_message_to_handle(&self, handle: &Handle, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> bool { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().map(|p| p.send_message_to_handle(handle, message, progress)).unwrap_or(false) } }
                    fn send_message_to_network(&self, nsk: &NetworkSourceKey, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> bool { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().map(|p| p.send_message_to_network(nsk, user_id, message, progress)).unwrap_or(false) } }
                    fn send_message_to_id_with_response(&self, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().ok_or("null".to_string()).and_then(|p| p.send_message_to_id_with_response(user_id, message, progress)) } }
                    fn send_message_to_handle_with_response(&self, handle: &Handle, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().ok_or("null".to_string()).and_then(|p| p.send_message_to_handle_with_response(handle, message, progress)) } }
                    fn send_message_to_network_with_response(&self, nsk: &NetworkSourceKey, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { unsafe { self.0.load(std::sync::atomic::Ordering::SeqCst).as_ref().ok_or("null".to_string()).and_then(|p| p.send_message_to_network_with_response(nsk, user_id, message, progress)) } }
                    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) { }
                    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) { }
                }
                std::sync::Arc::new(DelegatingApi2(self_ptr_arc.clone())) as std::sync::Arc<dyn crate::api::bingle_api::BingleApi>
            };
            eng.set_bingle_api_for_handlers(delegator);
            // Provide back-reference so Engine can forward inbound app messages
            eng.set_api_ptr(self_ptr_arc.clone());
            eng.start(options.clone())?;
        }

        println!("[BingleApiImpl::start][exit] Ok(())");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::start][exit] Ok(())"); }
        Ok(())
    }

    fn stop(&mut self) {
        println!("[BingleApiImpl::stop][enter]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::stop][enter]"); }
        // Stop Engine if running
        if let Some(e) = &mut self.engine {
            e.stop();
        }
        println!("[BingleApiImpl::stop][exit]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::stop][exit]"); }
    }

    fn network_change(&mut self) {
        println!("[BingleApiImpl::network_change][enter]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::network_change][enter]"); }
        // Placeholder: in a full implementation, we would rescan STUN/static IP and update listeners.
        println!("[BingleApiImpl::network_change][exit]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::network_change][exit]"); }
    }

    fn send_message_to_id(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool {
        println!("[BingleApiImpl::send_message_to_id][enter] user_id={} msg={} progress={}", _user_id, _message, _progress.is_some());
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_id][enter] user_id={} msg={} progress={}", _user_id, _message, _progress.is_some())); }
        // Not implemented yet
        let __ret = false;
        println!("[BingleApiImpl::send_message_to_id][exit] return={}", __ret);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_id][exit] return={}", __ret)); }
        __ret
    }

    fn send_message_to_handle(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool {
        println!("[BingleApiImpl::send_message_to_handle][enter] handle={} msg={} progress={}", _handle, _message, _progress.is_some());
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_handle][enter] handle={} msg={} progress={}", _handle, _message, _progress.is_some())); }
        // Not implemented yet
        let __ret = false;
        println!("[BingleApiImpl::send_message_to_handle][exit] return={}", __ret);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_handle][exit] return={}", __ret)); }
        __ret
    }

    fn send_message_to_network(
        &self,
        network_source_key: &NetworkSourceKey,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> bool {
        println!("[BingleApiImpl::send_message_to_network][enter] nsk={} user_id={} msg={} progress={}", network_source_key, user_id, message, progress.is_some());
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_network][enter] nsk={} user_id={} msg={} progress={}", network_source_key, user_id, message, progress.is_some())); }
        if let Some(cb) = progress.as_ref() { cb(10, "Preparing send".to_string()); }
        // Only direct socket address path is implemented at this stage.
        if let Some(addr) = network_source_key.inet_socket_address {
            // Keep a copy of message for potential local synthetic response handling in tests
            let msg_clone = message.clone();

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
                Ok(bytes) => { eprintln!("[BingleApiImpl::send_message_to_network][ERROR] invalid user_id: base64 decoded length {} (expected 36)", bytes.len()); false },
                Err(e) => { eprintln!("[BingleApiImpl::send_message_to_network][ERROR] invalid user_id: base64 decode failed: {}", e); false },
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

            println!("[BingleApiImpl::send_message_to_network][exit] return={}", ok);
            #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_network][exit] return={}", ok)); }
            ok
        } else {
            if let Some(cb) = progress.as_ref() { cb(100, "Relay send not yet implemented".to_string()); }
            false
        }
    }

    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        println!("[BingleApiImpl::send_message_to_id_with_response][enter] user_id={} msg={} progress={}", _user_id, _message, _progress.is_some());
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_id_with_response][enter] user_id={} msg={} progress={}", _user_id, _message, _progress.is_some())); }
        let err = "not implemented".to_string();
        println!("[BingleApiImpl::send_message_to_id_with_response][exit] Err({})", err);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_id_with_response][exit] Err({})", err)); }
        Err(err)
    }

    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        println!("[BingleApiImpl::send_message_to_handle_with_response][enter] handle={} msg={} progress={}", _handle, _message, _progress.is_some());
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_handle_with_response][enter] handle={} msg={} progress={}", _handle, _message, _progress.is_some())); }
        let err = "not implemented".to_string();
        println!("[BingleApiImpl::send_message_to_handle_with_response][exit] Err({})", err);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_handle_with_response][exit] Err({})", err)); }
        Err(err)
    }

    fn send_message_to_network_with_response(
        &self,
        network_source_key: &NetworkSourceKey,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, String> {
        println!("[BingleApiImpl::send_message_to_network_with_response][enter] nsk={} user_id={} msg={} progress={}", network_source_key, user_id, message, progress.is_some());
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_network_with_response][enter] nsk={} user_id={} msg={} progress={}", network_source_key, user_id, message, progress.is_some())); }
        // Create a unique tag and register a pending waiter with the Engine
        let tag = Uuid::new_v4();
        if let Some(e) = &self.engine { e.register_pending(tag); }

        // Ensure message has the responseTag field
        let mut msg_with_tag = match message {
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
                println!("[BingleApiImpl::send_message_to_network_with_response][exit] Ok(response)");
                #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::send_message_to_network_with_response][exit] Ok(response)"); }
                Ok(resp)
            } else {
                if let Some(cb) = progress.as_ref() { cb(100, "Timed out waiting for response".to_string()); }
                let err = if sent_ok { "timeout waiting for response".to_string() } else { "send failed".to_string() };
                println!("[BingleApiImpl::send_message_to_network_with_response][exit] Err({})", err);
                #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_network_with_response][exit] Err({})", err)); }
                Err(err)
            }
        } else {
            Err("engine not initialized".to_string())
        }
    }

    fn set_on_message(&mut self, handler: Option<Arc<OnMessageHandler>>) {
            println!("[BingleApiImpl::set_on_message][enter] handler_is_some={}", handler.is_some());
            #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::set_on_message][enter] handler_is_some={}", handler.is_some())); }

            // Revert: store the handler directly without additional debug wrapping
            self.on_message = handler.clone();
            if let Ok(mut g) = self.shared_on_message.lock() { *g = handler; }

            println!("[BingleApiImpl::set_on_message][exit]");
            #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::set_on_message][exit]"); }
        }

    fn set_on_connect(&mut self, handler: Option<Arc<OnConnectHandler>>) { 
            println!("[BingleApiImpl::set_on_connect][enter] handler_is_some={}", handler.is_some());
            #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::set_on_connect][enter] handler_is_some={}", handler.is_some())); }
            self.on_connect = handler; 
            println!("[BingleApiImpl::set_on_connect][exit]");
            #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::set_on_connect][exit]"); }
        }
}

impl BingleApiImpl {
    /// Public entry from the networking layer for inbound messages.
    /// If message contains a responseTag, it is treated as a response and routed to waiter; otherwise it is dispatched to on_message.
    pub fn handle_incoming_network_message(&self, sender: UserId, sender_handle: Handle, message: JsonValue) {
        println!("[BingleApiImpl::handle_incoming_network_message][enter] sender={} handle={} msg={}", sender, sender_handle, message);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::handle_incoming_network_message][enter] sender={} handle={} msg={}", sender, sender_handle, message)); }
        // Engine now fulfills tagged responses; just forward application messages.
        if let Some(cb) = &self.on_message {
            cb(sender, sender_handle, message);
        }
        println!("[BingleApiImpl::handle_incoming_network_message][exit]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::handle_incoming_network_message][exit]"); }
    }
}


impl crate::api::bingle_api::BingleApiInternal for BingleApiImpl {
    fn set_state(&self, state: EngineState) {
        println!("[BingleApiImpl::set_state][enter] state={:?}", state);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::set_state][enter] state={:?}", state)); }
        if let Some(e) = &self.engine {
            let _ = e.set_state_internal(state);
        } else {
            eprintln!("[BingleApiImpl::set_state] engine not initialized");
        }
        println!("[BingleApiImpl::set_state][exit]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::set_state][exit]"); }
    }
}
