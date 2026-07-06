use base64::{Engine as _, engine::general_purpose};
use data_encoding::BASE32_NOPAD;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha512_256};

use crate::tests_common::TestAlgo;

#[derive(Clone, Default)]
pub struct SimpleBackend {
    passphrase: Option<String>,
    address: Option<String>,
    // use_dummy_config is ignored; network ops are simulated regardless
}

impl SimpleBackend {
    fn addr_from_pk_impl(byte_public_key: &[u8; 32]) -> String {
        let mut hasher = Sha512_256::new();
        hasher.update(byte_public_key);
        let hash = hasher.finalize();
        let checksum = &hash[hash.len() - 4..];
        let mut addr_bytes = [0u8; 36];
        addr_bytes[..32].copy_from_slice(byte_public_key);
        addr_bytes[32..].copy_from_slice(checksum);
        BASE32_NOPAD.encode(&addr_bytes)
    }

    fn pk_from_addr_impl(address: &str) -> Result<[u8; 32], String> {
        let decoded = BASE32_NOPAD
            .decode(address.as_bytes())
            .map_err(|e| format!("Invalid base32 address: {e}"))?;
        if decoded.len() < 32 {
            return Err("Decoded address is shorter than 32 bytes".to_string());
        }
        let mut pk = [0u8; 32];
        pk.copy_from_slice(&decoded[..32]);
        Ok(pk)
    }

    fn require_address(&self) -> Result<String, String> {
        self.address
            .clone()
            .ok_or_else(|| "This operation needs an address".to_string())
    }

    fn private_key_bytes_impl(&self) -> Result<Vec<u8>, String> {
        let pass = self
            .passphrase
            .as_ref()
            .ok_or_else(|| "This operation needs account access".to_string())?;
        if let Some(rest) = pass.strip_prefix("b64:") {
            let sk = general_purpose::STANDARD
                .decode(rest)
                .map_err(|e| format!("Invalid base64 secret: {e}"))?;
            if sk.len() != 32 {
                return Err(format!("Secret key must be 32 bytes, got {}", sk.len()));
            }
            Ok(sk)
        } else {
            Err("Unsupported passphrase format".to_string())
        }
    }
}

impl TestAlgo for SimpleBackend {
    fn new(passphrase: Option<String>, address: Option<String>, _use_dummy_config: bool) -> Self {
        Self {
            passphrase,
            address,
        }
    }

    fn create_address(&mut self, _save: bool, _always_new_address: bool) -> Result<String, String> {
        // Deterministic secret key (no OsRng used, iOS-safe)
        let mut sk: [u8; 32] = [0u8; 32];
        for i in 0..32 {
            sk[i] = i as u8;
        }
        let signing_key = SigningKey::from_bytes(&sk);
        let verifying_key = signing_key.verifying_key();
        let pk: [u8; 32] = verifying_key.to_bytes();
        let addr = Self::addr_from_pk_impl(&pk);
        self.address = Some(addr.clone());
        let sk_b64 = general_purpose::STANDARD.encode(sk);
        self.passphrase = Some(format!("b64:{}", sk_b64));
        Ok(addr)
    }

    fn public_key_bytes(&self) -> Result<[u8; 32], String> {
        let addr = self.require_address()?;
        Self::pk_from_addr_impl(&addr)
    }

    fn private_key_bytes(&self) -> Result<Vec<u8>, String> {
        self.private_key_bytes_impl()
    }

    fn contract_address(&self, app_id: u64) -> Result<String, String> {
        let mut hasher = Sha512_256::new();
        hasher.update(b"appID");
        hasher.update(app_id.to_be_bytes());
        let pk_bytes: [u8; 32] = hasher.finalize().into();
        Ok(Self::addr_from_pk_impl(&pk_bytes))
    }

    fn sign(&self, text: &str) -> Result<String, String> {
        let sk_bytes = self.private_key_bytes_impl()?;
        let sk_arr: [u8; 32] = sk_bytes
            .try_into()
            .map_err(|_| "Secret key must be 32 bytes".to_string())?;
        let signing_key = SigningKey::from_bytes(&sk_arr);
        let signature = signing_key.sign(text.as_bytes());
        Ok(general_purpose::STANDARD.encode(signature.to_bytes()))
    }

    fn verify(&self, text: &str, sig_b64: &str) -> Result<bool, String> {
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
            .map_err(|e| format!("invalid verifying key from address: {e}"))?;
        Ok(vk.verify(text.as_bytes(), &signature).is_ok())
    }

    fn global_state(
        &self,
        _maybe_app_id: Option<u64>,
    ) -> Result<Option<Vec<(u64, Vec<(String, String)>)>>, String> {
        // Require address, else error; simulate unreachable network => None
        let _ = self.require_address()?;
        Ok(None)
    }

    fn local_state_for_account(
        &self,
        _app_id: u64,
        account_address: &str,
    ) -> Result<Option<Vec<(String, String)>>, String> {
        // Validate address format, simulate unreachable network
        let _ = Self::pk_from_addr_impl(account_address)?;
        Ok(None)
    }

    fn send_algo(&self, to_address: &str, amount_algos: f64) -> Result<(), String> {
        if self.passphrase.is_none() {
            return Err("This operation needs account access".to_string());
        }
        if amount_algos <= 0.0 {
            return Err("amount must be positive".to_string());
        }
        // Validate receiver address
        let _ = Self::pk_from_addr_impl(to_address)
            .map_err(|_| "invalid receiver address".to_string())?;
        Err("network error".to_string())
    }

    fn create_asset(&self, name: &str, units_in_issue: u64) -> Result<Option<u64>, String> {
        if self.passphrase.is_none() {
            return Err("This operation needs account access".to_string());
        }
        if name.is_empty() {
            return Err("asset name must not be empty".to_string());
        }
        if units_in_issue == 0 {
            return Err("units_in_issue must be > 0".to_string());
        }
        Ok(None)
    }

    fn opt_in_to_asset(&self, _asset_id: u64) -> Result<(), String> {
        if self.passphrase.is_none() {
            return Err("This operation needs account access".to_string());
        }
        Err("network error".to_string())
    }

    fn addr_from_pk(pk: &[u8; 32]) -> Result<String, String> {
        Ok(Self::addr_from_pk_impl(pk))
    }

    fn pk_from_addr(addr: &str) -> Result<[u8; 32], String> {
        Self::pk_from_addr_impl(addr)
    }
}
