//! Chain-agnostic blockchain operations.
//!
//! Stage 1 of issue #158: these traits are introduced alongside the existing `AlgoOps`
//! inherent methods without changing any current behavior or call site. `AlgoOps` implements
//! them by delegating to its inherent methods, so both surfaces coexist. Later stages migrate
//! callers onto the traits, reduce the visibility of the direct constructor, and extract the
//! traits into a standalone `blockchain_ops` crate.
//!
//! The design intent (see `spec/algo_ops_api_surface.md`): the consumer is blockchain-aware,
//! so these traits cover the operations that read naturally on any chain, while chain-specific
//! power (application calls, asset configuration) stays on the concrete implementation and is
//! reached directly or through a native escape hatch.

use crate::blockchain::algo_ops::{AlgoChainConfig, AlgoOps};
use anyhow::Result;

/// Operations meaningful against any blockchain.
///
/// Amounts and balances are expressed as `f64` in units of the chain's default asset (for
/// Algorand, whole ALGO), rather than the chain's smallest indivisible unit.
pub trait BlockChainOps {
    /// Generate a new keypair, returning `(address, private-key mnemonic)`.
    fn generate_keypair() -> (String, String)
    where
        Self: Sized;

    /// Create a new address on this instance and return it.
    fn create_address(&mut self) -> Result<String>;

    /// This account's own address.
    fn address(&self) -> Result<String>;

    /// This account's public key bytes.
    fn public_key(&self) -> Result<[u8; 32]>;

    /// Export this account's private key bytes.
    fn private_key(&self) -> Result<Vec<u8>>;

    /// Sign UTF-8 text, returning a base64 signature.
    fn sign(&self, text: &str) -> Result<String>;

    /// Verify a base64 signature over UTF-8 text.
    fn verify(&self, text: &str, sig_b64: &str) -> Result<bool>;

    /// Send a native-token payment to `to`, in default-asset units.
    fn send_payment(&self, to: &str, payment_amount: f64) -> Result<()>;

    /// Balance of this account, in default-asset units.
    fn account_balance(&self) -> Result<Option<f64>>;

    /// Balance of an arbitrary account, in default-asset units.
    fn account_balance_at(&self, account: &str) -> Result<f64>;

    /// Wait for a transaction to confirm, giving up after `timeout` units (chain-defined; for
    /// Algorand, rounds).
    fn wait_for_confirmation(&self, tx_id: &str, timeout: u64) -> Result<()>;
}

/// Operations for chains that have first-class assets (Algorand Standard Assets, Ethereum
/// tokens, etc.). Implemented only by chains that support assets.
pub trait AssetOps {
    /// Transfer `amount` indivisible units of asset `asset_id` to `to`.
    fn send_asset(&self, asset_id: u64, amount: u64, to: &str) -> Result<()>;

    /// Holding of asset `asset_id` for `account`, in indivisible units.
    fn asset_holding(&self, account: &str, asset_id: u64) -> Result<u64>;
}

/// Number of micro-units in one whole ALGO.
const MICRO_PER_ALGO: f64 = 1_000_000.0;

impl BlockChainOps for AlgoOps {
    fn generate_keypair() -> (String, String) {
        AlgoOps::generate_keypair()
    }

    fn create_address(&mut self) -> Result<String> {
        AlgoOps::create_address(self)
    }

    fn address(&self) -> Result<String> {
        AlgoOps::address_str(self)
    }

    fn public_key(&self) -> Result<[u8; 32]> {
        AlgoOps::public_key_bytes(self)
    }

    fn private_key(&self) -> Result<Vec<u8>> {
        AlgoOps::private_key_bytes(self)
    }

    fn sign(&self, text: &str) -> Result<String> {
        AlgoOps::sign(self, text)
    }

    fn verify(&self, text: &str, sig_b64: &str) -> Result<bool> {
        AlgoOps::verify(self, text, sig_b64)
    }

    fn send_payment(&self, to: &str, payment_amount: f64) -> Result<()> {
        AlgoOps::send_algo(self, to, payment_amount)
    }

    fn account_balance(&self) -> Result<Option<f64>> {
        AlgoOps::account_balance(self)
    }

    fn account_balance_at(&self, account: &str) -> Result<f64> {
        // The inherent accessor returns micro-ALGO; express it in whole ALGO for the trait.
        Ok(AlgoOps::microalgos_at(self, account)? as f64 / MICRO_PER_ALGO)
    }

    fn wait_for_confirmation(&self, tx_id: &str, timeout: u64) -> Result<()> {
        AlgoOps::wait_for_confirmation(self, tx_id, timeout)
    }
}

impl AssetOps for AlgoOps {
    fn send_asset(&self, asset_id: u64, amount: u64, to: &str) -> Result<()> {
        AlgoOps::send_asset(self, asset_id, amount, to)
    }

    fn asset_holding(&self, account: &str, asset_id: u64) -> Result<u64> {
        AlgoOps::asset_holding(self, account, asset_id)
    }
}

impl AlgoOps {
    /// Factory constructor — the intended replacement for calling [`AlgoOps::new`] directly.
    ///
    /// Kept alongside `new` during stage 1; a later stage reduces `new`'s visibility so that
    /// construction goes through this factory (and, eventually, a chain-specific factory trait).
    pub fn factory(
        passphrase: Option<String>,
        address: Option<String>,
        config: Option<AlgoChainConfig>,
    ) -> Self {
        AlgoOps::new(passphrase, address, config)
    }
}
