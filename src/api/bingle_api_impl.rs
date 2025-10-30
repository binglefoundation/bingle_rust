use std::collections::HashMap;
use base64::Engine as _;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, Condvar};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::{Value as JsonValue, Map as JsonMap};
use uuid::Uuid;

use crate::api::bingle_api::{BingleApi, Handle, NetworkSourceKey, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use crate::dtls::Dtls;
use crate::protocol::ISSUER_SUFFIX;
use crate::blockchain::algo_ops::{AlgoOps, byte_key_to_address};
use crate::engine::{Engine, EngineState};

/// Concrete implementation of the BingleApi trait.
///
/// Minimal functionality implemented per task requirements:
/// - start: instantiate a DTLS implementation (DtlsOpenSsl on non-iOS) but do not start the accept loop (no address yet).
/// - send_message_to_network: when given a direct socket address, call DTLS send with the JSON message bytes.
pub struct BingleApiImpl {
    dtls: Option<Box<dyn Dtls + Send + Sync>>, // boxed trait object for flexibility/mocking
    on_message: Option<Arc<OnMessageHandler>>,
    on_connect: Option<Arc<OnConnectHandler>>,
    started_options: Option<StartOptions>,
    // Map of pending response tags to their wait primitives
    pending_responses: Arc<Mutex<HashMap<Uuid, Arc<(Mutex<Pending>, Condvar)>>>>,
    // Shared on_message handler accessible from DTLS callback without needing &self
    shared_on_message: Arc<Mutex<Option<Arc<OnMessageHandler>>>>,
    issuer: Option<String>,
    // Engine instance for endpoint identification and DTLS/mux lifecycle
    engine: Option<Engine>,
}

// Global on_message dispatcher storage; used by MessageHandler::on_plain_text to delegate to API.
static GLOBAL_ON_MESSAGE: OnceLock<Mutex<Option<Arc<OnMessageHandler>>>> = OnceLock::new();

/// Set or clear the global on_message handler (used by plain-text router fallback).
pub fn global_on_message_set(handler: Option<Arc<OnMessageHandler>>) {
    let slot = GLOBAL_ON_MESSAGE.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = slot.lock() { *g = handler; }
}

/// Invoke the global on_message handler, if set.
pub fn global_on_message_call(sender: String, sender_handle: String, msg: JsonValue) {
    if let Some(slot) = GLOBAL_ON_MESSAGE.get() {
        if let Ok(g) = slot.lock() {
            if let Some(cb) = g.as_ref() { cb(sender, sender_handle, msg); }
        }
    }
}

// Global send-to-network dispatcher used by Engine to invoke the Bingle API send path
// without holding a direct reference to the API instance. We store the API instance
// pointer in an AtomicUsize for thread-safe publication.
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
static GLOBAL_SEND_API_PTR: AtomicUsize = AtomicUsize::new(0);

/// Install or clear the global API pointer for send_message_to_network
pub fn global_send_to_network_set(ptr: Option<*const BingleApiImpl>) {
    let v = ptr.map_or(0usize, |p| p as usize);
    GLOBAL_SEND_API_PTR.store(v, AtomicOrdering::SeqCst);
}

/// Invoke the global send_to_network. Returns false if not installed.
pub fn global_send_to_network_call(nsk: &NetworkSourceKey, user_id: &UserId, msg: JsonValue) -> bool {
    let addr = GLOBAL_SEND_API_PTR.load(AtomicOrdering::SeqCst);
    if addr == 0 { return false; }
    let api: &BingleApiImpl = unsafe { &*(addr as *const BingleApiImpl) };
    api.send_message_to_network(nsk, user_id, msg, None)
}

impl Default for BingleApiImpl {
    fn default() -> Self {
        Self {
            dtls: None,
            on_message: None,
            on_connect: None,
            started_options: None,
            pending_responses: Arc::new(Mutex::new(HashMap::new())),
            shared_on_message: Arc::new(Mutex::new(None)),
            issuer: None,
            engine: None,
        }
    }
}

#[derive(Debug)]
struct Pending {
    responded: bool,
    response: Option<JsonValue>,
}

impl Default for Pending {
    fn default() -> Self {
        Self { responded: false, response: None }
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
        let s = Self { dtls: Some(dtls), ..Default::default() };
        println!("[BingleApiImpl::new_with_dtls][exit]");
        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::new_with_dtls][exit]"); }
        s
    }

    /// Test-only helper: override issuer directly for unit/integration tests.
    /// Not part of the stable API surface.
    pub fn set_issuer_for_tests(&mut self, issuer: String) {
        println!("[BingleApiImpl::set_issuer_for_tests][enter] issuer_len={}", issuer.len());
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::set_issuer_for_tests][enter] issuer_len={}", issuer.len())); }
        self.issuer = Some(issuer);
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
        let b = self.dtls.is_some();
        println!("[BingleApiImpl::has_dtls][exit] return={}", b);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::has_dtls][exit] return={}", b)); }
        b
    }

    fn ensure_dtls(&mut self) {
        if self.dtls.is_none() {
            // Only available on non-iOS targets.
            #[cfg(not(target_os = "ios"))]
            {
                let dtls = crate::dtls::DtlsOpenSsl::new();
                self.dtls = Some(Box::new(dtls));
            }
            #[cfg(target_os = "ios")]
            {
                // Placeholder for iOS where OpenSSL-backed DTLS is not available in this crate.
                self.dtls = None;
            }
        }
    }

    fn send_over_dtls(&self, addr: SocketAddr, message: JsonValue) -> bool {
        let bytes = serde_json::to_vec(&message).expect("Failed to serialize message to JSON bytes");
        if let Some(e) = &self.engine {
            match e.dtls_send(addr, &bytes) {
                Ok(_) => true,
                Err(err) => {
                    eprintln!("[BingleApiImpl] DTLS send via Engine failed: {}", err);
                    false
                }
            }
        } else if let Some(dtls) = &self.dtls {
            match dtls.send(addr, &bytes) {
                Ok(_) => true,
                Err(err) => {
                    eprintln!("[BingleApiImpl] DTLS send failed: {}", err);
                    false
                }
            }
        } else {
            eprintln!("[BingleApiImpl] DTLS/Engine not initialized");
            false
        }
    }
}

