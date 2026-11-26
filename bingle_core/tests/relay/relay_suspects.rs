// Issue #516: peer relays reported failed are kept as suspects and re-checked with backoff.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bingle_core::relay::relay_suspects::{
    RELAY_SUSPECT_FIRST_RECHECK, RELAY_SUSPECT_MAX_RECHECK, RelaySuspects,
};

/// A clock the test moves by hand.
fn manual_clock() -> (Arc<Mutex<Instant>>, Arc<dyn Fn() -> Instant + Send + Sync>) {
    let now = Arc::new(Mutex::new(Instant::now()));
    let reader = now.clone();
    (now, Arc::new(move || *reader.lock().unwrap()))
}

fn suspects_with_clock() -> (Arc<Mutex<Instant>>, RelaySuspects) {
    let (now, clock) = manual_clock();
    let suspects =
        RelaySuspects::with_clock(Duration::from_secs(30), Duration::from_secs(120), clock);
    (now, suspects)
}

#[test]
#[cfg(not(target_os = "ios"))]
fn first_recheck_waits_at_least_the_send_fail_backoff() {
    assert!(RELAY_SUSPECT_FIRST_RECHECK >= bingle_core::engine::SEND_FAIL_BACKOFF);
    assert!(RELAY_SUSPECT_MAX_RECHECK >= RELAY_SUSPECT_FIRST_RECHECK);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_marked_relay_is_due_after_the_first_wait() {
    let (now, suspects) = suspects_with_clock();
    assert!(suspects.mark("RELAYA"));
    assert!(suspects.is_suspect("RELAYA"));
    assert!(suspects.due().is_empty());

    *now.lock().unwrap() += Duration::from_secs(30);
    assert_eq!(suspects.due(), vec!["RELAYA".to_string()]);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn marking_again_keeps_the_backoff() {
    let (now, suspects) = suspects_with_clock();
    assert!(suspects.mark("RELAYA"));
    *now.lock().unwrap() += Duration::from_secs(20);
    assert!(!suspects.mark("RELAYA"), "already suspect");

    *now.lock().unwrap() += Duration::from_secs(10);
    assert_eq!(
        suspects.due(),
        vec!["RELAYA".to_string()],
        "a repeated report does not push the re-check back"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn failed_rechecks_double_the_wait_up_to_the_cap() {
    let (now, suspects) = suspects_with_clock();
    suspects.mark("RELAYA");
    assert_eq!(
        suspects.recheck_failed("RELAYA"),
        Some(Duration::from_secs(60))
    );
    assert_eq!(
        suspects.recheck_failed("RELAYA"),
        Some(Duration::from_secs(120))
    );
    assert_eq!(
        suspects.recheck_failed("RELAYA"),
        Some(Duration::from_secs(120)),
        "capped"
    );

    *now.lock().unwrap() += Duration::from_secs(119);
    assert!(suspects.due().is_empty());
    *now.lock().unwrap() += Duration::from_secs(1);
    assert_eq!(suspects.due(), vec!["RELAYA".to_string()]);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn clearing_ends_suspicion() {
    let (now, suspects) = suspects_with_clock();
    suspects.mark("RELAYA");
    assert!(suspects.clear("RELAYA"));
    assert!(!suspects.is_suspect("RELAYA"));
    assert!(suspects.is_empty());
    assert!(!suspects.clear("RELAYA"), "nothing left to clear");
    assert_eq!(suspects.recheck_failed("RELAYA"), None);

    *now.lock().unwrap() += Duration::from_secs(600);
    assert!(suspects.due().is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
fn due_lists_only_relays_whose_wait_is_over_sorted() {
    let (now, suspects) = suspects_with_clock();
    suspects.mark("RELAYB");
    suspects.mark("RELAYA");
    suspects.mark("RELAYC");
    suspects.recheck_failed("RELAYC");

    *now.lock().unwrap() += Duration::from_secs(30);
    assert_eq!(
        suspects.due(),
        vec!["RELAYA".to_string(), "RELAYB".to_string()]
    );
}
