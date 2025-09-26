use std::net::SocketAddr;
use std::sync::Arc;

use serde_json::Value as JsonValue;

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
    issuer: Option<String>,
}

impl Default for BingleApiImpl {
    fn default() -> Self {
        Self { dtls: None, on_message: None, on_connect: None, started_options: None, issuer: None }
    }
}

impl BingleApiImpl {
    pub fn new() -> Self { Self::default() }

    /// Test-oriented constructor to inject a custom DTLS implementation.
    pub fn new_with_dtls(dtls: Box<dyn Dtls + Send + Sync>) -> Self {
        Self { dtls: Some(dtls), ..Default::default() }
    }

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
    use openssl::rsa::Rsa;
    use openssl::x509::extension::{BasicConstraints, KeyUsage, SubjectKeyIdentifier, AuthorityKeyIdentifier};
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
        _network_source_key: &NetworkSourceKey,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, String> {
        Err("not implemented".to_string())
    }

    fn set_on_message(&mut self, handler: Option<Arc<OnMessageHandler>>) { self.on_message = handler; }

    fn set_on_connect(&mut self, handler: Option<Arc<OnConnectHandler>>) { self.on_connect = handler; }
}
