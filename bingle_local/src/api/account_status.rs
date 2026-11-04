//! On-chain account status resolution, shared by `keypair_status` and `inspect_passphrase`
//! (issue #388).
//!
//! [`resolve_account_status`] holds the registration checks (Bingle$ opt-in, `Handle` local state,
//! balance against the required funding) behind the small [`AccountStatusReads`] trait, so the
//! stored-keypair status and the status of a phrase being inspected can't drift, and the
//! resolution can be unit-tested with a mock instead of a live chain.

use algo_ops::AlgoOps;
use bingle_core::api::bingle_api::BingleError;
use bingle_core::blockchain::algo_bingle::AlgoBingle;

use crate::api::bingle_local_api::{KeypairStatus, REQUIRED_ALGO};
use crate::api::bingle_local_api_impl::keypair_status_from_facts;

/// The chain reads needed to resolve one account's status. The account is bound by the
/// implementation (its `AlgoOps` is built for that account), as are the app and asset ids.
pub trait AccountStatusReads {
    /// Whether `address` has opted in to the Bingle$ asset.
    fn is_opted_in_to_asset(&self, address: &str) -> Result<bool, BingleError>;

    /// The `Handle` entry in `address`'s Bingle DApp local state, if any.
    fn handle(&self, address: &str) -> Result<Option<String>, BingleError>;

    /// The account's Algo balance, `None` when the node has no record of it.
    fn balance_algos(&self) -> Result<Option<f64>, BingleError>;

    /// The balance considered adequate to register, from the live Bingle$ price and local-schema
    /// minimum balance.
    fn required_funding_algos(&self) -> Result<f64, BingleError>;
}

/// A chain read that failed while resolving a status: what was being read, and the error.
#[derive(Debug)]
pub struct StatusReadFailure {
    /// What was being read, for the log.
    pub context: String,
    /// The read's error.
    pub error: BingleError,
}

/// [`AccountStatusReads`] against the configured Algorand node, for the account `ops` is built for.
pub struct ChainAccountStatusReads {
    ops: AlgoOps,
    app_id: u64,
    asset_id: u64,
}

impl ChainAccountStatusReads {
    /// Reads for the account `ops` is built for, against the Bingle DApp `app_id` and Bingle$
    /// asset `asset_id`.
    pub fn new(ops: AlgoOps, app_id: u64, asset_id: u64) -> Self {
        ChainAccountStatusReads {
            ops,
            app_id,
            asset_id,
        }
    }
}

impl AccountStatusReads for ChainAccountStatusReads {
    fn is_opted_in_to_asset(&self, address: &str) -> Result<bool, BingleError> {
        self.ops
            .is_account_opted_in_to_asset(address, self.asset_id)
            .map_err(BingleError::from_anyhow)
    }

    fn handle(&self, address: &str) -> Result<Option<String>, BingleError> {
        let local_state = self
            .ops
            .local_state_for_account(self.app_id, address)
            .map_err(BingleError::from_anyhow)?;
        Ok(local_state.and_then(|entries| {
            entries
                .into_iter()
                .find(|(k, _)| k == "Handle")
                .map(|(_, v)| v)
        }))
    }

    fn balance_algos(&self) -> Result<Option<f64>, BingleError> {
        self.ops.account_balance().map_err(BingleError::from_anyhow)
    }

    fn required_funding_algos(&self) -> Result<f64, BingleError> {
        AlgoBingle::new(self.ops.clone(), self.app_id, self.asset_id)
            .required_funding()
            .map_err(BingleError::from_anyhow)
    }
}

/// Resolve `address`'s status from chain: `ACTIVE` when it holds Bingle$ and has a registered
/// handle, otherwise `FUNDED` or `UNFUNDED` by its balance against the required funding.
///
/// An `asset_id` or `app_id` of 0 (unconfigured, e.g. tests) skips that read. A failed opt-in,
/// handle or balance read is returned as a [`StatusReadFailure`] for the caller to handle; a failed
/// required-funding read falls back to [`REQUIRED_ALGO`], so a transient hiccup never blocks the
/// status (issue #15, A3b). Nothing is cached here.
pub fn resolve_account_status(
    reads: &dyn AccountStatusReads,
    address: String,
    app_id: u64,
    asset_id: u64,
) -> Result<KeypairStatus, StatusReadFailure> {
    let has_asset = if asset_id > 0 {
        reads
            .is_opted_in_to_asset(&address)
            .map_err(|error| StatusReadFailure {
                context: format!(
                    "Failed to check asset opt-in for {} (asset {})",
                    address, asset_id
                ),
                error,
            })?
    } else {
        false
    };

    // If the account holds the Bingle$ asset, look up its handle from on-chain local state
    let handle = if has_asset && app_id > 0 {
        reads.handle(&address).map_err(|error| StatusReadFailure {
            context: format!("Failed to get local state for {} (app {})", address, app_id),
            error,
        })?
    } else {
        None
    };

    // ACTIVE requires both the Bingle$ asset and a registered handle. If the account holds the
    // asset but has no Handle entry, fall through to the balance check rather than reporting ACTIVE.
    if has_asset && handle.is_none() {
        tracing::debug!(
            "[BingleLocalApi][keypair_status] Account {} holds Bingle$ asset but no Handle entry found; falling through to balance check",
            address
        );
    }
    let active = has_asset && handle.is_some();

    // Balance and the funding target are only needed when the account is not ACTIVE.
    let (balance_algos, required_target_algos) = if active {
        (0.0, REQUIRED_ALGO)
    } else {
        let balance = reads.balance_algos().map_err(|error| StatusReadFailure {
            context: "Failed to get account balance".to_string(),
            error,
        })?;
        let balance_algos = balance.unwrap_or(0.0);
        tracing::info!(
            "[BingleLocalApi][keypair_status] Balance: {} ALGOs (raw: {:?})",
            balance_algos,
            balance
        );
        let target = match reads.required_funding_algos() {
            Ok(target) => {
                tracing::info!(
                    "[BingleLocalApi][keypair_status] derived required funding {} ALGO",
                    target
                );
                target
            }
            Err(e) => {
                tracing::warn!(
                    "[BingleLocalApi][keypair_status] could not derive required funding ({}); falling back to REQUIRED_ALGO {}",
                    e,
                    REQUIRED_ALGO
                );
                REQUIRED_ALGO
            }
        };
        (balance_algos, target)
    };

    Ok(keypair_status_from_facts(
        address,
        has_asset,
        handle,
        balance_algos,
        required_target_algos,
    ))
}
