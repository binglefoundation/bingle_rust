use anyhow::{anyhow, bail, Result};
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
pub struct AlgoProviderConfig {
    pub client_api_url: String,
    pub client_api_port: u16,
    pub indexer_api_url: String,
    pub indexer_api_port: u16,
    pub token: Option<String>,
    pub token_key: Option<String>,
}

impl Default for AlgoProviderConfig {
    fn default() -> Self {
        // Defaults to localnet configuration matching the Kotlin implementation
        Self {
            client_api_url: "http://localhost".to_string(),
            client_api_port: 4001,
            indexer_api_url: "http://localhost".to_string(),
            indexer_api_port: 8980,
            token: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
            token_key: Some("X-Algo-API-Token".to_string()),
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
    pub config: AlgoProviderConfig,
}

impl AlgoOps {
    pub fn new(passphrase: Option<String>, address: Option<String>, config: Option<AlgoProviderConfig>) -> Self {
        // Use explicit config if provided; else Default (localnet)
        let config = config.unwrap_or_default();
        let ops = Self {
            passphrase,
            address,
            config,
        };

        if ops.address.is_none() {
            if ops.passphrase.is_some() {
                // Derive address from passphrase lazily later
            }
        }

        ops
    }

    fn algod_client(&self) -> Result<algonaut::algod::v2::Algod> {
        // Build base URL including port, e.g., http://localhost:4001
        let url = format!("{}:{}", self.config.client_api_url, self.config.client_api_port);
        let token = self.config.token.clone().unwrap_or_default();
        algonaut::algod::v2::Algod::new(&url, &token)
            .map_err(|e| anyhow!("failed to construct Algod client: {e}"))
    }

    // Helper: run an async future on a fresh current-thread Tokio runtime.
    fn rt_block_on<T>(&self, fut: impl Future<Output = T>) -> Result<T> {
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
        let client = self.algod_client()?;
        let address = self.require_address()?;
        let res = self.rt_block_on(client.account_information(&address))?;
        let info = match res {
            Ok(v) => v,
            Err(_e) => {
                // Mirror Kotlin: if not successful or not credited, return None
                return Ok(None);
            }
        };
        // amount is in microalgos
        let micro: u64 = info.amount.0;
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
        // Additional descriptive fields are optional; some SDKs allow setting them on the builder.
        // We keep it minimal to ensure compatibility across versions.
        let builder = algonaut::transaction::CreateAsset::new(issuer, units_in_issue, 0, false);
        let ca = builder.build();

        let tx = algonaut::transaction::TxnBuilder::with(&params, ca)
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
            .build()
            .map_err(|e| anyhow!("failed to build asset transfer transaction: {e}"))?;

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
        let ls = algonaut::transaction::transaction::StateSchema { number_ints: 1, number_byteslices: 1 };

        let client = self.algod_client()?;
        let params = match self.rt_block_on(client.suggested_transaction_params())? { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        // Build create application transaction
        let mut builder = algonaut::transaction::CreateApplication::new(sender, approval, clear, gs, ls);
        if let Some(aid) = asset_id { builder = builder.foreign_assets(vec![aid]); }
        let create = builder.build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, create)
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
                    // After successful creation, fund the app account with 321 ALGO from the creator
                    if let Ok(app_addr) = self.contract_address(app_id) {
                        // Best-effort funding; bubble up error if funding fails
                        self.send_algo(&app_addr, 321.0)?;
                    }
                    // If an asset id was provided, opt the app address in via the dApp admin method
                    if let Some(aid) = asset_id {
                        // Use AlgoBingle helper to call the admin method
                        let ab = crate::blockchain::algo_bingle::AlgoBingle::new(self.clone());
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

        let mut builder = algonaut::transaction::builder::UpdateApplication::new(sender, app_id, approval, clear);
        if let Some(aid) = asset_id { builder = builder.foreign_assets(vec![aid]); }
        let update = builder.build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, update)
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

    pub fn call_app(&self, app_id: u64, creator_address: &str, asset_id: Option<u64>, method: Option<&str>, args: &[AppArg]) -> Result<(String, Vec<Vec<u8>>)> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        let creator = Self::parse_address(creator_address)?;
        let sender = self.require_address()?;
        let sk = self.private_key_bytes()?;

        // Prepare args (prepend ARC-4 selector if method provided)
        let mut app_args: Vec<Vec<u8>> = Vec::new();
        if let Some(sig) = method {
            use sha2::{Digest, Sha512_256};
            let mut hasher = Sha512_256::new();
            hasher.update(sig.as_bytes());
            let digest: [u8; 32] = hasher.finalize().into();
            app_args.push(vec![digest[0], digest[1], digest[2], digest[3]]);
        }
        app_args.extend(args.iter().map(|a| a.to_bytes()));

        let client = self.algod_client()?;
        let params = match self.rt_block_on(client.suggested_transaction_params())? { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        // Build application call (NoOp)
        let mut builder = algonaut::transaction::builder::CallApplication::new(sender, app_id)
            .accounts(vec![creator])
            .app_arguments(app_args);
        if let Some(aid) = asset_id { builder = builder.foreign_assets(vec![aid]); }
        let call = builder.build();
        // Explicitly estimate and set a fee for this transaction. Build a zero-fee txn to estimate size,
        // then compute fee = max(min_fee, fee_per_byte * estimated_signed_tx_size).
        let tx_zero_fee = algonaut::transaction::TxnBuilder::with_fee(
            &params,
            algonaut::transaction::builder::TxnFee::zero(),
            call.clone(),
        )
        .build()
        .map_err(|e| anyhow!("failed to build app call transaction for fee estimation: {e}"))?;
        let est_size = tx_zero_fee
            .estimate_basic_sig_size()
            .map_err(|e| anyhow!("failed to estimate signed tx size: {e}"))?;
        let est_fee = {
            let per_byte = params.fee_per_byte; // MicroAlgos
            let min_fee = params.min_fee; // MicroAlgos
            let sized = per_byte * est_size;
            if sized > min_fee { sized } else { min_fee }
        };
        let tx = algonaut::transaction::TxnBuilder::with_fee(
            &params,
            algonaut::transaction::builder::TxnFee::Fixed(est_fee),
            call,
        )
        .build()
        .map_err(|e| anyhow!("failed to build app call transaction: {e}"))?;

        // Sign, submit, wait
        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = algonaut::transaction::account::Account::from_seed(seed);
        let signed_tx = account.sign_transaction(tx).map_err(|e| anyhow!("failed to sign app call transaction: {e}"))?;
        let signed = signed_tx.to_msg_pack().map_err(|e| anyhow!("failed to encode signed transaction: {e}"))?;
        let tx_id = match self.rt_block_on(client.broadcast_raw_transaction(&signed))? { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        self.wait_for_confirmation(&tx_id, 10)?;

        // Fetch logs from pending tx info and decode base64
        let p = match self.rt_block_on(client.pending_transaction_with_id(&tx_id))? { Ok(v) => v, Err(e) => return Err(anyhow!("failed to fetch pending transaction info: {e}")) };
        let v = serde_json::to_value(&p).map_err(|e| anyhow!("failed to serialize pending tx info: {e}"))?;
        let logs_arr = v.get("logs").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let mut logs: Vec<Vec<u8>> = Vec::new();
        for l in logs_arr {
            if let Some(s) = l.as_str() {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(s) {
                    logs.push(bytes);
                }
            }
        }
        Ok((tx_id, logs))
    }

    pub fn opt_in_app(&self, app_id: u64) -> Result<()> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        let sender = self.require_address()?;
        let sk = self.private_key_bytes()?;

        let client = self.algod_client()?;
        let params = match self.rt_block_on(client.suggested_transaction_params())? { Ok(p) => p, Err(e) => return Err(anyhow!("failed to fetch suggested params: {e}")) };

        let call = algonaut::transaction::builder::OptInApplication::new(sender, app_id).build();
        let tx = algonaut::transaction::TxnBuilder::with(&params, call)
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