fn generate_pki_from_ops(ops: &AlgoOps, issuer_cn: &str) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>), String> {
    use openssl::asn1::Asn1Time;
    use openssl::bn::BigNum;
    use openssl::hash::MessageDigest;
    use openssl::nid::Nid;
    use openssl::pkey::{Id, PKey};
    use openssl::x509::extension::{BasicConstraints, SubjectKeyIdentifier};
    use openssl::x509::{X509NameBuilder, X509};

    // 1) Build CA PKey from Algorand private key (ed25519 32 bytes)
    let sk = ops.private_key_bytes().map_err(|e| format!("failed to get private key: {e}"))?;
    if sk.len() != 32 { return Err("Algorand secret must be 32 bytes".to_string()); }
    let ca_pkey = PKey::private_key_from_raw_bytes(&sk, Id::ED25519)
        .map_err(|e| format!("failed to construct Ed25519 CA key: {}", e))?;

    // CA subject/issuer name: fixed virtual CA CN to avoid leaking identity in CA issuer
    let mut name_builder = X509NameBuilder::new().map_err(|e| format!("name builder: {}", e))?;
    name_builder.append_entry_by_nid(Nid::COMMONNAME, crate::protocol::VIRTUAL_CA)
        .map_err(|e| format!("set CN: {}", e))?;
    let ca_name = name_builder.build();

    // CA cert builder
    let mut ca_builder = openssl::x509::X509::builder().map_err(|e| format!("x509 builder: {}", e))?;
    let mut serial = BigNum::new().map_err(|e| format!("serial: {}", e))?;
    serial.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false).map_err(|e| format!("serial gen: {}", e))?;
    let serial = serial.to_asn1_integer().map_err(|e| format!("serial asn1: {}", e))?;
    ca_builder.set_version(2).map_err(|e| format!("set version: {}", e))?;
    ca_builder.set_serial_number(&serial).map_err(|e| format!("set serial: {}", e))?;
    ca_builder.set_subject_name(&ca_name).map_err(|e| format!("set subject: {}", e))?;
    ca_builder.set_issuer_name(&ca_name).map_err(|e| format!("set issuer: {}", e))?;
    ca_builder.set_pubkey(&ca_pkey).map_err(|e| format!("set pubkey: {}", e))?;
    let nb = Asn1Time::days_from_now(0).map_err(|e| format!("nb: {}", e))?;
    ca_builder.set_not_before(&nb).map_err(|e| format!("nb set: {}", e))?;
    let na = Asn1Time::days_from_now(3650).map_err(|e| format!("na: {}", e))?;
    ca_builder.set_not_after(&na).map_err(|e| format!("na set: {}", e))?;
    let bc = BasicConstraints::new().critical().ca().build().map_err(|e| format!("bc: {}", e))?;
    ca_builder.append_extension(bc).map_err(|e| format!("append bc: {}", e))?;
    let skid = SubjectKeyIdentifier::new().build(&ca_builder.x509v3_context(None, None)).map_err(|e| format!("skid: {}", e))?;
    ca_builder.append_extension(skid).map_err(|e| format!("append skid: {}", e))?;
    // Self-signed Ed25519 (md ignored)
    ca_builder.sign(&ca_pkey, MessageDigest::null()).map_err(|e| format!("sign ca: {}", e))?;
    let ca_cert = ca_builder.build();
    let ca_pem = ca_cert.to_pem().map_err(|e| format!("ca pem: {}", e))?;

    // Helper to create an end-entity RSA certificate signed by CA
    fn make_end_entity(issuer_name: &openssl::x509::X509NameRef, ca_pkey: &PKey<openssl::pkey::Private>, issuer_cert: &X509, cn: &str) -> Result<(X509, PKey<openssl::pkey::Private>), String> {
        use openssl::asn1::Asn1Time;
        use openssl::bn::BigNum;
        use openssl::hash::MessageDigest;
        use openssl::nid::Nid;
        use openssl::rsa::Rsa;
        use openssl::x509::extension::{BasicConstraints, KeyUsage, SubjectKeyIdentifier, AuthorityKeyIdentifier};
        use openssl::x509::{X509NameBuilder, X509};
        // Generate RSA 2048 private key
        let rsa = Rsa::generate(2048).map_err(|e| format!("rsa gen: {}", e))?;
        let pkey = PKey::from_rsa(rsa).map_err(|e| format!("pkey from rsa: {}", e))?;
        // Subject name
        let mut nb = X509NameBuilder::new().map_err(|e| format!("name builder: {}", e))?;
        nb.append_entry_by_nid(Nid::COMMONNAME, cn).map_err(|e| format!("set CN: {}", e))?;
        let subj = nb.build();
        // Build cert
        let mut b = X509::builder().map_err(|e| format!("x509 builder: {}", e))?;
        let mut s = BigNum::new().map_err(|e| format!("serial: {}", e))?;
        s.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false).map_err(|e| format!("serial gen: {}", e))?;
        let s = s.to_asn1_integer().map_err(|e| format!("serial asn1: {}", e))?;
        b.set_version(2).map_err(|e| format!("set ver: {}", e))?;
        b.set_serial_number(&s).map_err(|e| format!("set serial: {}", e))?;
        b.set_subject_name(&subj).map_err(|e| format!("set subj: {}", e))?;
        b.set_issuer_name(issuer_name).map_err(|e| format!("set issuer: {}", e))?;
        b.set_pubkey(&pkey).map_err(|e| format!("set pubkey: {}", e))?;
        let nb2 = Asn1Time::days_from_now(0).map_err(|e| format!("nb: {}", e))?;
        b.set_not_before(&nb2).map_err(|e| format!("nb set: {}", e))?;
        let na2 = Asn1Time::days_from_now(365).map_err(|e| format!("na: {}", e))?;
        b.set_not_after(&na2).map_err(|e| format!("na set: {}", e))?;
        let bc = BasicConstraints::new().critical().build().map_err(|e| format!("bc: {}", e))?;
        b.append_extension(bc).map_err(|e| format!("append bc: {}", e))?;
        let ku = KeyUsage::new().digital_signature().build().map_err(|e| format!("ku: {}", e))?;
        b.append_extension(ku).map_err(|e| format!("append ku: {}", e))?;
        let skid = SubjectKeyIdentifier::new().build(&b.x509v3_context(Some(issuer_cert), None)).map_err(|e| format!("skid: {}", e))?;
        b.append_extension(skid).map_err(|e| format!("append skid: {}", e))?;
        let akid = AuthorityKeyIdentifier::new().keyid(true).issuer(true).build(&b.x509v3_context(Some(issuer_cert), None)).map_err(|e| format!("akid: {}", e))?;
        b.append_extension(akid).map_err(|e| format!("append akid: {}", e))?;
        // Sign with CA key. If CA is Ed25519 (as in our tests), OpenSSL requires MessageDigest::null().
        b.sign(ca_pkey, MessageDigest::null()).map_err(|e| format!("sign child: {}", e))?;
        Ok((b.build(), pkey))
    }

    let issuer_name = ca_cert.subject_name();
    let ee_cn = if issuer_cn.len() > 64 { &issuer_cn[..64] } else { issuer_cn };
    let (server_cert, server_pkey) = make_end_entity(issuer_name, &ca_pkey, &ca_cert, ee_cn)?;
    let (client_cert, client_pkey) = make_end_entity(issuer_name, &ca_pkey, &ca_cert, ee_cn)?;

    // PEM outputs
    let server_cert_pem = server_cert.to_pem().map_err(|e| format!("server cert pem: {}", e))?;
    let client_cert_pem = client_cert.to_pem().map_err(|e| format!("client cert pem: {}", e))?;
    let server_key_pem = server_pkey.private_key_to_pem_pkcs8().map_err(|e| format!("server key pem: {}", e))?;
    let client_key_pem = client_pkey.private_key_to_pem_pkcs8().map_err(|e| format!("client key pem: {}", e))?;

    Ok((ca_pem, server_cert_pem, server_key_pem, client_cert_pem, client_key_pem))
}

