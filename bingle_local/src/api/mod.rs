//! The local-state API: the [`BingleLocalApi`] trait, its implementation, and the supporting
//! registration and notification seams.
//!
//! The trait and its data types live in [`bingle_local_api`]; the concrete implementation and its
//! configuration live in [`bingle_local_api_impl`]; on-chain handle registration lives in
//! [`registration`]; and best-effort push notifications live in [`notify`].

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

// Sidewinder Mailbox (FIFO) client for store-and-forward (epic #200, foundation story #213).
pub mod sidewinder;
pub use sidewinder::{MAILBOX_POP_TYPE, MAILBOX_POST_TYPE, Mailbox, MailboxConfig};

// Give-up nudge to the notify gateway (bingle_notify #11).
pub mod notify;
#[doc(hidden)]
pub use notify::build_alert_request;
pub use notify::{AlertPoster, AlertRequest, HttpAlertPoster};

// Shared outbound-send retry policy (issue #82): used by bingle_jsi (RN client) and bingle_cli chat.
pub mod send_retry;
#[doc(hidden)]
pub use send_retry::{
    RETRY_BACKOFF, SendFailure, classify_send_error, is_transient_send_failure,
    pending_failure_reason, select_sendable_message,
};
// Typed send-failure cause (issue #99), re-exported for clients that classify send results.
pub use bingle_core::api::bingle_api::SendFailureKind;
