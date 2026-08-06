//! Retry helper for `Arc::try_unwrap` when a brief, transient extra reference may exist.
//!
//! The DTLS peer writer clones the shared stream `Arc` out of its mutex before writing (so the write
//! does not hold the mutex), which means a concurrent worker can hold a short-lived extra reference
//! for the duration of one write. When the send path wants to take sole ownership of that `Arc` to
//! split the stream, a single `Arc::try_unwrap` can spuriously fail against that in-flight clone.
//! Retrying a handful of times with a small delay lets the transient reference drop; if it truly
//! cannot be taken, the `Arc` is handed back so the caller can recover (issue #85).

use std::sync::Arc;
use std::time::Duration;

/// Try to take sole ownership of `arc`, retrying up to `max_attempts` times with `delay` between
/// attempts. Returns `Ok(inner)` once this is the only owner, or `Err(arc)` (the still-shared `Arc`,
/// so the caller can restore it) if every attempt found another owner.
///
/// `max_attempts` is clamped to at least 1 (one immediate try).
pub fn try_unwrap_arc_with_retries<T>(
    mut arc: Arc<T>,
    max_attempts: u32,
    delay: Duration,
) -> Result<T, Arc<T>> {
    let attempts = max_attempts.max(1);
    for attempt in 0..attempts {
        match Arc::try_unwrap(arc) {
            Ok(inner) => return Ok(inner),
            Err(returned) => {
                arc = returned;
                // Don't sleep after the final attempt — we're about to give up.
                if attempt + 1 < attempts {
                    std::thread::sleep(delay);
                }
            }
        }
    }
    Err(arc)
}
