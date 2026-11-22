// Unit tests for the logclient spam limiter (bingle_cli::spam_limit, issue #66).
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use bingle_cli::spam_limit::{Admission, Clock, SpamLimiter};
use chrono::{TimeZone, Utc};
use serde_json::{Value, json};

/// A clock the test moves by hand.
#[derive(Clone)]
pub struct TestClock(Arc<AtomicI64>);

impl TestClock {
    /// A clock reading `millis`.
    pub fn at(millis: i64) -> Self {
        Self(Arc::new(AtomicI64::new(millis)))
    }

    pub fn set(&self, millis: i64) {
        self.0.store(millis, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now_millis(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Epoch milliseconds for a UTC time.
pub fn utc_millis(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s)
        .single()
        .expect("valid time")
        .timestamp_millis()
}

const HOUR: i64 = 60 * 60 * 1000;

fn forwards(limiter: &mut SpamLimiter, sender: &str) -> bool {
    limiter.admit(sender) == Admission::Forward
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_nth_message_is_forwarded_and_the_next_is_suppressed() {
    let clock = TestClock::at(utc_millis(2026, 11, 21, 12, 0, 0));
    let mut limiter = SpamLimiter::new(3, Box::new(clock));

    for _ in 0..3 {
        assert!(forwards(&mut limiter, "PEER_BOB"));
    }
    assert_eq!(
        limiter.admit("PEER_BOB"),
        Admission::Suppress {
            log: Some("suppressed messages from PEER_BOB (limit 3 reached)".to_string())
        }
    );
    assert_eq!(limiter.count("PEER_BOB"), 3);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn each_sender_has_its_own_count() {
    let clock = TestClock::at(utc_millis(2026, 11, 21, 12, 0, 0));
    let mut limiter = SpamLimiter::new(1, Box::new(clock));

    assert!(forwards(&mut limiter, "PEER_BOB"));
    assert!(!forwards(&mut limiter, "PEER_BOB"));
    assert!(forwards(&mut limiter, "PEER_CAROL"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_count_resets_at_utc_midnight() {
    let clock = TestClock::at(utc_millis(2026, 11, 21, 23, 59, 59));
    let mut limiter = SpamLimiter::new(2, Box::new(clock.clone()));
    assert!(forwards(&mut limiter, "PEER_BOB"));
    assert!(forwards(&mut limiter, "PEER_BOB"));
    assert!(!forwards(&mut limiter, "PEER_BOB"));

    clock.set(utc_millis(2026, 11, 22, 0, 0, 0));
    assert!(forwards(&mut limiter, "PEER_BOB"));
    assert_eq!(limiter.count("PEER_BOB"), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_new_day_drops_the_earlier_days_counts() {
    let clock = TestClock::at(utc_millis(2026, 11, 21, 10, 0, 0));
    let mut limiter = SpamLimiter::new(5, Box::new(clock.clone()));
    assert!(forwards(&mut limiter, "PEER_BOB"));

    clock.set(utc_millis(2026, 11, 22, 10, 0, 0));
    assert!(forwards(&mut limiter, "PEER_CAROL"));

    let record: Value = serde_json::from_str(&limiter.record().expect("record")).expect("json");
    assert_eq!(
        record,
        json!({ "day": "2026-11-22", "counts": { "PEER_CAROL": 1 } })
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn suppressions_are_logged_once_then_at_most_hourly_with_a_count() {
    let start = utc_millis(2026, 11, 21, 9, 0, 0);
    let clock = TestClock::at(start);
    let mut limiter = SpamLimiter::new(1, Box::new(clock.clone()));
    assert!(forwards(&mut limiter, "PEER_BOB"));

    assert!(matches!(
        limiter.admit("PEER_BOB"),
        Admission::Suppress { log: Some(_) }
    ));
    clock.set(start + HOUR - 1);
    assert_eq!(limiter.admit("PEER_BOB"), Admission::Suppress { log: None });
    clock.set(start + HOUR);
    assert_eq!(
        limiter.admit("PEER_BOB"),
        Admission::Suppress {
            log: Some("suppressed 3 messages from PEER_BOB today (limit 1)".to_string())
        }
    );
    assert_eq!(limiter.admit("PEER_BOB"), Admission::Suppress { log: None });
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn restored_counts_carry_on_within_the_day() {
    let clock = TestClock::at(utc_millis(2026, 11, 21, 8, 0, 0));
    let mut first = SpamLimiter::new(2, Box::new(clock.clone()));
    assert!(forwards(&mut first, "PEER_BOB"));
    assert!(forwards(&mut first, "PEER_BOB"));
    let record = first.record().expect("record");

    clock.set(utc_millis(2026, 11, 21, 20, 0, 0));
    let mut second = SpamLimiter::new(2, Box::new(clock));
    second.restore(&record).expect("restore");
    assert!(!forwards(&mut second, "PEER_BOB"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn restored_counts_from_an_earlier_day_do_not_apply() {
    let clock = TestClock::at(utc_millis(2026, 11, 22, 8, 0, 0));
    let mut limiter = SpamLimiter::new(1, Box::new(clock));
    limiter
        .restore(r#"{"day": "2026-11-21", "counts": {"PEER_BOB": 1}}"#)
        .expect("restore");
    assert!(forwards(&mut limiter, "PEER_BOB"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_unreadable_record_is_refused_and_changes_nothing() {
    let clock = TestClock::at(utc_millis(2026, 11, 21, 8, 0, 0));
    let mut limiter = SpamLimiter::new(1, Box::new(clock));
    for bad in [
        "not json",
        r#"{"counts": {}}"#,
        r#"{"day": "21/11/2026", "counts": {}}"#,
        r#"{"day": "2026-11-21"}"#,
        r#"{"day": "2026-11-21", "counts": {"PEER_BOB": -1}}"#,
    ] {
        assert!(limiter.restore(bad).is_err(), "{bad}");
    }
    assert_eq!(limiter.record(), None);
    assert!(forwards(&mut limiter, "PEER_BOB"));
}
