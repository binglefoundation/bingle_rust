//! The default [`RegisterPoster`]: a synchronous HTTP POST of a `/register` envelope.
//!
//! Unlike the give-up `/alert` nudge (best-effort, fire-and-forget off the delivery path),
//! registration is an explicit, user-triggered action whose outcome matters — so this poster is
//! synchronous and returns whether the gateway accepted the registration.

use super::RegisterPoster;
use super::register::RegisterRequest;
use bingle_core::api::bingle_api::BingleError;

/// Default [`RegisterPoster`]: POSTs the envelope and reports acceptance.
///
/// The POST runs on a freshly spawned OS thread that is then joined for the result. A
/// `reqwest::blocking` client owns a Tokio runtime, and building or dropping one on a thread already
/// inside an async runtime panics; the dedicated thread has no ambient runtime, so it is always
/// safe — while the join still gives the caller a synchronous success/failure.
pub struct HttpRegisterPoster;

impl HttpRegisterPoster {
    /// Create a new HTTP register poster.
    pub fn new() -> Self {
        Self
    }
}

impl Default for HttpRegisterPoster {
    fn default() -> Self {
        Self::new()
    }
}

impl RegisterPoster for HttpRegisterPoster {
    fn post_register(&self, gateway_url: &str, body: RegisterRequest) -> Result<bool, BingleError> {
        let endpoint = format!("{}/register", gateway_url.trim_end_matches('/'));
        std::thread::Builder::new()
            .name("bingle-notify-register".to_string())
            .spawn(move || {
                let client = reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                    .map_err(|e| BingleError::Other(format!("could not build HTTP client: {e}")))?;
                let resp = client
                    .post(&endpoint)
                    .json(&body)
                    .send()
                    .map_err(|e| BingleError::Other(format!("transport error: {e}")))?;
                let status = resp.status().as_u16();
                if (200..300).contains(&status) {
                    tracing::info!(
                        "[notify][register] gateway accepted registration (status {status})"
                    );
                    Ok(true)
                } else {
                    // 400 (malformed token), 401 (auth) etc. — the caller surfaces this; do not retry.
                    tracing::warn!(
                        "[notify][register] gateway rejected registration (status {status})"
                    );
                    Ok(false)
                }
            })
            .map_err(|e| BingleError::Other(format!("could not spawn register thread: {e}")))?
            .join()
            .map_err(|_| BingleError::Other("register thread panicked".to_string()))?
    }
}
