//! Give-up nudge to the notify gateway (bingle_notify #11).
//!
//! When `bingle_local`'s store-and-retry loop gives up delivering a message to a recipient, it
//! POSTs a content-free `/alert` to the notify gateway for that recipient so their device(s) wake
//! and the end-to-end flow can retry while both are online. Putting this in `bingle_local` means
//! every client built on the core gets it for free, including non-RN apps.
//!
//! The concerns are split across submodules:
//! - [`envelope`] — the content-free `/alert` body, the shared signer, and freshness helpers.
//! - [`http_alert_poster`] — the default non-blocking HTTP poster.
//! - [`nudge_helper`] — the per-recipient give-up orchestration.
//!
//! The POST is best-effort and must never block or fail message delivery.

pub mod envelope;
pub mod http_alert_poster;
pub mod http_register_poster;
pub mod nudge_helper;
pub mod register;

pub use envelope::AlertRequest;
#[doc(hidden)]
pub use envelope::{alert_exp, alert_status_accepted, build_alert_request, fresh_nonce, now_secs};
pub use http_alert_poster::HttpAlertPoster;
pub use http_register_poster::HttpRegisterPoster;
#[doc(hidden)]
pub use nudge_helper::post_giveup_alerts;
pub use register::RegisterRequest;
#[doc(hidden)]
pub use register::{build_register_request, encode_apns_token};

/// Best-effort delivery of an `/alert` POST to the notify gateway.
///
/// Implementations must **not block** the caller: the nudge fires from the message give-up path and
/// may never delay or fail delivery. The default [`HttpAlertPoster`] fires the request on a
/// detached thread and returns immediately. This is a seam so tests can observe the request without
/// a live gateway.
pub trait AlertPoster: Send + Sync {
    /// Send `body` to `{gateway_url}/alert`. Best-effort: any non-2xx status or transport error is
    /// logged by the implementation and swallowed — never surfaced to the caller.
    fn post_alert(&self, gateway_url: &str, body: AlertRequest);
}

/// Synchronous delivery of a `/register` POST to the notify gateway.
///
/// Unlike [`AlertPoster`], registration is an explicit user action whose outcome matters, so this
/// returns whether the gateway accepted it (`Ok(true)` on 2xx, `Ok(false)` on a rejection such as a
/// malformed token or failed auth, `Err` on a transport/spawn failure). A seam so tests can observe
/// the request without a live gateway.
pub trait RegisterPoster: Send + Sync {
    /// Send `body` to `{gateway_url}/register`.
    ///
    /// # Errors
    ///
    /// Returns `Ok(true)` when the gateway accepts the registration (2xx), `Ok(false)` on a
    /// rejection such as a malformed token or failed auth, and `Err` on a transport or spawn
    /// failure.
    fn post_register(
        &self,
        gateway_url: &str,
        body: register::RegisterRequest,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError>;
}
