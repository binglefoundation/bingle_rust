// Unit tests for crate::util::arc_retry::try_unwrap_arc_with_retries (issue #85).
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use bingle_core::util::arc_retry::try_unwrap_arc_with_retries;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unwraps_immediately_when_sole_owner() {
    let arc = Arc::new(42u32);
    let inner = try_unwrap_arc_with_retries(arc, 5, Duration::from_millis(1))
        .expect("sole owner should unwrap");
    assert_eq!(inner, 42);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn returns_arc_back_when_permanently_shared() {
    let arc = Arc::new(7u32);
    let _held = arc.clone(); // a second owner that never drops
    let result = try_unwrap_arc_with_retries(arc, 3, Duration::from_millis(1));
    // Cannot unwrap; the still-shared Arc is handed back so the caller can recover.
    let returned = result.expect_err("shared Arc should not unwrap");
    assert_eq!(*returned, 7);
    assert_eq!(Arc::strong_count(&returned), 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn succeeds_after_a_transient_second_owner_drops() {
    // Mirror the DTLS race: another thread holds a clone briefly, then drops it. The retry loop
    // should succeed once the transient owner is gone.
    let arc = Arc::new(99u32);
    let clone = arc.clone();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let handle = std::thread::spawn(move || {
        // Hold the extra reference until told to release, then drop it.
        let _hold = clone;
        let _ = release_rx.recv();
    });

    // Release the transient owner shortly after we start trying.
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        let _ = release_tx.send(());
    });

    // Generous attempt budget so the transient owner is released well within the retries.
    let inner = try_unwrap_arc_with_retries(arc, 200, Duration::from_millis(2))
        .expect("should unwrap once the transient owner drops");
    assert_eq!(inner, 99);

    handle.join().expect("holder thread");
    releaser.join().expect("releaser thread");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn max_attempts_is_clamped_to_at_least_one() {
    // 0 attempts still performs one immediate try.
    let arc = Arc::new(1u32);
    assert_eq!(
        try_unwrap_arc_with_retries(arc, 0, Duration::from_millis(1)).expect("one try"),
        1
    );
}
