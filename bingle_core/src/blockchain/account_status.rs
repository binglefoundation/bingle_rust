//! On-chain registration facts for one account: whether it holds Bingle$, its registered `Handle`,
//! and its balance against the funding needed to register (issue #388).
//!
//! [`resolve_account_status`](crate::blockchain::account_status::resolve_account_status) holds the checks behind the small [`AccountStatusReads`](crate::blockchain::account_status::AccountStatusReads) trait, which
//! [`AlgoBingle`](crate::blockchain::algo_bingle::AlgoBingle) implements for the account its `ops` is built for. bingle_local maps the
//! resulting [`AccountStatusFacts`](crate::blockchain::account_status::AccountStatusFacts) to the client-facing keypair status, for both the stored
//! keypair and a recovery phrase being inspected, so the two can't drift. A mock of the trait lets
//! the resolution be unit-tested without a live chain.

use anyhow::Result;

use crate::blockchain::algo_bingle::AlgoBingle;

/// The chain reads needed to resolve one account's status. The account is bound by the
/// implementation (its `AlgoOps` is built for that account), as are the app and asset ids.
pub trait AccountStatusReads {
    /// Whether `address` has opted in to the Bingle$ asset.
    fn is_opted_in_to_asset(&self, address: &str) -> Result<bool>;

    /// The `Handle` entry in `address`'s Bingle DApp local state, if any.
    fn handle(&self, address: &str) -> Result<Option<String>>;

    /// The account's Algo balance, `None` when the node has no record of it.
    fn balance_algos(&self) -> Result<Option<f64>>;

    /// The balance considered adequate to register, from the live Bingle$ price and local-schema
    /// minimum balance.
    fn required_funding_algos(&self) -> Result<f64>;
}

impl AccountStatusReads for AlgoBingle {
    fn is_opted_in_to_asset(&self, address: &str) -> Result<bool> {
        self.ops
            .is_account_opted_in_to_asset(address, self.asset_id)
    }

    fn handle(&self, address: &str) -> Result<Option<String>> {
        self.handle_for_address(self.app_id, address)
    }

    fn balance_algos(&self) -> Result<Option<f64>> {
        self.ops.account_balance()
    }

    fn required_funding_algos(&self) -> Result<f64> {
        self.required_funding()
    }
}

/// What the chain says about one account, for the caller to map to a status.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountStatusFacts {
    /// The account's address.
    pub address: String,
    /// Whether it has opted in to the Bingle$ asset.
    pub has_asset: bool,
    /// Its registered handle, read only when it holds Bingle$.
    pub handle: Option<String>,
    /// Its Algo balance (0 when unknown); not read, and 0, when the account is registered.
    pub balance_algos: f64,
    /// The balance needed to register: the live cost, or the caller's fallback when that read
    /// failed or the account is registered.
    pub required_funding_algos: f64,
}

impl AccountStatusFacts {
    /// Registered: it holds Bingle$ and has a `Handle` entry.
    pub fn is_registered(&self) -> bool {
        self.has_asset && self.handle.is_some()
    }
}

/// A chain read that failed while resolving a status: what was being read, and the error.
#[derive(Debug)]
pub struct StatusReadFailure {
    /// What was being read, for the log.
    pub context: String,
    /// The read's error.
    pub error: anyhow::Error,
}

/// Read `address`'s registration facts from chain. It is registered when it holds Bingle$ and has a
/// handle; otherwise its balance and the required funding are read for a funded / unfunded check.
///
/// An `asset_id` or `app_id` of 0 (unconfigured, e.g. tests) skips that read. A failed opt-in,
/// handle or balance read is returned as a [`StatusReadFailure`] for the caller to handle; a failed
/// required-funding read falls back to `fallback_required_funding_algos`, so a transient hiccup
/// never blocks the status (issue #15, A3b). Nothing is cached here.
pub fn resolve_account_status(
    reads: &dyn AccountStatusReads,
    address: String,
    app_id: u64,
    asset_id: u64,
    fallback_required_funding_algos: f64,
) -> std::result::Result<AccountStatusFacts, StatusReadFailure> {
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

    // Registration requires both the Bingle$ asset and a registered handle. If the account holds
    // the asset but has no Handle entry, fall through to the balance check.
    if has_asset && handle.is_none() {
        tracing::debug!(
            "[account_status] Account {} holds Bingle$ asset but no Handle entry found; falling through to balance check",
            address
        );
    }
    let registered = has_asset && handle.is_some();

    // Balance and the funding target are only needed when the account is not registered.
    let (balance_algos, required_funding_algos) = if registered {
        (0.0, fallback_required_funding_algos)
    } else {
        let balance = reads.balance_algos().map_err(|error| StatusReadFailure {
            context: "Failed to get account balance".to_string(),
            error,
        })?;
        let balance_algos = balance.unwrap_or(0.0);
        tracing::info!(
            "[account_status] Balance: {} ALGOs (raw: {:?})",
            balance_algos,
            balance
        );
        let target = match reads.required_funding_algos() {
            Ok(target) => {
                tracing::info!("[account_status] derived required funding {} ALGO", target);
                target
            }
            Err(e) => {
                tracing::warn!(
                    "[account_status] could not derive required funding ({}); falling back to {}",
                    e,
                    fallback_required_funding_algos
                );
                fallback_required_funding_algos
            }
        };
        (balance_algos, target)
    };

    Ok(AccountStatusFacts {
        address,
        has_asset,
        handle,
        balance_algos,
        required_funding_algos,
    })
}
