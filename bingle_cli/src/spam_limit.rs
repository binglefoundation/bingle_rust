//! A per-sender daily cap on the messages `logclient` forwards (epic #63, issue #66), so one sender
//! can't flood the email channel.
//!
//! Once a sender has had `limit` messages forwarded in a UTC day, its later messages that day are
//! suppressed: `logclient` keeps them in the state-file history but doesn't hand them to the sink.
//! The counts are kept in the state file as an app record ([`SPAM_LIMIT_RECORD`]), so a restart
//! within the day doesn't reset them; counts from an earlier day are dropped on the first message
//! of a new day.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};

/// The state-file app record holding the day's counts:
/// `{"day": "<YYYY-MM-DD>", "counts": {"<sender>": <n>, …}}`.
pub const SPAM_LIMIT_RECORD: &str = "logclient.spam_limit";

/// The least time between two suppression log lines for one sender, in milliseconds.
const LOG_INTERVAL_MILLIS: i64 = 60 * 60 * 1000;

/// Where the limiter reads the time, so tests can move it.
pub trait Clock: Send {
    /// The current time, in milliseconds since the Unix epoch.
    fn now_millis(&self) -> i64;
}

/// The system clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_millis(&self) -> i64 {
        Utc::now().timestamp_millis()
    }
}

/// What to do with one message.
#[derive(Debug, PartialEq, Eq)]
pub enum Admission {
    /// Forward it: the sender is within its limit, and this message has been counted.
    Forward,
    /// Don't forward it. `log` is a line to log, at most one an hour per sender.
    Suppress { log: Option<String> },
}

/// How many of a sender's messages were suppressed today, and when that was last logged.
struct Suppressed {
    count: u64,
    last_logged_millis: i64,
}

/// Counts each sender's forwarded messages per UTC day against a fixed limit.
pub struct SpamLimiter {
    limit: u32,
    clock: Box<dyn Clock>,
    /// The UTC day `counts` are for; `None` until the first message or restore.
    day: Option<NaiveDate>,
    counts: BTreeMap<String, u32>,
    /// Today's suppressions, for logging only; not persisted.
    suppressed: BTreeMap<String, Suppressed>,
}

impl SpamLimiter {
    /// A limiter forwarding at most `limit` messages per sender per UTC day, reading the time from
    /// `clock`.
    pub fn new(limit: u32, clock: Box<dyn Clock>) -> Self {
        Self {
            limit,
            clock,
            day: None,
            counts: BTreeMap::new(),
            suppressed: BTreeMap::new(),
        }
    }

    /// The daily limit.
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// Take the counts from a stored [`SPAM_LIMIT_RECORD`] value. Counts for an earlier day are
    /// kept until the next message drops them. Returns why the record can't be read, changing
    /// nothing.
    pub fn restore(&mut self, record: &str) -> Result<(), String> {
        let value: Value = serde_json::from_str(record).map_err(|e| e.to_string())?;
        let day = value["day"]
            .as_str()
            .ok_or("no day")
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| "a bad day"))?;
        let stored = value["counts"].as_object().ok_or("no counts")?;
        let mut counts = BTreeMap::new();
        for (sender, count) in stored {
            let count = count
                .as_u64()
                .and_then(|c| u32::try_from(c).ok())
                .ok_or_else(|| format!("a bad count for {sender}"))?;
            counts.insert(sender.clone(), count);
        }
        self.day = Some(day);
        self.counts = counts;
        Ok(())
    }

    /// The counts as a [`SPAM_LIMIT_RECORD`] value, or `None` before the first message or restore.
    pub fn record(&self) -> Option<String> {
        let day = self.day?;
        Some(
            json!({ "day": day.format("%Y-%m-%d").to_string(), "counts": self.counts }).to_string(),
        )
    }

    /// How many of `sender`'s messages have been forwarded on the current day.
    pub fn count(&self, sender: &str) -> u32 {
        self.counts.get(sender).copied().unwrap_or(0)
    }

    /// Decide whether to forward a message from `sender` (its Bingle id, or its handle when the id
    /// isn't known), counting it when it is forwarded.
    pub fn admit(&mut self, sender: &str) -> Admission {
        let now = self.clock.now_millis();
        let Some(today) = DateTime::<Utc>::from_timestamp_millis(now).map(|t| t.date_naive())
        else {
            // A clock outside chrono's range: forward rather than lose the message.
            tracing::warn!("logclient: clock reads {now} ms; not applying the spam limit");
            return Admission::Forward;
        };
        if self.day != Some(today) {
            self.day = Some(today);
            self.counts.clear();
            self.suppressed.clear();
        }

        let count = self.counts.entry(sender.to_string()).or_insert(0);
        if *count < self.limit {
            *count += 1;
            return Admission::Forward;
        }

        let limit = self.limit;
        let log = match self.suppressed.get_mut(sender) {
            None => {
                self.suppressed.insert(
                    sender.to_string(),
                    Suppressed {
                        count: 1,
                        last_logged_millis: now,
                    },
                );
                Some(format!(
                    "suppressed messages from {sender} (limit {limit} reached)"
                ))
            }
            Some(s) => {
                s.count += 1;
                (now - s.last_logged_millis >= LOG_INTERVAL_MILLIS).then(|| {
                    s.last_logged_millis = now;
                    format!(
                        "suppressed {} messages from {sender} today (limit {limit})",
                        s.count
                    )
                })
            }
        };
        Admission::Suppress { log }
    }
}
