// Tests for PeriodicTask, the loop behind the relay permission refresh (#120) and the advert
// refresh and expiry sweep (#280).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bingle_core::util::periodic_task::{FirstPass, PeriodicTask};

fn counting_pass(count: Arc<AtomicU64>) -> Box<dyn Fn() -> bool + Send> {
    Box::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
        true
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

/// With `AfterInterval` nothing runs until a full interval has passed, so a refresh does not
/// repeat the registration that was just made.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn after_interval_waits_before_the_first_pass() {
    let count = Arc::new(AtomicU64::new(0));
    let mut task = PeriodicTask::new(
        "test",
        Duration::from_millis(300),
        FirstPass::AfterInterval,
        counting_pass(count.clone()),
    );
    task.start();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "no pass before the interval"
    );
    assert!(
        wait_for(|| count.load(Ordering::SeqCst) >= 1, Duration::from_secs(5)),
        "a pass runs once the interval has passed"
    );
    task.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn immediately_runs_a_pass_at_start() {
    let count = Arc::new(AtomicU64::new(0));
    let mut task = PeriodicTask::new(
        "test",
        Duration::from_secs(3600),
        FirstPass::Immediately,
        counting_pass(count.clone()),
    );
    task.start();
    assert!(wait_for(
        || count.load(Ordering::SeqCst) == 1,
        Duration::from_secs(5)
    ));
    task.stop();
}

/// Stop wakes a loop waiting out a long first interval rather than waiting for it.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn stop_during_the_first_interval_returns_promptly() {
    let count = Arc::new(AtomicU64::new(0));
    let mut task = PeriodicTask::new(
        "test",
        Duration::from_secs(3600),
        FirstPass::AfterInterval,
        counting_pass(count.clone()),
    );
    task.start();
    let started = Instant::now();
    task.stop();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(count.load(Ordering::SeqCst), 0);
}
