use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// One pass of a [`PeriodicTask`]. Returns `false` when the owner has gone away and the loop
/// should end.
pub type PeriodicPass = Box<dyn Fn() -> bool + Send + 'static>;

/// When a [`PeriodicTask`] runs its first pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstPass {
    /// Run a pass as soon as the task starts, then once per interval.
    Immediately,
    /// Wait one interval before the first pass.
    AfterInterval,
}

/// Background loop that runs a pass once per interval until stopped. The pass is supplied by the
/// owner (usually the engine), so the loop is testable on its own.
///
/// Used for the relay permission refresh (issue #120), and for the advert refresh and the
/// expired-advert sweep (issue #280).
pub struct PeriodicTask {
    name: &'static str,
    interval: Duration,
    first_pass: FirstPass,
    pass: Option<PeriodicPass>,
    running: Arc<AtomicBool>,
    stop_signal: Arc<(Mutex<()>, Condvar)>,
    pass_count: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl PeriodicTask {
    /// Create a task that runs `pass` every `interval` once started. `name` is used in logs.
    pub fn new(
        name: &'static str,
        interval: Duration,
        first_pass: FirstPass,
        pass: PeriodicPass,
    ) -> Self {
        Self {
            name,
            interval,
            first_pass,
            pass: Some(pass),
            running: Arc::new(AtomicBool::new(false)),
            stop_signal: Arc::new((Mutex::new(()), Condvar::new())),
            pass_count: Arc::new(AtomicU64::new(0)),
            thread: None,
        }
    }

    /// Number of passes run so far.
    pub fn pass_count(&self) -> u64 {
        self.pass_count.load(Ordering::SeqCst)
    }

    /// The interval between passes.
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Start the loop. A second call is a no-op.
    pub fn start(&mut self) {
        let Some(pass) = self.pass.take() else {
            return;
        };
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let stop_signal = self.stop_signal.clone();
        let pass_count = self.pass_count.clone();
        let interval = self.interval;
        let name = self.name;
        let mut wait_first = self.first_pass == FirstPass::AfterInterval;
        self.thread = Some(thread::spawn(move || {
            loop {
                if wait_first {
                    // Wait a full interval, waking early on stop. As in RelayKeepAliveSender, check
                    // `running` under the stop_signal lock and wait under the same lock, so a
                    // stop() notification cannot be lost between the check and the wait.
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
                wait_first = true;
                if !running.load(Ordering::SeqCst) {
                    return;
                }
                let keep_going = pass();
                pass_count.fetch_add(1, Ordering::SeqCst);
                if !keep_going {
                    tracing::info!("[PeriodicTask:{name}] owner dropped; exiting loop");
                    return;
                }
            }
        }));
    }

    /// Stop the loop and wait for it to finish. An in-flight pass runs to completion first.
    ///
    /// Called from the loop thread itself (the owner dropped while a pass held the last
    /// reference), it only signals the loop to end, since a thread cannot join itself.
    pub fn stop(&mut self) {
        if self.running.swap(false, Ordering::SeqCst) {
            {
                let _guard = self.stop_signal.0.lock().unwrap();
                self.stop_signal.1.notify_all();
            }
            if let Some(h) = self.thread.take()
                && h.thread().id() != thread::current().id()
            {
                let _ = h.join();
            }
        }
    }
}

impl Drop for PeriodicTask {
    fn drop(&mut self) {
        self.stop();
    }
}
