use anyhow::{anyhow, bail, Result};
use uuid::Uuid;
use data_encoding::BASE32_NOPAD;
use serde::{Deserialize, Serialize};
use base64::{engine::general_purpose, Engine as _};
use algonaut::core::ToMsgPack;

// Ensure the algonaut crate is referenced so this module is explicitly implemented with it.
// We keep most calls abstracted for now and will progressively replace stubs with concrete calls.
#[allow(unused_imports)]
use algonaut as _algonaut;

/// Configuration for Algod and Indexer endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AlgoChainConfig {
    pub client_api_url: String,
    pub client_api_port: u16,
    pub indexer_api_url: String,
    pub indexer_api_port: u16,
    pub token: Option<String>,
    pub token_key: Option<String>,
    pub app_id: Option<u64>,
    pub asset_id: Option<u64>,
}

impl Default for AlgoChainConfig {
    fn default() -> Self {
        // Defaults to localnet configuration matching the Kotlin implementation
        Self {
            client_api_url: "http://localhost".to_string(),
            client_api_port: 4001,
            indexer_api_url: "http://localhost".to_string(),
            indexer_api_port: 8980,
            token: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
            token_key: Some("X-Algo-API-Token".to_string()),
            app_id: None,
            asset_id: None,
        }
    }
}

/// Minimal key provider trait analogous to the Kotlin IKeyProvider interface.
pub trait KeyProvider: Send + Sync {
    fn get_id(&self) -> Option<String> { None }
    fn get_private_key(&self) -> Option<String> { None }
    fn save_private_key_object(&self, _address: &str, _passphrase: &str) {}
}

/// Main operations struct analogous to Kotlin's AlgoOps.
#[derive(Debug, Clone)]
pub struct AlgoOps {
    pub passphrase: Option<String>,
    pub address: Option<String>,
    pub config: AlgoChainConfig,
}

impl AlgoOps {
    /// Generate a new Algorand keypair (secure random) and return `(id, passphrase)`.
    ///
    /// - `id` is the Algorand base32 address derived from the public key.
    /// - `passphrase` is a base64-encoded 32-byte seed prefixed with `b64:` (compatible with AlgoOps::new).
    ///
    /// Note: We return a base64 passphrase to ensure compatibility with existing parsing
    /// logic (mnemonic first, then legacy `b64:`). Callers can store/display it or convert
    /// to a mnemonic externally if desired.
    pub fn generate_keypair() -> (String, String) {
        use ed25519_dalek::SigningKey;
        use rand_core::{OsRng, RngCore};

        // Generate a secure random 32-byte seed
        let mut rng = OsRng;
        let mut sk: [u8; 32] = [0u8; 32];
        rng.fill_bytes(&mut sk);

        // Derive public key and Algorand address
        let signing_key = SigningKey::from_bytes(&sk);
        let verifying_key = signing_key.verifying_key();
        let pk: [u8; 32] = verifying_key.to_bytes();
        let id = byte_key_to_address(&pk).expect("failed to derive Algorand address from public key");

        // Encode secret seed as base64 passphrase with b64: prefix (supported by AlgoOps::new)
        let passphrase = format!("b64:{}", general_purpose::STANDARD.encode(sk));
        (id, passphrase)
    }
    pub fn new(passphrase: Option<String>, address: Option<String>, config: Option<AlgoChainConfig>) -> Self {
        // Enforce that a source of address is provided (either explicit address or a passphrase/seed)
        if passphrase.is_none() && address.is_none() {
            panic!("AlgoOps::new requires either a passphrase or an address");
        }
        // Use explicit config if provided; else Default (localnet)
        let config = config.unwrap_or_default();
        let mut ops = Self {
            passphrase,
            address,
            config,
        };

        // If no address was provided but we have a passphrase, derive the address immediately
        if ops.address.is_none() {
            if let Some(ref pass) = ops.passphrase {
                // Try Algorand mnemonic first
                let maybe_seed = if !pass.is_empty() {
                    match algonaut::crypto::mnemonic::to_key(pass) {
                        Ok(k) if k.len() == 32 => Some(k.to_vec()),
                        _ => None,
                    }
                } else { None };

                let seed32: Option<[u8; 32]> = if let Some(bytes) = maybe_seed {
                    bytes.as_slice().try_into().ok()
                } else if let Some(rest) = pass.strip_prefix("b64:") {
                    match base64::engine::general_purpose::STANDARD.decode(rest) {
                        Ok(b) => b.as_slice().try_into().ok(),
                        Err(_) => None,
                    }
                } else {
                    None
                };

                if let Some(seed) = seed32 {
                    use ed25519_dalek::SigningKey;
                    let signing_key = SigningKey::from_bytes(&seed);
                    let verifying_key = signing_key.verifying_key();
                    let pk: [u8; 32] = verifying_key.to_bytes();
                    if let Ok(addr) = byte_key_to_address(&pk) {
                        ops.address = Some(addr);
                    }
                }
            }
        }

        ops
    }

    /// Helper to generate a unique note (e.g. for avoiding "transaction already in ledger" errors)
    pub fn unique_note() -> Vec<u8> {
        let mut n = Vec::new();
        n.extend_from_slice(Uuid::new_v4().as_bytes());
        n
    }

    fn algod_client(&self) -> Result<algonaut::algod::v2::Algod> {
        // Build base URL including port, e.g., http://localhost:4001
        let url = format!("{}:{}", self.config.client_api_url, self.config.client_api_port);
        // Per requirement: default the token to empty string for Algod::new calls
        let token = self.config.token.clone().unwrap_or_default();
        algonaut::algod::v2::Algod::new(&url, &token)
            .map_err(|e| anyhow!("failed to construct Algod client: {e}"))
    }

    // Helper: run an async future on a fresh current-thread Tokio runtime.
    fn rt_block_on<T: Send>(&self, fut: impl std::future::Future<Output = T> + Send) -> Result<T> {
        // If we are already in a tokio runtime, we must avoid nested block_on and 
        // the "cannot drop runtime" panic during unwinding/drop.
        if tokio::runtime::Handle::try_current().is_ok() {
            return std::thread::scope(|s| {
                let handle = s.spawn(|| {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("failed to build temporary tokio runtime");
                    rt.block_on(fut)
                });
                handle.join().map_err(|_| anyhow!("rt_block_on thread panicked"))
            });
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| anyhow!("failed to build tokio runtime: {e}"))?;
        Ok(rt.block_on(fut))
    }

    // Helper: parse address string into algonaut Address.
    fn parse_address(addr: &str) -> Result<algonaut::core::Address> {
        use std::str::FromStr;
        algonaut::core::Address::from_str(addr).map_err(|e| anyhow!("invalid address: {e}"))
    }

