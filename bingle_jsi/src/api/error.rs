use crate::api::types::FailureKind;

/// Errors returned by the Bingle JSI API.
///
/// The bridges report each variant's name and fields to JavaScript as `error.userInfo`
/// (bingle_rust #470). A new variant must be added to both bridges' `bingle-error-kinds` blocks and
/// to `BingleJsiErrorInfo` in `ts/index.ts`; `tests/api/bridge_coverage_tests.rs` checks this.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum BingleJsiError {
    #[error("Not found: {reason}")]
    NotFound { reason: String },

    #[error("Invalid request: {reason}")]
    InvalidRequest { reason: String },

    #[error("Not implemented: {reason}")]
    NotImplemented { reason: String },

    #[error("Internal error: {reason}")]
    InternalError { reason: String },

    #[error("No blockchain: {reason}")]
    NoBlockchain { reason: String },

    #[error("Retryable error: {reason}")]
    Retryable { reason: String },

    #[error("Handle already in use: {reason}")]
    HandleTaken { reason: String },

    /// The key storage settings exist only for the Keychain store (issue #385); this instance keeps
    /// the recovery phrase in the state file.
    #[error("Key storage unavailable: {reason}")]
    KeyStorageUnavailable { reason: String },
    /// The sealed state file can't be opened or saved because the device is locked (issue #459).
    /// Try again after the device unlocks.
    #[error("State file locked: {reason}")]
    StateLocked { reason: String },
    /// The sealed state file doesn't open on this device, or the recovery phrase given doesn't open
    /// it (issue #459). Ask for the account's recovery phrase.
    #[error("Recovery phrase required: {reason}")]
    RecoveryPhraseRequired { reason: String },
    /// Generating or importing a key would replace a different key the user syncs through iCloud
    /// Keychain, removing that account from every device on their Apple Account (issue #452).
    /// Nothing changed. Warn the user, naming `stored_address`, and retry with
    /// `replace_synced_key: true` if they agree.
    #[error("Synced key would be replaced: {stored_address}")]
    SyncedKeyWouldBeReplaced { stored_address: String },
    /// A send failed (bingle_rust #554). `failure_kind` is the typed cause, the same one a stored
    /// message's `failure_kind` carries; `reason` is the detail, for logs.
    #[error("Send failed ({failure_kind:?}): {reason}")]
    SendFailed {
        failure_kind: FailureKind,
        reason: String,
    },
}