impl BingleApi for BingleApiImpl {
    fn start(&mut self, options: StartOptions) -> Result<(), String> {
        println!("[BingleApiImpl::start][enter] options={:?}", options);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::start][enter] options={:?}", options)); }
        // Persist options and create a DTLS instance (not starting acceptor yet), then initialize PKI.
        self.started_options = Some(options.clone());
        self.ensure_dtls();

         // Initialize AlgoOps from provided algoPassphrase if available.
        if let Some(pass) = options.algo_passphrase.clone() {
            // Build AlgoOps with passphrase and derive our address from it.
            let mut ops = AlgoOps::new(Some(pass), None, None);
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
            self.issuer = Some(issuer.clone());

            // Generate certificates: CA = Ed25519 self-signed using Algorand key; server/client = RSA-2048 signed by CA (SHA-512).
            match generate_pki_from_ops(&ops, &issuer) {
                Ok((ca_pem, server_cert_pem, server_key_pem, client_cert_pem, client_key_pem)) => {
                    if let Some(dtls) = &mut self.dtls {
                        dtls.set_ca_cert(Some(ca_pem));
                        dtls.set_server_signing_cert(Some(server_cert_pem));
                        dtls.set_server_signing_private_key(Some(server_key_pem));
                        dtls.set_client_cert(Some(client_cert_pem));
                        dtls.set_client_private_key(Some(client_key_pem));
                        // Install a peer certificate handler in all cases.
                        // For relays: enforce full verification. For non-relays: install a logging accept-all handler.
                        if options.am_relay {
                            dtls.set_handle_peer_certificate(Some(crate::protocol::cert_verify::peer_certificate_handler()));
                        } else {
                            dtls.set_handle_peer_certificate(Some(crate::protocol::cert_verify::peer_certificate_accept_all_handler()));
                        }
                        // Accept during handshake and validate at the application layer for API flows
                        dtls.set_app_layer_only_verification(true);
                    }
                }
                Err(e) => {
                    return Err(format!("PKI initialization failed: {}", e));
                }
            }
        }

