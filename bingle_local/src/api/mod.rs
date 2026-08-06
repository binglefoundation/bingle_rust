// This module has been split so that the BingleLocalApi trait and related types live
// in a dedicated file `bingle_local_api.rs`. Keep this wrapper to preserve the
// `bingle_local::api::*` path while referencing the new file.

pub mod bingle_local_api;
pub use bingle_local_api::*;

// Local implementation stub (only generate_keypair currently implemented)
pub mod bingle_local_api_impl;
pub use bingle_local_api_impl::*;

// On-chain registration seam (see issue #15, step A4).
pub mod registration;
pub use registration::{ChainRegistrationOps, RegistrationOps, run_registration};

// Give-up nudge to the notify gateway (bingle_notify #11).
pub mod notify;
pub use notify::{AlertPoster, AlertRequest, HttpAlertPoster, build_alert_request};

// Shared outbound-send retry policy (issue #82): used by bingle_jsi (RN client) and bingle_cli chat.
pub mod send_retry;
pub use send_retry::{
    RETRY_BACKOFF, is_transient_send_failure, pending_failure_reason, select_sendable_message,
};
