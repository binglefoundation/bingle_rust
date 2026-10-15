// Issue #122: the negative cache of relays a status probe could not reach.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bingle_core::relay::relay_probe_failures::{RELAY_PROBE_FAILURE_COOLDOWN, RelayProbeFailures};

/// A clock the test moves by hand.
fn manual_clock() -> (Arc<Mutex<Instant>>, Arc<dyn Fn() -> Instant + Send + Sync>) {
    let now = Arc::new(Mutex::new(Instant::now()));
    let reader = now.clone();
    (now, Arc::new(move || *reader.lock().unwrap()))
}

#[test]
#[cfg(not(target_os = "ios"))]
fn failure_cools_down_for_the_cooldown_then_expires() {
    let (now, clock) = manual_clock();
    let failures = RelayProbeFailures::with_clock(Duration::from_secs(60), clock);
    failures.record_failure("RELAYA");
    assert!(failures.is_cooling_down("RELAYA"));

    *now.lock().unwrap() += Duration::from_secs(59);
    assert!(failures.is_cooling_down("RELAYA"));

    *now.lock().unwrap() += Duration::from_secs(1);
    assert!(!failures.is_cooling_down("RELAYA"));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn success_clears_a_failure() {
    let failures = RelayProbeFailures::new();
    failures.record_failure("RELAYA");
    failures.record_success("RELAYA");
    assert!(!failures.is_cooling_down("RELAYA"));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn only_failed_relays_cool_down() {
    let failures = RelayProbeFailures::new();
    failures.record_failure("RELAYA");
    assert!(!failures.is_cooling_down("RELAYB"));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn default_cooldown_is_a_minute() {
    assert_eq!(RELAY_PROBE_FAILURE_COOLDOWN, Duration::from_secs(60));
}
