//! Start-up check that the account is registered on the configured app (issue #314).
//!
//! After the app is replaced by a new one, an account registered on the old app is not opted in to
//! the new one until it migrates. Commands run with such an account used to carry on and fail later
//! with an unrelated symptom (relays dropping its requests, "No relays discovered"). Every command
//! that starts the engine now checks first and names the fix. The classification and messages here
//! are pure; the chain reads and the exiting stay in `main.rs`.

use bingle_core::blockchain::algo_bingle::AlgoBingle;

/// The account's registration on the configured app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountReadiness {
    /// The account holds a handle on the app.
    Registered,
    /// The account holds no handle on the app, but has state on blessed ancestor `ancestor` that
    /// `bingle_cli migrate` (or `--auto-migrate`) would copy across.
    NeedsMigration {
        /// The nearest ancestor app holding the account's state.
        ancestor: u64,
    },
    /// The account has no state on the app or on any of its ancestors: it must register.
    Unregistered,
}

/// Classify the account from whether it holds a handle on the app and the nearest blessed ancestor
/// that holds its state (if any).
pub fn classify_account(holds_handle: bool, migratable_ancestor: Option<u64>) -> AccountReadiness {
    if holds_handle {
        AccountReadiness::Registered
    } else if let Some(ancestor) = migratable_ancestor {
        AccountReadiness::NeedsMigration { ancestor }
    } else {
        AccountReadiness::Unregistered
    }
}

/// Read the account's registration on `app_id` from chain. The ancestors are only read when the
/// account holds no handle on the app. Errors are the chain read's, as text.
pub fn readiness_on_chain(
    bingle: &AlgoBingle,
    app_id: u64,
    account: &str,
) -> Result<AccountReadiness, String> {
    if bingle
        .holds_handle(app_id, account)
        .map_err(|e| e.to_string())?
    {
        return Ok(AccountReadiness::Registered);
    }
    let ancestor = bingle
        .find_migratable_ancestor(app_id, account)
        .map_err(|e| e.to_string())?;
    Ok(classify_account(false, ancestor))
}

/// The message for an account whose state is on ancestor app `ancestor` but has not been migrated
/// to `app_id`.
pub fn migration_needed_message(account: &str, app_id: u64, ancestor: u64) -> String {
    format!(
        "account {account} is not registered on app {app_id}: it is registered on ancestor app \
         {ancestor} and has not been migrated. Migrate it once with \
         `bingle_cli migrate --passphrase <mnemonic> --node-file <node_file>`, or re-run this \
         command with --auto-migrate."
    )
}

/// The message for an account with no state on `app_id` or any of its ancestors.
pub fn registration_needed_message(account: &str, app_id: u64) -> String {
    format!(
        "account {account} is not registered on app {app_id} or any of its ancestor apps. \
         Register it first with \
         `bingle_cli register --handle <handle> --passphrase <mnemonic> --node-file <node_file> --price-units <n>`."
    )
}
