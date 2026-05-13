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
}
