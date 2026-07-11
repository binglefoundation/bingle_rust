//! Test seam for on-chain handle registration (issue #15, step A4).
//!
//! `register_keypair` used to drive `AlgoOps`/`AlgoBingle` directly, so the ordering of
//! the on-chain steps (opt-ins, buy, register) could only be exercised against a live
//! localnet. This module extracts those steps behind the [`RegistrationOps`] trait and an
//! [`run_registration`] orchestration function so the sequence can be unit-tested with a
//! recording mock.
//!
//! A4 preserves the existing behaviour exactly. The fail-fast handle pre-check (A1) and
//! idempotent opt-ins (A2) build on this seam in later changes.

use bingle_core::api::bingle_api::BingleError;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::blockchain::algo_ops::AlgoOps;

/// The minimal set of on-chain operations needed to register a handle.
///
/// Kept deliberately small and account-scoped (`app_id`/`asset_id` are bound by the
/// implementation) so a mock can record call order and inject failures without a chain.
pub trait RegistrationOps {
    /// The signer's own account address.
    fn self_address(&self) -> Result<String, BingleError>;

    /// Indexer lookup of the account currently owning `handle`, if any. Used by the
    /// fail-fast pre-check (A1); present on the seam from A4 so the trait is stable.
    fn handle_lookup(&self, handle: &str) -> Result<Option<String>, BingleError>;

    /// Opt the account in to the Bingle app.
    fn opt_in_app(&self) -> Result<(), BingleError>;

    /// Opt the account in to the Bingle$ asset.
    fn opt_in_to_asset(&self) -> Result<(), BingleError>;

    /// Current Bingle$ price, in microalgos.
    fn get_bingle_price(&self) -> Result<u64, BingleError>;

    /// Buy one Bingle$ unit at `price_microalgos`.
    fn buy_bingle(&self, price_microalgos: u64) -> Result<(), BingleError>;

    /// Register `handle` on-chain for the account.
    fn register(&self, handle: &str) -> Result<(), BingleError>;
}

/// Orchestrate the on-chain registration sequence.
///
/// A4 preserves the historical order exactly: opt in to the app and asset, read the
/// price, buy one unit, then register. Each step short-circuits on error so a later step
/// never runs after an earlier one fails.
pub fn run_registration(ops: &dyn RegistrationOps, handle: &str) -> Result<(), BingleError> {
    ops.opt_in_app().inspect_err(|e| {
        tracing::error!("[register] opt_in_app failed: {}", e);
    })?;
    ops.opt_in_to_asset().inspect_err(|e| {
        tracing::error!("[register] opt_in_to_asset failed: {}", e);
    })?;
    let price = ops.get_bingle_price().inspect_err(|e| {
        tracing::error!("[register] get_bingle_price failed: {}", e);
    })?;
    ops.buy_bingle(price).inspect_err(|e| {
        tracing::error!("[register] buy_bingle failed (price={}): {}", price, e);
    })?;
    ops.register(handle).inspect_err(|e| {
        // A registration failure is usually a user-chosen duplicate handle, not a fault.
        tracing::info!("[register] register handle '{}' failed: {}", handle, e);
    })?;
    Ok(())
}

/// Concrete [`RegistrationOps`] backed by [`AlgoOps`] + [`AlgoBingle`].
pub struct ChainRegistrationOps {
    ops: AlgoOps,
    bgl: AlgoBingle,
    app_id: u64,
    asset_id: u64,
}

impl ChainRegistrationOps {
    /// Build the on-chain ops bound to `app_id`/`asset_id`. Mirrors the previous inline
    /// `AlgoBingle::new(ops.clone(), ...)` construction in `register_keypair`.
    pub fn new(ops: AlgoOps, app_id: u64, asset_id: u64) -> Self {
        let bgl = AlgoBingle::new(ops.clone(), app_id, asset_id);
        Self {
            ops,
            bgl,
            app_id,
            asset_id,
        }
    }
}

impl RegistrationOps for ChainRegistrationOps {
    fn self_address(&self) -> Result<String, BingleError> {
        self.ops.address_str().map_err(BingleError::from_anyhow)
    }

    fn handle_lookup(&self, handle: &str) -> Result<Option<String>, BingleError> {
        self.bgl.handle_lookup(handle).map_err(BingleError::from_anyhow)
    }

    fn opt_in_app(&self) -> Result<(), BingleError> {
        self.ops
            .opt_in_app(self.app_id)
            .map_err(BingleError::from_anyhow)
    }

    fn opt_in_to_asset(&self) -> Result<(), BingleError> {
        self.ops
            .opt_in_to_asset(self.asset_id)
            .map_err(BingleError::from_anyhow)
    }

    fn get_bingle_price(&self) -> Result<u64, BingleError> {
        self.bgl
            .get_bingle_price(self.app_id)
            .map_err(BingleError::from_anyhow)
    }

    fn buy_bingle(&self, price_microalgos: u64) -> Result<(), BingleError> {
        self.bgl
            .buy_bingle(self.app_id, self.asset_id, price_microalgos)
            .map(|_tx| ())
            .map_err(BingleError::from_anyhow)
    }

    fn register(&self, handle: &str) -> Result<(), BingleError> {
        self.bgl
            .register(self.app_id, self.asset_id, handle, 1)
            .map(|_tx| ())
            .map_err(BingleError::from_anyhow)
    }
}