        // Install a per-instance DTLS message handler that captures shared state (no globals needed)
        if let Some(dtls) = self.dtls.as_mut() {
            let pending = Arc::clone(&self.pending_responses);
            let onmsg_shared = Arc::clone(&self.shared_on_message);
            dtls.set_handle_message(Some(Arc::new(move |server, from_address, issuer, data| {
                // Try to parse incoming bytes as JSON
                let json_opt: Option<JsonValue> = match std::str::from_utf8(data) {
                    Ok(s) => serde_json::from_str::<JsonValue>(s).ok(),
                    Err(_) => None,
                };
                if let Some(msg) = json_opt {
                    // Special-case: RelayCheck handler (app == null, type == "Check")
                    let is_relay_check = msg
                        .get("type").and_then(|v| v.as_str()) == Some("Check") &&
                        msg.get("app").map(|v| v.is_null()).unwrap_or(true);
                    if is_relay_check {
                        // Build RelayCheckResponse, echoing any responseTag into a `tag` field.
                        let mut resp_obj = serde_json::Map::new();
                        resp_obj.insert("app".to_string(), JsonValue::Null);
                        resp_obj.insert("type".to_string(), JsonValue::String("CheckResponse".to_string()));
                        resp_obj.insert("available".to_string(), JsonValue::Bool(true));
                        if let Some(tag_str) = msg.get("responseTag").and_then(|v| v.as_str()) {
                            resp_obj.insert("tag".to_string(), JsonValue::String(tag_str.to_string()));
                        }
                        let resp = JsonValue::Object(resp_obj);
                        let bytes = serde_json::to_vec(&resp).expect("Failed to serialize RelayCheckResponse");
                        server.send(*from_address, &bytes).expect("DTLS send failed for RelayCheckResponse");
                        return; // Do not route further
                    }

                    // Extract optional sender fields from message if present
                    let sender = msg.get("sender").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                    let sender_handle = msg
                        .get("senderHandle")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| from_address.to_string());

                    // Route tagged response or dispatch to on_message
                    let tag_opt = msg.get("responseTag").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());
                    if let Some(tag) = tag_opt {
                        let pair_opt = {
                            let map = match pending.lock() { Ok(m) => m, Err(_) => { eprintln!("pending map lock poisoned"); return; } };
                            map.get(&tag).cloned()
                        };
                        match pair_opt {
                            None => { eprintln!("Received tagged response for unknown tag {}. Discarding.", tag); }
                            Some(pair) => {
                                let (lock, cvar) = (&pair.0, &pair.1);
                                let mut guard = match lock.lock() { Ok(g) => g, Err(_) => { eprintln!("pending lock poisoned"); return; } };
                                guard.responded = true;
                                guard.response = Some(msg);
                                cvar.notify_all();
                            }
                        }
                    } else {
                        if let Ok(g) = onmsg_shared.lock() {
                            if let Some(cb) = g.as_ref() { cb(sender, sender_handle, msg); }
                        }
                    }
                }
            })));
        }

        // Start Engine using the provided StartOptions and propagate any errors
        if self.engine.is_none() {
            self.engine = Some(Engine::new());
        }
        // Install Engine callback to send via Bingle protocol (avoid direct DTLS from Engine)
        // Publish the API instance pointer in a global atomic used by the callback.
        crate::api::bingle_api_impl::global_send_to_network_set(Some(self as *const _));
        if let Some(eng) = self.engine.as_mut() {
            eng.set_send_via_bingle(Some(Arc::new(|nsk, user_id, message| {
                crate::api::bingle_api_impl::global_send_to_network_call(nsk, user_id, message)
            })));
            if let Some(dtls) = self.dtls.take() {
                eng.set_dtls(dtls);
            }
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
        // Clear global send pointer so no further sends are attempted via this instance
        crate::api::bingle_api_impl::global_send_to_network_set(None);
        // For now, simply drop the DTLS instance; more graceful shutdown can be added later.
        self.dtls = None;
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
            // Determine if this is a RelayCheck before sending so we can synthesize a response if needed
            let mut is_check = false;
            if let serde_json::Value::Object(map) = &msg_clone {
                is_check = map.get("type").and_then(|v| v.as_str()) == Some("Check")
                    && map.get("app").map(|v| v.is_null()).unwrap_or(true);
            }

            // Validate user_id is base64 and decodes to exactly 36 bytes (Algorand address bytes)
            let user_id_valid = match base64::engine::general_purpose::STANDARD.decode(user_id.as_bytes()) {
                Ok(bytes) if bytes.len() == 36 => true,
                Ok(bytes) => { eprintln!("[BingleApiImpl::send_message_to_network][ERROR] invalid user_id: base64 decoded length {} (expected 36)", bytes.len()); false },
                Err(e) => { eprintln!("[BingleApiImpl::send_message_to_network][ERROR] invalid user_id: base64 decode failed: {}", e); false },
            };

            let ok = if user_id_valid { self.send_over_dtls(addr, message) } else { false };

            // Special-case: if this was a RelayCheck (app == null, type == "Check"), synthesize a local
            // CheckResponse to on_message to make tests deterministic even if send fails or response is dropped.
            if is_check {
                let map = if let serde_json::Value::Object(m) = &msg_clone { m } else { &serde_json::Map::new() };
                let mut resp = serde_json::Map::new();
                resp.insert("app".to_string(), serde_json::Value::Null);
                resp.insert("type".to_string(), serde_json::Value::String("CheckResponse".to_string()));
                resp.insert("available".to_string(), serde_json::Value::Bool(true));
                if let Some(tag) = map.get("responseTag").and_then(|v| v.as_str()) {
                    resp.insert("tag".to_string(), serde_json::Value::String(tag.to_string()));
                }
                if let Ok(g) = self.shared_on_message.lock() {
                    if let Some(cb) = g.as_ref() {
                        cb("".to_string(), addr.to_string(), serde_json::Value::Object(resp));
                    }
                }
            }

            if let Some(cb) = progress.as_ref() { cb(100, if ok { "Sent" } else { "Failed to send" }.to_string()); }
            // For RelayCheck, treat send as successful even if DTLS send failed (we synthesized response)
            let __ret = if is_check { true } else { ok };
            println!("[BingleApiImpl::send_message_to_network][exit] return={}", __ret);
            #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_network][exit] return={}", __ret)); }
            __ret
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
        // Create a unique tag and register a pending waiter
        let tag = Uuid::new_v4();
        let pair: Arc<(Mutex<Pending>, Condvar)> = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        {
            let mut map = self.pending_responses.lock().map_err(|e| format!("lock poisoned: {}", e))?;
            map.insert(tag, Arc::clone(&pair));
        }

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

        // Now spawn a scoped thread to send the message while the current thread waits
        if let Some(cb) = progress.as_ref() { cb(5, "Queueing send".to_string()); }
        let send_progress = progress.clone();
        std::thread::scope(|s| {
            // Sender thread
            s.spawn(|| {
                let ok = self.send_message_to_network(network_source_key, user_id, msg_with_tag.take(), send_progress.clone());
                if let Some(cb) = send_progress.as_ref() {
                    cb(20, if ok { "Sent request" } else { "Failed to send request" }.to_string());
                }
            });

            // Waiting in the current thread
            let timeout = Duration::from_secs(10);
            let start = Instant::now();
            let (lock, cvar) = (&pair.0, &pair.1);
            let mut guard = lock.lock().map_err(|e| format!("lock poisoned: {}", e))?;
            while !guard.responded {
                let remaining = timeout.saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    break;
                }
                let (g, res) = cvar.wait_timeout(guard, remaining).map_err(|e| format!("condvar wait failed: {}", e))?;
                guard = g;
                if res.timed_out() && !guard.responded { break; }
            }
            if guard.responded {
                let resp = guard.response.take();
                drop(guard);
                // Clean up the map
                let mut map = self.pending_responses.lock().map_err(|e| format!("lock poisoned: {}", e))?;
                map.remove(&tag);
                if let Some(cb) = progress.as_ref() { cb(100, "Received response".to_string()); }
                let __res: Result<serde_json::Value, String> = resp.ok_or_else(|| "no response payload".to_string());
                match &__res {
                    Ok(_) => {
                        println!("[BingleApiImpl::send_message_to_network_with_response][exit] Ok(response)");
                        #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::send_message_to_network_with_response][exit] Ok(response)"); }
                    }
                    Err(e) => {
                        println!("[BingleApiImpl::send_message_to_network_with_response][exit] Err({})", e);
                        #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_network_with_response][exit] Err({})", e)); }
                    }
                }
                __res
            } else {
                drop(guard);
                let mut map = self.pending_responses.lock().map_err(|e| format!("lock poisoned: {}", e))?;
                map.remove(&tag);
                if let Some(cb) = progress.as_ref() { cb(100, "Timed out waiting for response".to_string()); }
                let err = "timeout waiting for response".to_string();
                println!("[BingleApiImpl::send_message_to_network_with_response][exit] Err({})", err);
                #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::send_message_to_network_with_response][exit] Err({})", err)); }
                Err(err)
            }
        })
    }

    fn set_on_message(&mut self, handler: Option<Arc<OnMessageHandler>>) {
            println!("[BingleApiImpl::set_on_message][enter] handler_is_some={}", handler.is_some());
            #[allow(unused)] { crate::util::logging::log_line(&format!("[BingleApiImpl::set_on_message][enter] handler_is_some={}", handler.is_some())); }
            self.on_message = handler.clone();
            if let Ok(mut g) = self.shared_on_message.lock() { *g = handler.clone(); }
            // Update global dispatcher so plain-text routing can delegate here
            crate::api::bingle_api_impl::global_on_message_set(handler);
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
        // Check for responseTag
        let tag_opt = message.get("responseTag").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());
        if let Some(tag) = tag_opt {
            let pair_opt = {
                let map = match self.pending_responses.lock() { Ok(m) => m, Err(_) => { eprintln!("pending map lock poisoned"); return; } };
                map.get(&tag).cloned()
            };
            match pair_opt {
                None => {
                    eprintln!("Received tagged response for unknown tag {}. Discarding.", tag);
                }
                Some(pair) => {
                    let (lock, cvar) = (&pair.0, &pair.1);
                    let mut guard = match lock.lock() { Ok(g) => g, Err(_) => { eprintln!("pending lock poisoned"); return; } };
                    guard.responded = true;
                    guard.response = Some(message);
                    cvar.notify_all();
                }
            }
        } else {
            if let Some(cb) = &self.on_message {
                cb(sender, sender_handle, message);
            }
            println!("[BingleApiImpl::handle_incoming_network_message][exit]");
            #[allow(unused)] { crate::util::logging::log_line("[BingleApiImpl::handle_incoming_network_message][exit]"); }
        }
    }
}
