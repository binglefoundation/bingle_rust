use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How often a relay re-checks the on-chain `allow_relay` permission of the relays in its set
/// (issue #120). A relay disabled on chain leaves the set within this interval, without a sign-off
/// or a cluster restart. Each pass makes one chain read per relay in the set, which is a handful.
pub const RELAY_PERMISSION_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

/// One refresh pass. Returns `false` when the owner has gone away and the loop should end.
pub type RefreshPass = Box<dyn Fn() -> bool + Send + 'static>;

/// Background loop on a relay that runs a refresh pass at start and then once per interval, to
/// evict relays whose on-chain `allow_relay` permission has been withdrawn from the relay set
/// (issue #120). The pass itself is supplied by the engine, so the loop is testable on its own.
pub struct RelayPermissionRefresher {
    interval: Duration,
    pass: Option<RefreshPass>,
    running: Arc<AtomicBool>,
    stop_signal: Arc<(Mutex<()>, Condvar)>,
    pass_count: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl RelayPermissionRefresher {
    /// Create a refresher that runs `pass` every `interval` once started.
    pub fn new(interval: Duration, pass: RefreshPass) -> Self {
        Self {
            interval,
            pass: Some(pass),
            running: Arc::new(AtomicBool::new(false)),
            stop_signal: Arc::new((Mutex::new(()), Condvar::new())),
            pass_count: Arc::new(AtomicU64::new(0)),
            thread: None,
        }
    }

    /// Number of refresh passes run so far.
    pub fn pass_count(&self) -> u64 {
        self.pass_count.load(Ordering::SeqCst)
    }

    /// Start the loop: one pass now, then one per interval until [`stop`](Self::stop).
    /// A second call is a no-op.
    pub fn start(&mut self) {
        let Some(pass) = self.pass.take() else {
            return;
        };
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let stop_signal = self.stop_signal.clone();
        let pass_count = self.pass_count.clone();
        let interval = self.interval;
        self.thread = Some(thread::spawn(move || {
            loop {
                if !running.load(Ordering::SeqCst) {
                    return;
                }
                let keep_going = pass();
                pass_count.fetch_add(1, Ordering::SeqCst);
                if !keep_going {
                    tracing::info!(
                        "[RelayPermissionRefresher] owner dropped; exiting refresh loop"
                    );
                    return;
                }
                // Wait a full interval, waking early on stop. As in RelayKeepAliveSender, check
                // `running` under the stop_signal lock and wait under the same lock, so a stop()
                // notification cannot be lost between the check and the wait.
                let deadline = Instant::now() + interval;
                loop {
                    let now = Instant::now();
                    if now >= deadline {
                        break;
                    }
                    let guard = stop_signal.0.lock().unwrap();
                    if !running.load(Ordering::SeqCst) {
                        return;
                    }
                    let _ = stop_signal.1.wait_timeout(guard, deadline - now).unwrap();
                }
            }
        }));
    }

    /// Stop the loop and wait for it to finish. An in-flight pass runs to completion first.
    pub fn stop(&mut self) {
        if self.running.swap(false, Ordering::SeqCst) {
            {
                let _guard = self.stop_signal.0.lock().unwrap();
                self.stop_signal.1.notify_all();
            }
            if let Some(h) = self.thread.take() {
                let _ = h.join();
            }
        }
    }
}

impl Drop for RelayPermissionRefresher {
    fn drop(&mut self) {
        self.stop();
    }
}