    // Helper: require self.address and parse it.
    fn require_address(&self) -> Result<algonaut::core::Address> {
        let addr_str = self
            .address
            .as_ref()
            .ok_or_else(|| anyhow!("This operation needs an address"))?;
        Self::parse_address(addr_str)
    }

    // Helper: JSON field accessor that supports snake_case and kebab-case alternatives.
    fn json_get<'a>(o: &'a serde_json::Value, k1: &str, k2: &str) -> Option<&'a serde_json::Value> {
        o.get(k1).or_else(|| o.get(k2))
    }

    // Helper: decode TEAL key/value state entries from JSON into vector of (key, value) strings.
    // - Keys are base64-encoded in algod JSON and represent UTF-8 strings: decode to UTF-8.
    // - Byte values (type == 1) may be represented as an array of numbers (bytes) or as a base64 string,
    //   depending on the source struct and serializer. Handle both. Attempt to decode to UTF-8; if not valid,
    //   fall back to a 0x-prefixed hex string for readability.
    fn decode_state_entries(entries: &[serde_json::Value]) -> Vec<(String, String)> {
        let mut kvs: Vec<(String, String)> = Vec::new();
        for entry in entries {
            let key_b64 = match entry.get("key").and_then(|x| x.as_str()) { Some(s) => s, None => continue };
            let key = match general_purpose::STANDARD.decode(key_b64) {
                Ok(bytes) => String::from_utf8(bytes).unwrap_or_else(|_| key_b64.to_string()),
                Err(_) => key_b64.to_string(),
            };
            let val_obj = match entry.get("value").and_then(|x| x.as_object()) { Some(o) => o, None => continue };
            let vtype = val_obj.get("type").and_then(|x| x.as_u64()).unwrap_or(0);
            let val = if vtype == 1 {
                // bytes can be an array of numbers or a base64 string
                let bytes_opt = if let Some(arr) = val_obj.get("bytes").and_then(|x| x.as_array()) {
                    // Collect numeric array into bytes
                    let mut buf: Vec<u8> = Vec::with_capacity(arr.len());
                    for n in arr {
                        if let Some(u) = n.as_u64() {
                            buf.push((u & 0xFF) as u8);
                        } else if let Some(i) = n.as_i64() {
                            if i >= 0 { buf.push(((i as u64) & 0xFF) as u8); }
                        }
                    }
                    Some(buf)
                } else if let Some(b64) = val_obj.get("bytes").and_then(|x| x.as_str()) {
                    match general_purpose::STANDARD.decode(b64) {
                        Ok(b) => Some(b),
                        Err(_) => None,
                    }
                } else {
                    None
                };

                if let Some(bytes) = bytes_opt {
                    match String::from_utf8(bytes.clone()) {
                        Ok(s) => s,
                        Err(_) => {
                            // Fallback: hex string for non-UTF8 bytes
                            let mut hex = String::from("0x");
                            for byte in bytes { hex.push_str(&format!("{:02x}", byte)); }
                            hex
                        }
                    }
                } else {
                    String::new()
                }
            } else {
                val_obj.get("uint").and_then(|x| x.as_u64()).map(|u| u.to_string()).unwrap_or_else(|| "0".to_string())
            };
            kvs.push((key, val));
        }
        kvs
    }

    /// Create a new address, generating an ed25519 keypair. For compatibility with
    /// existing tests, we store the secret key in `passphrase` as a base64-encoded
    /// string with a "b64:" prefix. Note: production code expects an ASCII mnemonic
    /// passphrase to be provided by callers; this storage is only used by tests.
    pub fn create_address(&mut self, _save: bool, _always_new_address: bool) -> Result<String> {
        use ed25519_dalek::SigningKey;
        use rand_core::{OsRng, RngCore};

        let mut rng = OsRng;
        let mut sk: [u8; 32] = [0u8; 32];
        rng.fill_bytes(&mut sk);
        let signing_key = SigningKey::from_bytes(&sk);
        let verifying_key = signing_key.verifying_key();
        let pk: [u8; 32] = verifying_key.to_bytes();

        let addr = byte_key_to_address(&pk)?;
        self.address = Some(addr.clone());
        // Store secret key as base64 so we can recover it for signing
        let sk_b64 = general_purpose::STANDARD.encode(sk);
        self.passphrase = Some(format!("b64:{}", sk_b64));
        Ok(addr)
    }

    pub fn public_key_bytes(&self) -> Result<[u8; 32]> {
        let addr = self.address.as_ref().ok_or_else(|| anyhow!("This operation needs an address"))?;
        address_to_byte_key(addr)
    }

    pub fn private_key_bytes(&self) -> Result<Vec<u8>> {
        let pass = self
            .passphrase
            .as_ref()
            .ok_or_else(|| anyhow!("This operation needs account access"))?;

        // New behavior: expect ASCII mnemonic passphrase. Derive 32-byte key using Algorand mnemonic.
        if !pass.is_empty() {
            if let Ok(key32) = algonaut::crypto::mnemonic::to_key(pass) {
                if key32.len() == 32 {
                    return Ok(key32.to_vec());
                } else {
                    bail!("Derived key must be 32 bytes, got {}", key32.len());
                }
            }
        }

        // Backward compatibility: support legacy base64 format with "b64:" prefix
        if let Some(rest) = pass.strip_prefix("b64:") {
            let sk = general_purpose::STANDARD.decode(rest).map_err(|e| anyhow!("Invalid base64 secret: {e}"))?;
            if sk.len() != 32 {
                bail!("Secret key must be 32 bytes, got {}", sk.len());
            }
            return Ok(sk);
        }

        bail!("Unsupported passphrase format: expected ASCII mnemonic")
    }

    /// Compute the application (contract) address from app id.
    /// Spec: pubkey = SHA512/256("appID" || app_id_be_u64), then Algorand address from pubkey.
    pub fn contract_address(&self, app_id: u64) -> Result<String> {
        use sha2::{Digest, Sha512_256};
        let mut hasher = Sha512_256::new();
        hasher.update(b"appID");
        hasher.update(app_id.to_be_bytes());
        let pk_bytes: [u8; 32] = hasher.finalize().into();
        byte_key_to_address(&pk_bytes)
    }

    pub fn sign(&self, text: &str) -> Result<String> {
        use ed25519_dalek::{Signer, SigningKey};
        let sk_bytes = self.private_key_bytes()?;
        let sk_arr: [u8; 32] = sk_bytes
            .try_into()
            .map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let signing_key = SigningKey::from_bytes(&sk_arr);
        let signature = signing_key.sign(text.as_bytes());
        Ok(general_purpose::STANDARD.encode(signature.to_bytes()))
    }

    pub fn verify(&self, text: &str, sig_b64: &str) -> Result<bool> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let sig_bytes = match general_purpose::STANDARD.decode(sig_b64) {
            Ok(b) => b,
            Err(_) => return Ok(false),
        };
        if sig_bytes.len() != 64 {
            return Ok(false);
        }
        let sig_arr: [u8; 64] = match sig_bytes.try_into() {
            Ok(a) => a,
            Err(_) => return Ok(false),
        };
        let signature = Signature::from_bytes(&sig_arr);
        let pk = self.public_key_bytes()?;
        let vk = VerifyingKey::from_bytes(&pk)
            .map_err(|e| anyhow!("invalid verifying key from address: {e}"))?;
        Ok(vk.verify(text.as_bytes(), &signature).is_ok())
    }

    pub fn account_balance(&self) -> Result<Option<f64>> {
        let client = match self.algod_client() {
            Ok(c) => c,
            Err(e) => {
                log::error!("[account_balance] Failed to access algod client: {}", e);
                return Err(e);
            }
        };
        let address = match self.require_address() {
            Ok(a) => a,
            Err(e) => {
                log::error!("[account_balance] Failed to resolve address: {}", e);
                return Err(e);
            }
        };
        let res = match self.rt_block_on(client.account_information(&address)) {
            Ok(r) => r,
            Err(e) => {
                log::error!("[account_balance] Failed to fetch account information for {}: {}", address, e);
                return Err(e);
            }
        };
        log::trace!("[account_balance] Retrieved account info for address: {} => {:?}", address, res);
        let info = match res {
            Ok(v) => v,
            Err(e) => {
                log::error!("[account_balance] Account information request failed for {}: {}", address, e);
                return Ok(None);
            }
        };
        // amount is in microalgos
        let micro: u64 = info.amount.0;
        log::debug!("[account_balance] Balance for address {} is {} microalgos", address, micro);
        Ok(Some(micro as f64 / 1_000_000.0))
    }

    pub fn global_state(&self, maybe_app_id: Option<u64>) -> Result<Option<Vec<(u64, Vec<(String, String)>)>>> {
        let client = self.algod_client()?;
        let address = self.require_address()?;
        let res = self.rt_block_on(client.account_information(&address))?;
        let info = match res { Ok(v) => v, Err(_e) => return Ok(None) };

        // Convert to json Value for resilient traversal
        let v = serde_json::to_value(&info)
            .map_err(|e| anyhow!("failed to serialize account info: {e}"))?;

        let created = match Self::json_get(&v, "created_apps", "created-apps").and_then(|x| x.as_array()) {
            Some(a) => a,
            None => return Ok(Some(vec![])),
        };

        let mut out: Vec<(u64, Vec<(String, String)>)> = Vec::new();
        for app in created {
            let id = match app.get("id").and_then(|x| x.as_u64()) { Some(i) => i, None => continue };
            if let Some(filter) = maybe_app_id { if filter != id { continue; } }
            let params = match app.get("params").and_then(|x| x.as_object()) { Some(p) => p, None => continue };
            let params_value = serde_json::Value::Object(params.clone());
            let gs_vec: Vec<serde_json::Value> = Self::json_get(&params_value, "global_state", "global-state")
                .and_then(|x| x.as_array())
                .map(|v| v.clone())
                .unwrap_or_default();
            let kvs = Self::decode_state_entries(&gs_vec);
            out.push((id, kvs));
        }
        Ok(Some(out))
    }

    pub fn local_state_for_account(&self, app_id: u64, account_address: &str) -> Result<Option<Vec<(String, String)>>> {
        let client = self.algod_client()?;
        let address = Self::parse_address(account_address)?;
        let res = self.rt_block_on(client.account_information(&address))?;
        let info = match res { Ok(v) => v, Err(_e) => return Ok(None) };

        log::debug!("Retrieved account info for address: {}", account_address);
        log::trace!("Account info: {:#?}", info);

        let v = serde_json::to_value(&info)
            .map_err(|e| anyhow!("failed to serialize account info: {e}"))?;

        let als = match Self::json_get(&v, "apps_local_state", "apps-local-state").and_then(|x| x.as_array()) {
            Some(a) => a,
            None => return Ok(None),
        };
        for st in als {
            let id = st.get("id").and_then(|x| x.as_u64());
            if id == Some(app_id) {
                let keyvals_vec: Vec<serde_json::Value> = Self::json_get(st, "key_value", "key-value")
                    .and_then(|x| x.as_array())
                    .map(|v| v.clone())
                    .unwrap_or_default();
                let out = Self::decode_state_entries(&keyvals_vec);
                return Ok(Some(out));
            }
        }
        Ok(None)
    }

    pub fn send_algo(&self, to_address: &str, amount_algos: f64) -> Result<()> {
        // Validate amount
        if amount_algos <= 0.0 {
            bail!("amount must be positive");
        }
        // Require account access (private key) and valid addresses
        let sk = self.private_key_bytes()?;
        let from = self.require_address()?;
        let to = Self::parse_address(to_address)?; // invalid -> early error

        // Algod client
        let client = self.algod_client()?;

        // Fetch suggested params
        let params_res = self.rt_block_on(client.suggested_transaction_params())?;
        let params = match params_res {
            Ok(p) => p,
            Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")),
        };

        // Build payment transaction
        let micro = (amount_algos * 1_000_000.0).round() as u64;
        let pay = algonaut::transaction::Pay::new(from, to, algonaut::core::MicroAlgos(micro)).build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, pay)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build transaction: {e}"))?;

        // Validate secret key length (32 bytes)
        let seed: [u8; 32] = sk
            .as_slice()
            .try_into()
            .map_err(|_| anyhow!("Secret key must be 32 bytes"))?;

        // Sign using algonaut account helper
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account
            .sign_transaction(tx)
            .map_err(|e| anyhow!("failed to sign transaction: {e}"))?;
        let signed = signed_tx
            .to_msg_pack()
            .map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;

        // Submit raw transaction via algod
        let send_res = self.rt_block_on(client.broadcast_raw_transaction(&signed))?;
        let tx_id = match send_res {
            Ok(r) => r.tx_id,
            Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")),
        };

        // Wait for confirmation up to a default timeout (e.g., 10 rounds)
        self.wait_for_confirmation(&tx_id, 10)?;
        Ok(())
    }

    pub fn create_asset(&self, name: &str, units_in_issue: u64) -> Result<Option<u64>> {
        // Validate inputs
        if name.trim().is_empty() { bail!("asset name must not be empty"); }
        if units_in_issue == 0 { bail!("units_in_issue must be > 0"); }

        // Require account access and issuer address
        let sk = self.private_key_bytes()?;
        let issuer = self.require_address()?;

        // Algod client and suggested params
        let client = self.algod_client()?;
        let params_res = self.rt_block_on(client.suggested_transaction_params())?;
        let params = match params_res { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        // Build CreateAsset transaction: minimal: total=units_in_issue, decimals=0
        // Set manager, reserve, and clawback to issuer so we can later reconfigure if needed
        let ca = algonaut::transaction::CreateAsset::new(issuer, units_in_issue, 0, false)
            .manager(issuer)
            .reserve(issuer)
            .clawback(issuer)
            .build();

        let tx = algonaut::transaction::TxnBuilder::with(&params, ca)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build asset create transaction: {e}"))?;

        // Sign
        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign asset create transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;

        // Submit and wait
        let send_res = self.rt_block_on(client.broadcast_raw_transaction(&signed))?;
        let tx_id = match send_res { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        self.wait_for_confirmation(&tx_id, 10)?;

        // Retrieve created asset id from pending transaction info
        let p = self.rt_block_on(client.pending_transaction_with_id(&tx_id))?;
        match p {
            Ok(info) => Ok(info.asset_index.or(Some(0)).filter(|id| *id != 0)),
            Err(e) => Err(anyhow!("failed to fetch pending transaction info for asset id: {e}")),
        }
    }

    /// Create an ASA with the reserve address set to the application address for `app_id`.
    /// Manager remains the issuer for future reconfiguration; decimals=0.
    pub fn create_asset_with_reserve_app(&self, name: &str, units_in_issue: u64, app_id: u64) -> Result<Option<u64>> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        if name.trim().is_empty() { bail!("asset name must not be empty"); }
        if units_in_issue == 0 { bail!("units_in_issue must be > 0"); }

        let sk = self.private_key_bytes()?;
        let issuer = self.require_address()?;
        let reserve_addr_str = self.contract_address(app_id)?;
        let reserve_addr = Self::parse_address(&reserve_addr_str)?;

        let client = self.algod_client()?;
        let params_res = self.rt_block_on(client.suggested_transaction_params())?;
        let params = match params_res { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        // Build CreateAsset with reserve and clawback set to app address
        let ca = algonaut::transaction::CreateAsset::new(issuer, units_in_issue, 0, false)
            .manager(issuer)
            .reserve(reserve_addr)
            .clawback(reserve_addr)
            .build();

        let tx = algonaut::transaction::TxnBuilder::with(&params, ca)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build asset create transaction: {e}"))?;

        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign asset create transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;

        let send_res = self.rt_block_on(client.broadcast_raw_transaction(&signed))?;
        let tx_id = match send_res { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        self.wait_for_confirmation(&tx_id, 10)?;

        let p = self.rt_block_on(client.pending_transaction_with_id(&tx_id))?;
        match p {
            Ok(info) => Ok(info.asset_index.or(Some(0)).filter(|id| *id != 0)),
            Err(e) => Err(anyhow!("failed to fetch pending transaction info for asset id: {e}")),
        }
    }

    /// Check whether `account_address` has opted-in to `asset_id`.
    pub fn is_account_opted_in_to_asset(&self, account_address: &str, asset_id: u64) -> Result<bool> {
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        let client = self.algod_client()?;
        let address = Self::parse_address(account_address)?;
        let res = self.rt_block_on(client.account_information(&address))?;
        let info = match res { Ok(v) => v, Err(_e) => return Ok(false) };
        let v = serde_json::to_value(&info).map_err(|e| anyhow!("failed to serialize account info: {e}"))?;
        let assets_arr = v.get("assets").or_else(|| v.get("assets")).and_then(|x| x.as_array()).cloned().unwrap_or_default();
        for a in assets_arr {
            let id = a.get("asset-id").and_then(|x| x.as_u64()).or_else(|| a.get("asset_id").and_then(|x| x.as_u64())).unwrap_or(0);
            if id == asset_id { return Ok(true); }
        }
        Ok(false)
    }


    pub fn set_asset_clawback_to_app(&self, app_id: u64, asset_id: u64) -> Result<()> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        // Manager must sign this transaction
        let sk = self.private_key_bytes()?;
        let manager = self.require_address()?;
        let clawback_str = self.contract_address(app_id)?;
        let clawback = Self::parse_address(&clawback_str)?;

        let client = self.algod_client()?;
        let params_res = self.rt_block_on(client.suggested_transaction_params())?;
        let params = match params_res { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        log::info!("Setting clawback to app address: {:?}", clawback);

        // Build asset reconfiguration: explicitly keep manager and reserve; set clawback to app address
        let cfg = algonaut::transaction::builder::UpdateAsset::new(manager, asset_id)
            .manager(manager)
            .reserve(clawback) // reserve is the app address in our model
            .clawback(clawback)
            .build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, cfg)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build asset config transaction: {e}"))?;

        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign asset config transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;

        let send_res = self.rt_block_on(client.broadcast_raw_transaction(&signed))?;
        let tx_id = match send_res { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        self.wait_for_confirmation(&tx_id, 10)
    }

    /// Change an ASA's reserve address using an AssetConfiguration (reconfigure) transaction.
    /// Requires the caller to be the asset creator (manager) to succeed on-chain.
    /// Additionally, after updating the reserve address, transfer the caller's current
    /// holding of the asset to the new reserve address (if any balance is held).
    pub fn change_asset_reserve_address(&self, asset_id: u64, reserve_address: &str) -> Result<()> {
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        // Parse target reserve address early
        let new_reserve = Self::parse_address(reserve_address)
            .map_err(|e| anyhow!("invalid reserve address: {e}"))?;

        // We must sign as the asset manager/creator
        let sk = self.private_key_bytes()?;
        let signer_addr = self.require_address()?;

        // Verify the caller is the asset creator by querying asset information
        let client = self.algod_client()?;
        let asset_info = match self.rt_block_on(client.asset_information(asset_id))? {
            Ok(info) => info,
            Err(e) => return Err(anyhow!("failed to fetch asset information: {e}")),
        };
        let v = serde_json::to_value(&asset_info)
            .map_err(|e| anyhow!("failed to serialize asset info: {e}"))?;
        // Try to read creator from params or top-level
        let creator_str = v
            .get("params").and_then(|p| p.get("creator").and_then(|x| x.as_str()))
            .or_else(|| v.get("creator").and_then(|x| x.as_str()))
            .or_else(|| v.get("params").and_then(|p| p.get("creator-address").or_else(|| p.get("creator_address")).and_then(|x| x.as_str())))
            .ok_or_else(|| anyhow!("asset info did not contain creator field"))?;
        let caller_str = {
            use std::str::FromStr;
            algonaut::core::Address::from_str(&self.address.as_ref().ok_or_else(|| anyhow!("This operation needs an address"))?)
                .map_err(|e| anyhow!("invalid caller address: {e}"))?
                .to_string()
        };
        if creator_str != caller_str {
            bail!("caller must be the asset creator to change the reserve address");
        }

        // Suggested params
        let params_res = self.rt_block_on(client.suggested_transaction_params())?;
        let params = match params_res { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        // Build UpdateAsset setting reserve and clawback to the same new address; keep manager unchanged (set to signer to be explicit)
        let cfg = algonaut::transaction::builder::UpdateAsset::new(signer_addr, asset_id)
            .manager(signer_addr)
            .reserve(new_reserve)
            .clawback(new_reserve)
            .build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, cfg)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build asset config transaction: {e}"))?;
        log::info!("[change_asset_reserve_address] tx: {:#?}", tx);

        // Sign and submit
        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign asset config transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;
        let send_res = self.rt_block_on(client.broadcast_raw_transaction(&signed))?;
        let tx_id = match send_res { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        // Wait for reconfiguration confirmation
        self.wait_for_confirmation(&tx_id, 10)?;

        // After reserve update, transfer creator's current holdings to the new reserve address
        // 1) Determine creator's balance of this asset
        let acct_info = match self.rt_block_on(client.account_information(&signer_addr))? { Ok(v) => v, Err(e) => return Err(anyhow!("failed to fetch creator account information: {e}")) };
        let va = serde_json::to_value(&acct_info).map_err(|e| anyhow!("failed to serialize creator account info: {e}"))?;
        let mut creator_amount: u64 = 0;
        if let Some(arr) = va.get("assets").and_then(|x| x.as_array()) {
            for holding in arr {
                let id = holding.get("asset-id").and_then(|x| x.as_u64()).or_else(|| holding.get("asset_id").and_then(|x| x.as_u64())).unwrap_or(0);
                if id == asset_id {
                    creator_amount = holding.get("amount").and_then(|x| x.as_u64()).unwrap_or(0);
                    break;
                }
            }
        }
        if creator_amount == 0 {
            log::info!("[change_asset_reserve_address] creator has no balance of asset {}", asset_id);
            return Ok(()); // nothing to move
        }

        // 2) Ensure the new reserve address is opted-in to receive the asset
        if !self.is_account_opted_in_to_asset(reserve_address, asset_id)? {
            bail!("new reserve address is not opted-in to asset {}", asset_id);
        }

        // 3) Transfer entire balance to the new reserve address
        self.send_asset(asset_id, creator_amount, reserve_address)
    }

    pub fn send_asset(&self, asset_id: u64, amount: u64, to_address: &str) -> Result<()> {
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        if amount == 0 { bail!("amount must be > 0"); }

        // Validate env and addresses
        let sk = self.private_key_bytes()?;
        let from = self.require_address()?;
        let to = Self::parse_address(to_address)?;

        let client = self.algod_client()?;
        let params_res = self.rt_block_on(client.suggested_transaction_params())?;
        let params = match params_res { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        // Build asset transfer transaction
        let xfer = algonaut::transaction::TransferAsset::new(from, asset_id, amount, to).build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, xfer)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build asset transfer transaction: {e}"))?;

        log::trace!("[send_asset] tx: {:#?}", tx);

        // Sign and submit
        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign asset transfer transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;

        let send_res = self.rt_block_on(client.broadcast_raw_transaction(&signed))?;
        let tx_id = match send_res { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };

        self.wait_for_confirmation(&tx_id, 10)
    }

    pub fn opt_in_to_asset(&self, asset_id: u64) -> Result<()> {
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        // Ensure we have account access
        let sk = self.private_key_bytes()?;
        let addr = self.require_address()?;

        let client = self.algod_client()?;
        let params_res = self.rt_block_on(client.suggested_transaction_params())?;
        let params = match params_res { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        // Opt-in is a zero-amount transfer from self to self
        let xfer = algonaut::transaction::TransferAsset::new(addr, asset_id, 0, addr).build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, xfer)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build asset opt-in transaction: {e}"))?;

        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign asset opt-in transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;

        let send_res = self.rt_block_on(client.broadcast_raw_transaction(&signed))?;
        let tx_id = match send_res { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };

        self.wait_for_confirmation(&tx_id, 10)
    }

    /// Extract (creator, reserve) addresses from an asset_information JSON value. Returns None if fields missing.
    pub fn parse_creator_reserve_from_asset_info_value(v: &serde_json::Value) -> Option<(String, String)> {
        let params = v.get("params");
        let creator = params.and_then(|p| p.get("creator").and_then(|x| x.as_str()))
            .or_else(|| v.get("creator").and_then(|x| x.as_str()))?;
        let reserve = params
            .and_then(|p| p.get("reserve").and_then(|x| x.as_str())
                .or_else(|| p.get("reserve-address").or_else(|| p.get("reserve_address")).and_then(|x| x.as_str())))?;
        Some((creator.to_string(), reserve.to_string()))
    }

    /// From an account_information JSON value, find the holding amount for the given asset id.
    pub fn parse_holding_amount_from_account_value(v: &serde_json::Value, asset_id: u64) -> u64 {
        if let Some(arr) = v.get("assets").and_then(|x| x.as_array()) {
            for holding in arr {
                let id = holding.get("asset-id").and_then(|x| x.as_u64())
                    .or_else(|| holding.get("asset_id").and_then(|x| x.as_u64()))
                    .unwrap_or(0);
                if id == asset_id {
                    return holding.get("amount").and_then(|x| x.as_u64()).unwrap_or(0);
                }
            }
        }
        0
    }

    /// Transfer the entire reserve balance of an ASA to the creator. The caller must control the reserve address.
    pub fn recover_reserve_balance(&self, asset_id: u64) -> Result<()> {
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        let client = self.algod_client()?;
        // Fetch asset info
        let asset_info = match self.rt_block_on(client.asset_information(asset_id))? { Ok(i) => i, Err(e) => return Err(anyhow!("failed to fetch asset information: {e}")) };
        let v = serde_json::to_value(&asset_info).map_err(|e| anyhow!("failed to serialize asset info: {e}"))?;
        let (creator_str, reserve_str) = match Self::parse_creator_reserve_from_asset_info_value(&v) {
            Some(t) => t,
            None => return Err(anyhow!("asset information did not contain creator/reserve fields")),
        };
        // Ensure we are the reserve account
        let caller_addr = self.address.as_ref().ok_or_else(|| anyhow!("This operation needs an address"))?.clone();
        if caller_addr != reserve_str {
            bail!("caller must be the current reserve address to recover the reserve balance");
        }
        // Ensure creator is opted-in to receive the asset
        if !self.is_account_opted_in_to_asset(&creator_str, asset_id)? {
            bail!("creator address is not opted-in to asset {asset_id}");
        }
        // Get reserve account holdings
        let reserve_addr = Self::parse_address(&reserve_str)?;
        let acct_info = match self.rt_block_on(client.account_information(&reserve_addr))? { Ok(v) => v, Err(e) => return Err(anyhow!("failed to fetch reserve account information: {e}")) };
        let va = serde_json::to_value(&acct_info).map_err(|e| anyhow!("failed to serialize reserve account info: {e}"))?;
        let amount = Self::parse_holding_amount_from_account_value(&va, asset_id);
        if amount == 0 {
            log::info!("[recover_reserve_balance] reserve has zero balance for asset {asset_id}");
            return Ok(());
        }
        // Transfer to creator
        self.send_asset(asset_id, amount, &creator_str)
    }

    pub fn compile_teal(&self, source: &str) -> Result<Vec<u8>> {
        if source.is_empty() {
            bail!("source must not be empty");
        }
        // If already provided as base64 compiled program, allow it for convenience.
        if let Some(rest) = source.strip_prefix("base64:") {
            let bytes = general_purpose::STANDARD
                .decode(rest)
                .map_err(|e| anyhow!("invalid base64 program: {e}"))?;
            return Ok(bytes);
        }
        let client = self.algod_client()?;
        let resp = self
            .rt_block_on(client.compile_teal(source.as_bytes()))?
            .map_err(|e| anyhow!("algod compile_teal failed: {e}"))?;
        // The compiled bytes are in the tuple struct directly
        Ok(resp.0)
    }

    // Compute 4-byte ARC-4 method selector from a signature string.
    pub fn arc4_selector(sig: &str) -> [u8; 4] {
        use sha2::{Digest, Sha512_256};
        let mut hasher = Sha512_256::new();
        hasher.update(sig.as_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        [digest[0], digest[1], digest[2], digest[3]]
    }

    #[inline]
    fn estimate_fee_for_programs(params: &algonaut::core::SuggestedTransactionParams, sizes: &[usize]) -> algonaut::core::MicroAlgos {
        let total_size: usize = sizes.iter().copied().sum();
        let per_byte = params.fee_per_byte; // MicroAlgos per byte
        let min_fee = params.min_fee; // MicroAlgos
        let sized = per_byte * (total_size as u64);
        if sized > min_fee { sized } else { min_fee }
    }

    #[inline]
    fn estimate_fee_for_signed_size(params: &algonaut::core::SuggestedTransactionParams, est_size: u64) -> algonaut::core::MicroAlgos {
        let per_byte = params.fee_per_byte; // MicroAlgos
        let min_fee = params.min_fee; // MicroAlgos
        let sized = per_byte * est_size;
        if sized > min_fee { sized } else { min_fee }
    }

    pub fn deploy_app(&self, approval_program: &[u8], clear_state_program: &[u8], asset_id: Option<u64>) -> Result<Option<u64>> {
        if approval_program.is_empty() { bail!("approval_program must not be empty"); }
        if clear_state_program.is_empty() { bail!("clear_state_program must not be empty"); }
        // Ensure we have account access
        let sk = self.private_key_bytes()?;
        let sender = self.require_address()?;

        // Build programs and schemas
        let approval = algonaut::core::CompiledTeal(approval_program.to_vec());
        let clear = algonaut::core::CompiledTeal(clear_state_program.to_vec());
        // Schema: Global needs at least 1 integer (e.g., BinglePrice).
        // Local needs at least 1 byteslice (Handle) and 1 integer (HandleTime).
        let gs = algonaut::transaction::transaction::StateSchema { number_ints: 1, number_byteslices: 0 };
        let ls = algonaut::transaction::transaction::StateSchema { number_ints: 3, number_byteslices: 2 };

        let client = self.algod_client()?;
        let params = match self.rt_block_on(client.suggested_transaction_params())? { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        // Before building/sending, estimate the cost of the CreateApplication and ensure we have funds
        // Estimate bytes using CompiledTeal.bytes_to_sign per requirement
        let est_prog_size: usize = approval.bytes_to_sign().len() + clear.bytes_to_sign().len();
        let est_fee = Self::estimate_fee_for_programs(&params, &[est_prog_size]);
        let est_fee_micro: u64 = est_fee.0; // microAlgos

        let balance_algos = self
            .account_balance()? // Always validate Option succeeds per guideline
            .ok_or_else(|| anyhow!("Unable to determine account balance from algod; the account may not be funded or node unreachable"))?;
        let balance_micro = (balance_algos * 1_000_000.0).floor() as u64;
        if balance_micro < est_fee_micro {
            let need_algos = est_fee_micro as f64 / 1_000_000.0;
            return Err(anyhow!(
                "Insufficient funds to create application: balance {:.6} ALGO, estimated required fee {:.6} ALGO. Please fund the creator account and retry.",
                balance_algos, need_algos
            ));
        }

        // Print estimated fee components and current balance for visibility
        let per_byte = params.fee_per_byte;
        let min_fee = params.min_fee;
        log::info!(
            "Preflight: min_fee={} µAlgos, fee_per_byte={} µAlgos/byte, est_prog_size={} bytes, estimated fee: {:.6} ALGO ({} µAlgos); current balance: {:.6} ALGO ({} µAlgos)",
            min_fee.0,
            per_byte.0,
            est_prog_size,
            est_fee_micro as f64 / 1_000_000.0,
            est_fee_micro,
            balance_algos,
            balance_micro
        );

        // Build create application transaction
        let mut builder = algonaut::transaction::CreateApplication::new(sender, approval, clear, gs, ls);
        if let Some(aid) = asset_id { builder = builder.foreign_assets(vec![aid]); }
        let create = builder.build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, create)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build app create transaction: {e}"))?;

        // Sign and submit
        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign app create transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;

        let tx_id = match self.rt_block_on(client.broadcast_raw_transaction(&signed))? { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };

        self.wait_for_confirmation(&tx_id, 10)?;

        // Retrieve created application id from pending tx info (robust JSON-based extraction)
        match self.rt_block_on(client.pending_transaction_with_id(&tx_id))? {
            Ok(info) => {
                let v = serde_json::to_value(&info).map_err(|e| anyhow!("failed to serialize pending tx info: {e}"))?;
                let app_id = v.get("application-index").and_then(|x| x.as_u64())
                    .or_else(|| v.get("application_index").and_then(|x| x.as_u64()))
                    .unwrap_or(0);

                if app_id != 0 {
                    // After successful creation, fund the app account with 3.21 ALGO from the creator
                    if let Ok(app_addr) = self.contract_address(app_id) {
                        // Best-effort funding; bubble up error if funding fails
                        self.send_algo(&app_addr, 3.21)?;
                    }
                    // If an asset id was provided, opt the app address in via the dApp admin method
                    if let Some(aid) = asset_id {
                        // Use AlgoBingle helper to call the admin method
                        let ab = crate::blockchain::algo_bingle::AlgoBingle::new(self.clone(), app_id, aid);
                        // Propagate any error from the admin call; ignore returned tx id
                        let _ = ab.opt_in_app_to_asset(app_id, aid)?;
                    }
                }
                Ok((app_id != 0).then_some(app_id))
            }
            Err(e) => Err(anyhow!("failed to fetch pending transaction info for app id: {e}")),
        }
    }

    pub fn update_app(&self, app_id: u64, approval_program: &[u8], clear_state_program: &[u8], asset_id: Option<u64>) -> Result<Option<u64>> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        if approval_program.is_empty() { bail!("approval_program must not be empty"); }
        if clear_state_program.is_empty() { bail!("clear_state_program must not be empty"); }
        let sk = self.private_key_bytes()?;
        let sender = self.require_address()?;

        let approval = algonaut::core::CompiledTeal(approval_program.to_vec());
        let clear = algonaut::core::CompiledTeal(clear_state_program.to_vec());

        let client = self.algod_client()?;
        let params = match self.rt_block_on(client.suggested_transaction_params())? { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        // Estimate fee using helper and params
        let est_prog_size: usize = approval.bytes_to_sign().len() + clear.bytes_to_sign().len();
        let est_fee = Self::estimate_fee_for_programs(&params, &[est_prog_size]);
        let per_byte = params.fee_per_byte;
        let min_fee = params.min_fee;
        log::info!(
            "Update preflight: min_fee={} µAlgos, fee_per_byte={} µAlgos/byte, est_prog_size={} bytes, estimated fee: {:.6} ALGO ({} µAlgos)",
            min_fee.0,
            per_byte.0,
            est_prog_size,
            est_fee.0 as f64 / 1_000_000.0,
            est_fee.0
        );

        // Note: Application state schemas (global/local) are immutable after creation on Algorand.
        // Do NOT attempt to change schemas in an update; only approval/clear programs (and other updatable
        // fields supported by the protocol) can be changed. Therefore, we build a plain UpdateApplication
        // with the new programs and do not set schemas here.
        let mut builder = algonaut::transaction::builder::UpdateApplication::new(sender, app_id, approval, clear);
        if let Some(aid) = asset_id { builder = builder.foreign_assets(vec![aid]); }
        let update = builder.build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, update)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build app update transaction: {e}"))?;

        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign app update transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;

        let tx_id = match self.rt_block_on(client.broadcast_raw_transaction(&signed))? { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        self.wait_for_confirmation(&tx_id, 10)?;
        Ok(Some(app_id))
    }

    pub fn call_app(&self, app_id: u64, asset_id: Option<u64>, method: Option<&str>, args: &[AppArg]) -> Result<(String, Vec<Vec<u8>>)> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        // Build tx
        let tx = self.build_call_app_tx(app_id, asset_id, method, args)?;
        log::info!("[call_app] method={:?} app_id={} asset_id={:?} args_len={}", method, app_id, asset_id, args.len());
        log::trace!("[call_app] tx={:?}", tx);
        let sk = self.private_key_bytes()?;
        let client = self.algod_client()?;
        // Sign, submit, wait
        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign app call transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;
        let tx_id = match self.rt_block_on(client.broadcast_raw_transaction(&signed))? { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        self.wait_for_confirmation(&tx_id, 10)?;
        // Fetch logs
        let p = match self.rt_block_on(client.pending_transaction_with_id(&tx_id))? { Ok(v) => v, Err(e) => return Err(anyhow!("failed to fetch pending transaction info: {e}")) };
        let v = serde_json::to_value(&p).map_err(|e| anyhow!("failed to serialize pending tx info: {e}"))?;
        let logs_arr = v.get("logs").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let mut logs: Vec<Vec<u8>> = Vec::new();
        for l in logs_arr { if let Some(s) = l.as_str() { if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(s) { logs.push(bytes); } } }
        Ok((tx_id, logs))
    }

    pub fn build_call_app_tx(&self, app_id: u64, asset_id: Option<u64>, method: Option<&str>, args: &[AppArg]) -> Result<algonaut::transaction::transaction::Transaction> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        // Lookup application information to determine the creator address (to include in foreign accounts)
        let client = self.algod_client()?;
        let app_info = match self.rt_block_on(client.application_information(app_id))? {
            Ok(info) => info,
            Err(e) => return Err(anyhow!("failed to fetch application information: {e}")),
        };
        let app_info_v = serde_json::to_value(&app_info).map_err(|e| anyhow!("failed to serialize application info: {e}"))?;
        let creator_str = app_info_v
            .get("params").and_then(|p| p.get("creator").and_then(|x| x.as_str()))
            .or_else(|| app_info_v.get("creator").and_then(|x| x.as_str()))
            .ok_or_else(|| anyhow!("application info did not contain creator field"))?;
        let creator = Self::parse_address(creator_str)?;
        let sender = self.require_address()?;

        // Prepare args (prepend ARC-4 selector if method provided)
        let mut app_args: Vec<Vec<u8>> = Vec::new();
        if let Some(sig) = method {
            let sel = Self::arc4_selector(sig);
            app_args.push(sel.to_vec());
        }
        app_args.extend(args.iter().map(|a| a.to_bytes()));

        let params = match self.rt_block_on(client.suggested_transaction_params())? { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        // Build application call (NoOp)
        // Always include creator in accounts for compatibility; for set_allow_static we also include the target address as Accounts[0]
        let mut accounts: Vec<algonaut::core::Address> = vec![creator];
        if let Some(sig) = method {
            if sig == "set_allow_static(address,uint64)void" {
                if let Some(first) = args.get(0) {
                    if let AppArg::Bytes(b) = first {
                        if b.len() == 32 {
                            let mut pk = [0u8; 32];
                            pk.copy_from_slice(&b[..32]);
                            if let Ok(addr_str) = crate::blockchain::algo_ops::byte_key_to_address(&pk) {
                                if let Ok(target) = Self::parse_address(&addr_str) {
                                    // Insert target at index 0 so Txn.Accounts[0] == target
                                    accounts.insert(0, target);
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut builder = algonaut::transaction::builder::CallApplication::new(sender, app_id)
            .accounts(accounts)
            .app_arguments(app_args);
        if let Some(aid) = asset_id { builder = builder.foreign_assets(vec![aid]); }
        let call = builder.build();
        // Explicitly estimate and set a fee for this transaction. Build a zero-fee txn to estimate size,
        // then compute fee using helper = max(min_fee, fee_per_byte * estimated_signed_tx_size).
        let tx_zero_fee = algonaut::transaction::TxnBuilder::with_fee(
            &params,
            algonaut::transaction::builder::TxnFee::zero(),
            call.clone(),
        )
        .note(Self::unique_note())
        .build()
        .map_err(|e| anyhow!("failed to build app call transaction for fee estimation: {e}"))?;
        let est_size = tx_zero_fee
            .estimate_basic_sig_size()
            .map_err(|e| anyhow!("failed to estimate signed tx size: {e}"))?;
        let est_fee = Self::estimate_fee_for_signed_size(&params, est_size);
        let tx = algonaut::transaction::TxnBuilder::with_fee(
            &params,
            algonaut::transaction::builder::TxnFee::Fixed(est_fee),
            call,
        )
        .note(Self::unique_note())
        .build()
        .map_err(|e| anyhow!("failed to build app call transaction: {e}"))?;
        Ok(tx)
    }

    pub fn opt_in_app(&self, app_id: u64) -> Result<()> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        // If already opted-in to local state for this app, nothing to do.
        // Always validate that Option succeeds per guidelines.
        let addr_str = self
            .address
            .as_ref()
            .ok_or_else(|| anyhow!("This operation needs an address"))?;
        if let Some(_entries) = self.local_state_for_account(app_id, addr_str)? {
            return Ok(());
        }

        let sender = self.require_address()?;
        let sk = self.private_key_bytes()?;

        let client = self.algod_client()?;
        let params = match self.rt_block_on(client.suggested_transaction_params())? { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        let call = algonaut::transaction::builder::OptInApplication::new(sender, app_id).build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, call)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build app opt-in transaction: {e}"))?;

        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign app opt-in transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;
        let tx_id = match self.rt_block_on(client.broadcast_raw_transaction(&signed))? { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        self.wait_for_confirmation(&tx_id, 10)
    }

    pub fn clear_state_app(&self, app_id: u64) -> Result<()> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        let sender = self.require_address()?;
        let sk = self.private_key_bytes()?;

        let client = self.algod_client()?;
        let params = match self.rt_block_on(client.suggested_transaction_params())? { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        let call = algonaut::transaction::builder::ClearApplication::new(sender, app_id).build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, call)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build app clear transaction: {e}"))?;

        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign app clear transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;
        let tx_id = match self.rt_block_on(client.broadcast_raw_transaction(&signed))? { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        self.wait_for_confirmation(&tx_id, 10)
    }

    pub fn close_out_app(&self, app_id: u64) -> Result<()> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        let sender = self.require_address()?;
        let sk = self.private_key_bytes()?;

        let client = self.algod_client()?;
        let params = match self.rt_block_on(client.suggested_transaction_params())? { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        let call = algonaut::transaction::builder::CloseApplication::new(sender, app_id).build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, call)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build app close-out transaction: {e}"))?;

        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign app close-out transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;
        let tx_id = match self.rt_block_on(client.broadcast_raw_transaction(&signed))? { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        self.wait_for_confirmation(&tx_id, 10)
    }

    pub fn delete_app(&self, app_id: u64) -> Result<()> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        let sender = self.require_address()?;
        let sk = self.private_key_bytes()?;

        let client = self.algod_client()?;
        let params = match self.rt_block_on(client.suggested_transaction_params())? { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        let call = algonaut::transaction::builder::DeleteApplication::new(sender, app_id).build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, call)
            .note(Self::unique_note())
            .build()
            .map_err(|e| anyhow!("failed to build app delete transaction: {e}"))?;

        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign app delete transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;
        let tx_id = match self.rt_block_on(client.broadcast_raw_transaction(&signed))? { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        self.wait_for_confirmation(&tx_id, 10)
    }

    /// Wait for a transaction to be confirmed up to `timeout_rounds` rounds.
    /// Follows the Kotlin logic: poll pending tx info and wait for next rounds.
    pub fn wait_for_confirmation(&self, tx_id: &str, timeout_rounds: u64) -> Result<()> {
        if timeout_rounds == 0 {
            bail!("timeout_rounds must be > 0");
        }
        let client = self.algod_client()?;

        // Get starting round
        let status = self
            .rt_block_on(client.status())?
            .map_err(|e| anyhow!("status query failed: {e}"))?;
        let start_round = status.last_round + 1;
        let end_round = start_round + timeout_rounds;

        let mut current_round = start_round;
        while current_round < end_round {
            // Check pending transaction info
            match self.rt_block_on(client.pending_transaction_with_id(tx_id))? {
                Ok(p) => {
                    if let Some(cr) = p.confirmed_round {
                        if cr > 0 {
                            return Ok(());
                        }
                    }
                    if !p.pool_error.is_empty() {
                        bail!("Transaction rejected with pool error: {}", p.pool_error);
                    }
                }
                Err(_e) => {
                    // If the node no longer remembers the tx or transient error, continue waiting until timeout
                }
            }

            // Wait for next round
            self
                .rt_block_on(client.status_after_round(algonaut::core::Round(current_round)))?
                .map_err(|e| anyhow!("status_after_round failed: {e}"))?;
            current_round += 1;
        }
        Err(anyhow!("Transaction not confirmed after {} rounds", timeout_rounds))
    }
}

/// Application argument type, similar to Kotlin variant handling.
#[derive(Debug, Clone)]
pub enum AppArg {
    Bytes(Vec<u8>),
    Utf8(String),
    Uint(u64),
}

impl AppArg {
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            AppArg::Bytes(b) => b.clone(),
            AppArg::Utf8(s) => s.as_bytes().to_vec(),
            AppArg::Uint(v) => v.to_be_bytes().to_vec(),
        }
    }
}

/// Decode first 32 bytes of an Algorand address' base32 to get the public key.
pub fn address_to_byte_key(address: &str) -> Result<[u8; 32]> {
    // Algorand addresses use RFC4648 base32 without padding and include checksum.
    let decoded = BASE32_NOPAD
        .decode(address.as_bytes())
        .map_err(|e| anyhow!("Invalid base32 address: {e}"))?;
    if decoded.len() < 32 {
        bail!("Decoded address is shorter than 32 bytes");
    }
    let mut pk = [0u8; 32];
    pk.copy_from_slice(&decoded[..32]);
    Ok(pk)
}

/// Build an Algorand address from a 32-byte public key.
pub fn byte_key_to_address(byte_public_key: &[u8; 32]) -> Result<String> {
    use sha2::{Digest, Sha512_256};
    // Compute checksum: SHA512/256 over public key, take last 4 bytes
    let mut hasher = Sha512_256::new();

    hasher.update(byte_public_key);
    let hash = hasher.finalize();
    let checksum = &hash[hash.len() - 4..];

    let mut addr_bytes = [0u8; 36];
    addr_bytes[..32].copy_from_slice(byte_public_key);
    addr_bytes[32..].copy_from_slice(checksum);

    let addr = BASE32_NOPAD.encode(&addr_bytes);
    Ok(addr)
}


// Shared helper: Convert an Algorand base32 address string into base64-encoded 36-byte form.
// Returns Err if the input is not valid base32 or does not decode to exactly 36 bytes.
pub fn id_b64_from_algorand_addr(id: &str) -> Result<String, String> {
    use base64::Engine as _;
    match BASE32_NOPAD.decode(id.as_bytes()) {
        Ok(bytes) if bytes.len() == 36 => Ok(base64::engine::general_purpose::STANDARD.encode(bytes)),
        Ok(bytes) => Err(format!("base32 decoded len {} != 36", bytes.len())),
        Err(e) => Err(format!("base32 decode failed: {}", e)),
    }
}
