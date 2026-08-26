//! Small wall-clock helpers shared across the Bingle crates.

use std::time::{SystemTime, UNIX_EPOCH};

/// The current wall-clock time in epoch milliseconds, or `0` if the system clock is before the Unix
/// epoch. Used for locally-stamped timestamps — e.g. the store-and-forward `delivered_time` stamped
/// when a message is read from the Sidewinder Mailbox (issue #215).
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
