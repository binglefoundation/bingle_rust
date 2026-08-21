//! Orchestration for the give-up nudge: sign one content-free `/alert` per recipient and hand each
//! to the poster. Pulled out of the API impl so the give-up call site stays thin and this policy
//! (one alert per recipient, best-effort, off the delivery path) is testable on its own.

use algo_ops::AlgoOps;

use super::AlertPoster;
use super::envelope::{alert_exp, build_alert_request, fresh_nonce, now_secs};

/// Fire the best-effort give-up nudge for each recipient we gave up on (bingle_notify #11).
///
/// The gate (`notify_on_giveup` and a configured `gateway_url`) is assumed already checked by the
/// caller, which also resolves `iss` (the local handle) and `ops` (the active keypair). For each
/// recipient this signs a fresh content-free alert envelope with `ops`, issued as `iss`, and hands
/// it to `poster`. Best-effort: a signing failure for one recipient is logged and skipped; the
/// poster sends off the delivery path, so this never blocks or fails message delivery.
#[doc(hidden)]
pub fn post_giveup_alerts(
    poster: &dyn AlertPoster,
    gateway_url: &str,
    ops: &AlgoOps,
    iss: &str,
    recipient_handles: &[String],
) {
    // A single expiry for the whole give-up event; each recipient gets its own fresh nonce.
    let exp = alert_exp(now_secs());
    for audience in recipient_handles {
        let nonce = fresh_nonce();
        match build_alert_request(ops, iss, audience, &nonce, exp) {
            Ok(req) => {
                tracing::info!(
                    "[notify_giveup] posting give-up alert for recipient '{}'",
                    audience
                );
                poster.post_alert(gateway_url, req);
            }
            Err(e) => tracing::warn!(
                "[notify_giveup] could not sign alert for '{}' ({}); skipping",
                audience,
                e
            ),
        }
    }
}
