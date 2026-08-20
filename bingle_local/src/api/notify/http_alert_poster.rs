//! The default [`AlertPoster`]: a best-effort, non-blocking HTTP POST to the notify gateway.

use super::AlertPoster;
use super::envelope::{AlertRequest, alert_status_accepted};

/// Default [`AlertPoster`]: fires the POST on a detached thread using a blocking HTTP client so the
/// give-up path never blocks on the network. A non-2xx (other than the gateway's coalescing
/// `429`) or a transport error is logged and swallowed.
///
/// The blocking client is built lazily inside the detached thread, never in the constructor: a
/// `reqwest::blocking` client owns a Tokio runtime, and creating or dropping one on a thread that is
/// already inside an async runtime (e.g. the webserver) panics. The detached thread is a plain OS
/// thread with no ambient runtime, so it is always safe there.
pub struct HttpAlertPoster;

impl HttpAlertPoster {
    /// Create a new HTTP alert poster.
    pub fn new() -> Self {
        Self
    }
}

impl Default for HttpAlertPoster {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertPoster for HttpAlertPoster {
    fn post_alert(&self, gateway_url: &str, body: AlertRequest) {
        let endpoint = format!("{}/alert", gateway_url.trim_end_matches('/'));
        // Detach: the nudge is best-effort and must not delay or block message delivery. The
        // blocking client is built here, on this plain OS thread, so its runtime is never created
        // or dropped inside a caller's async context.
        let spawned = std::thread::Builder::new()
            .name("bingle-notify-alert".to_string())
            .spawn(move || {
                let client = match reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(
                            "[notify][alert] could not build HTTP client for '{}': {}; dropping nudge",
                            body.audience,
                            e
                        );
                        return;
                    }
                };
                match client.post(&endpoint).json(&body).send() {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        // 200/202 accepted; 429 is the gateway coalescing/rate-limiting per caller
                        // — all fine. Anything else is logged and ignored.
                        if alert_status_accepted(status) {
                            // Info, not debug: the give-up nudge is a rare, significant event, so
                            // its outcome (including a 429 = gateway coalesced) should be visible at
                            // the default log level.
                            tracing::info!(
                                "[notify][alert] gateway accepted alert for '{}' (status {})",
                                body.audience,
                                status
                            );
                        } else {
                            tracing::warn!(
                                "[notify][alert] gateway rejected alert for '{}' (status {}); ignoring",
                                body.audience,
                                status
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "[notify][alert] transport error posting alert for '{}': {}; ignoring",
                            body.audience,
                            e
                        );
                    }
                }
            });
        if let Err(e) = spawned {
            // Failing to spawn the detached thread is itself best-effort: log and drop the nudge.
            tracing::warn!(
                "[notify][alert] could not spawn alert thread: {}; dropping nudge",
                e
            );
        }
    }
}
