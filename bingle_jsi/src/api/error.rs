/// Errors returned by the Bingle JSI API.
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
}
