//! Account deletion (issue #100): opt the account out of the Bingle DApp, then clear this device.
//!
//! What deletion removes and what stays:
//!
//! - **Removed on chain:** the account's local state in the Bingle DApp, on the configured app and
//!   on every blessed ancestor app, through clear-state. That holds the handle and the endpoint
//!   records, so the handle is free for anyone to register at once, and it can't be migrated back.
//! - **Removed on this device:** the keypair and any pending handle clash recovery keypair, their
//!   phrases in the key store (on iOS, the `key/<scope>` and `recovery/<scope>` Keychain items; a
//!   synced item also goes from the user's other devices), and the contacts, blocks, messages and
//!   the memoized handle in memory. The caller deletes the state file.
//! - **Stays:** the Algorand account itself, with its ALGO and its Bingle$ holding (the
//!   registration fee asset isn't opted out or transferred), and the blockchain's history of past
//!   transactions, including the handle once registered. Anyone holding the recovery phrase can
//!   still use the account. The Keychain's per-device install record also stays.
//!
//! The on-chain step goes first, since it needs the key to sign. A failure there leaves this
//! device untouched, and a failure part-way through the local step leaves the key in place, so
//! calling again resumes: apps already cleared are skipped.

use algo_ops::{AlgoChainConfig, AlgoOps};
use bingle_core::api::bingle_api::BingleError;
use bingle_core::blockchain::algo_bingle::AlgoBingle;

use crate::api::Keypair;

/// Opts one account out of the Bingle DApp. A seam so the deletion sequence can be tested without
/// a chain.
pub trait AccountOptOut {
    /// Clear `keypair`'s account's local state from the configured app and its ancestors. Returns
    /// the app ids cleared; empty if the account held no state, which is not an error.
    fn opt_out(&self, keypair: &Keypair) -> Result<Vec<u64>, BingleError>;
}

/// [`AccountOptOut`] on the configured Algorand network, through
/// [`AlgoBingle::leave_app_lineage`].
pub struct ChainAccountOptOut {
    algo_config: AlgoChainConfig,
    app_id: u64,
}

impl ChainAccountOptOut {
    /// Opt out of `app_id` and its ancestors on the network `algo_config` describes.
    pub fn new(algo_config: AlgoChainConfig, app_id: u64) -> Self {
        Self {
            algo_config,
            app_id,
        }
    }
}

impl AccountOptOut for ChainAccountOptOut {
    fn opt_out(&self, keypair: &Keypair) -> Result<Vec<u64>, BingleError> {
        if self.app_id == 0 {
            return Err(BingleError::Other("app_id not set in config".to_string()));
        }
        let ops = AlgoOps::new_for_algorand(
            Some(keypair.passphrase.clone()),
            None,
            Some(self.algo_config.clone()),
        );
        AlgoBingle::new(ops, self.app_id, 0)
            .leave_app_lineage(self.app_id)
            .map_err(BingleError::from_anyhow)
    }
}
