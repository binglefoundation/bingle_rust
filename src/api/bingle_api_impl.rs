use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, Condvar};
use std::time::{Duration, Instant};

use serde_json::{Value as JsonValue, Map as JsonMap};
use uuid::Uuid;

use crate::api::bingle_api::{BingleApi, Handle, NetworkSourceKey, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use crate::dtls::Dtls;
use crate::protocol::ISSUER_SUFFIX;
use crate::blockchain::algo_ops::{AlgoOps, byte_key_to_address};

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
    pub fn new() -> Self { Self::default() }
}

impl BingleApiImpl {
    /// Test-oriented constructor to inject a custom DTLS implementation.
    pub fn new_with_dtls(dtls: Box<dyn Dtls + Send + Sync>) -> Self {
        Self { dtls: Some(dtls), ..Default::default() }
    }

    /// Test-only helper: override issuer directly for unit/integration tests.
    /// Not part of the stable API surface.
    pub fn set_issuer_for_tests(&mut self, issuer: String) { self.issuer = Some(issuer); }

    /// Exposed for integration tests: whether a DTLS instance has been created.
    pub fn has_dtls(&self) -> bool { self.dtls.is_some() }

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
        let bytes = match serde_json::to_vec(&message) {
            Ok(b) => b,
            Err(_) => return false,
        };
        if let Some(dtls) = &self.dtls {
            let issuer = self.issuer.as_deref().unwrap_or("");
            dtls.send(addr, issuer, &bytes).is_ok()
        } else {
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
        .map_err(|_| "failed to construct Ed25519 CA key".to_string())?;

    // CA subject/issuer name
    let mut name_builder = X509NameBuilder::new().map_err(|_| "name builder".to_string())?;
    name_builder.append_entry_by_nid(Nid::COMMONNAME, issuer_cn).map_err(|_| "set CN".to_string())?;
    let ca_name = name_builder.build();

    // CA cert builder
    let mut ca_builder = openssl::x509::X509::builder().map_err(|_| "x509 builder".to_string())?;
    let mut serial = BigNum::new().map_err(|_| "serial".to_string())?;
    serial.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false).map_err(|_| "serial gen".to_string())?;
    let serial = serial.to_asn1_integer().map_err(|_| "serial asn1".to_string())?;
    ca_builder.set_version(2).map_err(|_| "set version".to_string())?;
    ca_builder.set_serial_number(&serial).map_err(|_| "set serial".to_string())?;
    ca_builder.set_subject_name(&ca_name).map_err(|_| "set subject".to_string())?;
    ca_builder.set_issuer_name(&ca_name).map_err(|_| "set issuer".to_string())?;
    ca_builder.set_pubkey(&ca_pkey).map_err(|_| "set pubkey".to_string())?;
    let nb = Asn1Time::days_from_now(0).map_err(|_| "nb".to_string())?;
    ca_builder.set_not_before(&nb).map_err(|_| "nb set".to_string())?;
    let na = Asn1Time::days_from_now(3650).map_err(|_| "na".to_string())?;
    ca_builder.set_not_after(&na).map_err(|_| "na set".to_string())?;
    let bc = BasicConstraints::new().critical().ca().build().map_err(|_| "bc".to_string())?;
    ca_builder.append_extension(bc).map_err(|_| "append bc".to_string())?;
    let skid = SubjectKeyIdentifier::new().build(&ca_builder.x509v3_context(None, None)).map_err(|_| "skid".to_string())?;
    ca_builder.append_extension(skid).map_err(|_| "append skid".to_string())?;
    // Self-signed Ed25519 (md ignored)
    ca_builder.sign(&ca_pkey, MessageDigest::null()).map_err(|_| "sign ca".to_string())?;
    let ca_cert = ca_builder.build();
    let ca_pem = ca_cert.to_pem().map_err(|_| "ca pem".to_string())?;

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
        let rsa = Rsa::generate(2048).map_err(|_| "rsa gen".to_string())?;
        let pkey = PKey::from_rsa(rsa).map_err(|_| "pkey from rsa".to_string())?;
        // Subject name
        let mut nb = X509NameBuilder::new().map_err(|_| "name builder".to_string())?;
        nb.append_entry_by_nid(Nid::COMMONNAME, cn).map_err(|_| "set CN".to_string())?;
        let subj = nb.build();
        // Build cert
        let mut b = X509::builder().map_err(|_| "x509 builder".to_string())?;
        let mut s = BigNum::new().map_err(|_| "serial".to_string())?;
        s.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false).map_err(|_| "serial gen".to_string())?;
        let s = s.to_asn1_integer().map_err(|_| "serial asn1".to_string())?;
        b.set_version(2).map_err(|_| "set ver".to_string())?;
        b.set_serial_number(&s).map_err(|_| "set serial".to_string())?;
        b.set_subject_name(&subj).map_err(|_| "set subj".to_string())?;
        b.set_issuer_name(issuer_name).map_err(|_| "set issuer".to_string())?;
        b.set_pubkey(&pkey).map_err(|_| "set pubkey".to_string())?;
        let nb2 = Asn1Time::days_from_now(0).map_err(|_| "nb".to_string())?;
        b.set_not_before(&nb2).map_err(|_| "nb set".to_string())?;
        let na2 = Asn1Time::days_from_now(365).map_err(|_| "na".to_string())?;
        b.set_not_after(&na2).map_err(|_| "na set".to_string())?;
        let bc = BasicConstraints::new().critical().build().map_err(|_| "bc".to_string())?;
        b.append_extension(bc).map_err(|_| "append bc".to_string())?;
        let ku = KeyUsage::new().digital_signature().build().map_err(|_| "ku".to_string())?;
        b.append_extension(ku).map_err(|_| "append ku".to_string())?;
        let skid = SubjectKeyIdentifier::new().build(&b.x509v3_context(Some(issuer_cert), None)).map_err(|_| "skid".to_string())?;
        b.append_extension(skid).map_err(|_| "append skid".to_string())?;
        let akid = AuthorityKeyIdentifier::new().keyid(true).issuer(true).build(&b.x509v3_context(Some(issuer_cert), None)).map_err(|_| "akid".to_string())?;
        b.append_extension(akid).map_err(|_| "append akid".to_string())?;
        // Sign with CA using SHA-512. Note: this produces RSA-SHA512 signature if CA is RSA; with Ed25519 CA, it will be Ed25519.
        b.sign(ca_pkey, MessageDigest::sha512()).map_err(|_| "sign child".to_string())?;
        Ok((b.build(), pkey))
    }

    let issuer_name = ca_cert.subject_name();
    let (server_cert, server_pkey) = make_end_entity(issuer_name, &ca_pkey, &ca_cert, issuer_cn)?;
    let (client_cert, client_pkey) = make_end_entity(issuer_name, &ca_pkey, &ca_cert, issuer_cn)?;

    // PEM outputs
    let server_cert_pem = server_cert.to_pem().map_err(|_| "server cert pem".to_string())?;
    let client_cert_pem = client_cert.to_pem().map_err(|_| "client cert pem".to_string())?;
    let server_key_pem = server_pkey.private_key_to_pem_pkcs8().map_err(|_| "server key pem".to_string())?;
    let client_key_pem = client_pkey.private_key_to_pem_pkcs8().map_err(|_| "client key pem".to_string())?;

    Ok((ca_pem, server_cert_pem, server_key_pem, client_cert_pem, client_key_pem))
}

