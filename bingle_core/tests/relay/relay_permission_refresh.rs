// Tests for RelayPermissionRefresher: the relay-side loop that periodically evicts relays whose
// on-chain allow_relay permission was withdrawn (issue #120).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bingle_core::relay::relay_permission_refresh::{
    RELAY_PERMISSION_REFRESH_INTERVAL, RelayPermissionRefresher,
};

fn counting_pass(count: Arc<AtomicU64>, keep_going: bool) -> Box<dyn Fn() -> bool + Send> {
    Box::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
        keep_going
    })
}

fn wait_for(cond: impl Fn() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    cond()
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn default_interval_is_five_minutes() {
    assert_eq!(RELAY_PERMISSION_REFRESH_INTERVAL, Duration::from_secs(300));
}

/// The first pass runs at start, so a disabled relay re-added during initialisation is cleared
/// without waiting a full interval.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn runs_a_pass_immediately_on_start() {
    let count = Arc::new(AtomicU64::new(0));
    let mut r = RelayPermissionRefresher::new(
        Duration::from_secs(3600),
        counting_pass(count.clone(), true),
    );
    r.start();
    assert!(wait_for(
        || count.load(Ordering::SeqCst) == 1,
        Duration::from_secs(2)
    ));
    r.stop();
    assert_eq!(r.pass_count(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn repeats_once_per_interval() {
    let count = Arc::new(AtomicU64::new(0));
    let mut r = RelayPermissionRefresher::new(
        Duration::from_millis(30),
        counting_pass(count.clone(), true),
    );
    r.start();
    assert!(wait_for(
        || count.load(Ordering::SeqCst) >= 3,
        Duration::from_secs(2)
    ));
    r.stop();
}

/// stop() wakes the loop out of a long interval rather than waiting it out.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn stop_returns_promptly_during_a_long_interval() {
    let count = Arc::new(AtomicU64::new(0));
    let mut r = RelayPermissionRefresher::new(
        Duration::from_secs(3600),
        counting_pass(count.clone(), true),
    );
    r.start();
    assert!(wait_for(
        || count.load(Ordering::SeqCst) == 1,
        Duration::from_secs(2)
    ));
    let started = Instant::now();
    r.stop();
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// A pass that reports its owner gone ends the loop.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn ends_when_the_pass_reports_its_owner_gone() {
    let count = Arc::new(AtomicU64::new(0));
    let mut r = RelayPermissionRefresher::new(
        Duration::from_millis(10),
        counting_pass(count.clone(), false),
    );
    r.start();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    r.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_second_start_is_a_no_op() {
    let count = Arc::new(AtomicU64::new(0));
    let mut r = RelayPermissionRefresher::new(
        Duration::from_secs(3600),
        counting_pass(count.clone(), true),
    );
    r.start();
    r.start();
    assert!(wait_for(
        || count.load(Ordering::SeqCst) == 1,
        Duration::from_secs(2)
    ));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    r.stop();
}