impl BingleApi for BingleApiImpl {
    fn start(&mut self, options: StartOptions) -> Result<(), String> {
        // Persist options and create a DTLS instance (not starting acceptor yet), then initialize PKI.
        self.started_options = Some(options.clone());
        self.ensure_dtls();

         // Initialize AlgoOps from provided algoPassphrase if available.
        if let Some(pass) = options.algo_passphrase.clone() {
            // Build AlgoOps with passphrase and derive our address from it.
            let mut ops = AlgoOps::new(Some(pass), None, None);
            // Derive address from the private key bytes.
            if let Ok(sk_bytes) = ops.private_key_bytes() {
                if sk_bytes.len() == 32 {
                    if let Ok(arr) = <[u8; 32]>::try_from(sk_bytes.as_slice()) {
                        let signing = ed25519_dalek::SigningKey::from_bytes(&arr);
                        let pk: [u8; 32] = signing.verifying_key().to_bytes();
                        if let Ok(addr) = byte_key_to_address(&pk) {
                            ops.address = Some(addr);
                        }
                    }
                }
            }
            if let Some(addr) = ops.address.clone() {
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
                            // Install default peer certificate handler for verification
                            dtls.set_handle_peer_certificate(Some(crate::protocol::cert_verify::peer_certificate_handler()));
                        }
                    }
                    Err(e) => {
                        return Err(format!("PKI initialization failed: {}", e));
                    }
                }
            }
        }

        // Install a per-instance DTLS message handler that captures shared state (no globals needed)
        if let Some(dtls) = self.dtls.as_mut() {
            let pending = Arc::clone(&self.pending_responses);
            let onmsg_shared = Arc::clone(&self.shared_on_message);
            dtls.set_handle_message(Some(Arc::new(move |server, from_address, data| {
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
                        if let Ok(bytes) = serde_json::to_vec(&resp) {
                            let _ = server.send(*from_address, &bytes);
                        }
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
        Ok(())
    }

    fn stop(&mut self) {
        // For now, simply drop the DTLS instance; more graceful shutdown can be added later.
        self.dtls = None;
    }

    fn network_change(&mut self) {
        // Placeholder: in a full implementation, we would rescan STUN/static IP and update listeners.
    }

    fn send_message_to_id(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool {
        // Not implemented yet
        false
    }

    fn send_message_to_handle(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool {
        // Not implemented yet
        false
    }

    fn send_message_to_network(
        &self,
        network_source_key: &NetworkSourceKey,
        _user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> bool {
        if let Some(cb) = progress.as_ref() { cb(10, "Preparing send".to_string()); }
        // Only direct socket address path is implemented at this stage.
        if let Some(addr) = network_source_key.inet_socket_address {
            let ok = self.send_over_dtls(addr, message);
            if let Some(cb) = progress.as_ref() { cb(100, if ok { "Sent" } else { "Failed to send" }.to_string()); }
            ok
        } else {
            if let Some(cb) = progress.as_ref() { cb(100, "Relay send not yet implemented".to_string()); }
            false
        }
    }

    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        Err("not implemented".to_string())
    }

    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        Err("not implemented".to_string())
    }

    fn send_message_to_network_with_response(
        &self,
        network_source_key: &NetworkSourceKey,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, String> {
        // Create a unique tag and register a pending waiter
        let tag = Uuid::new_v4();
        let pair: Arc<(Mutex<Pending>, Condvar)> = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        {
            let mut map = self.pending_responses.lock().map_err(|_| "lock poisoned")?;
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
            let mut guard = lock.lock().map_err(|_| "lock poisoned")?;
            while !guard.responded {
                let remaining = timeout.saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    break;
                }
                let (g, res) = cvar.wait_timeout(guard, remaining).map_err(|_| "condvar wait failed")?;
                guard = g;
                if res.timed_out() && !guard.responded { break; }
            }
            if guard.responded {
                let resp = guard.response.take();
                drop(guard);
                // Clean up the map
                let mut map = self.pending_responses.lock().map_err(|_| "lock poisoned")?;
                map.remove(&tag);
                if let Some(cb) = progress.as_ref() { cb(100, "Received response".to_string()); }
                resp.ok_or_else(|| "no response payload".to_string())
            } else {
                drop(guard);
                let mut map = self.pending_responses.lock().map_err(|_| "lock poisoned")?;
                map.remove(&tag);
                if let Some(cb) = progress.as_ref() { cb(100, "Timed out waiting for response".to_string()); }
                Err("timeout waiting for response".to_string())
            }
        })
    }

    fn set_on_message(&mut self, handler: Option<Arc<OnMessageHandler>>) {
            self.on_message = handler.clone();
            if let Ok(mut g) = self.shared_on_message.lock() { *g = handler; }
        }

    fn set_on_connect(&mut self, handler: Option<Arc<OnConnectHandler>>) { self.on_connect = handler; }
}

impl BingleApiImpl {
    /// Public entry from the networking layer for inbound messages.
    /// If message contains a responseTag, it is treated as a response and routed to waiter; otherwise it is dispatched to on_message.
    pub fn handle_incoming_network_message(&self, sender: UserId, sender_handle: Handle, message: JsonValue) {
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
        }
    }
}
