use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::api::bingle_api::{BingleApiBothType, NetworkEndpoint};
use crate::messages::marshal::to_json_value;
use crate::messages::types::{Message, RelayKeepAlive, RelayMessage};

/// How often a NAT-restricted client refreshes its NAT mapping towards its
/// registered relay. NAT UDP timeouts are commonly 30s-5min for unused
/// mappings but relays see traffic on the same 5-tuple, so 10 minutes keeps
/// the mapping alive without meaningful load.
pub const RELAY_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(600);

/// Base delay before retrying after a *failed* keep-alive send. On failure the loop no longer waits
/// a full interval (which left inbound dark for up to ~10 minutes after a background/idle resume —
/// issue #50); it retries on an exponential backoff starting here and doubling up to the normal
/// interval. A prompt retry re-establishes the DTLS session, which both refreshes the relay's
/// listener mapping and fires the engine's fresh-outbound-session trigger to re-register.
pub const RELAY_KEEP_ALIVE_FAILURE_BASE_BACKOFF: Duration = Duration::from_secs(5);

/// Compute the wait before the next keep-alive attempt: the full `interval` when the last send
/// succeeded (`consecutive_failures == 0`), otherwise `base * 2^(failures-1)` capped at `interval`.
/// Public so the backoff schedule can be verified directly in unit tests.
pub fn next_wait(interval: Duration, base: Duration, consecutive_failures: u32) -> Duration {
    if consecutive_failures == 0 {
        return interval;
    }
    // Cap the shift so the multiply can't overflow, then clamp the result to the interval.
    let shift = (consecutive_failures - 1).min(16);
    let factor = 1u32 << shift;
    base.checked_mul(factor).unwrap_or(interval).min(interval)
}

/// Background sender that periodically sends a Relay::KeepAlive to the relay
/// this client registered with (TURN Listen), refreshing the NAT mapping for
/// the client<->relay 5-tuple so the relay can keep delivering inbound data.
pub struct RelayKeepAliveSender {
    api: BingleApiBothType,
    relay_id: String,
    relay_addr: SocketAddr,
    interval: Duration,
    running: Arc<AtomicBool>,
    stop_signal: Arc<(Mutex<()>, Condvar)>,
    send_count: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl RelayKeepAliveSender {
    pub fn new(
        api: BingleApiBothType,
        relay_id: String,
        relay_addr: SocketAddr,
        interval: Duration,
    ) -> Self {
        Self {
            api,
            relay_id,
            relay_addr,
            interval,
            running: Arc::new(AtomicBool::new(false)),
            stop_signal: Arc::new((Mutex::new(()), Condvar::new())),
            send_count: Arc::new(AtomicU64::new(0)),
            thread: None,
        }
    }

    pub fn relay_id(&self) -> &str {
        &self.relay_id
    }

    pub fn relay_addr(&self) -> SocketAddr {
        self.relay_addr
    }

    /// Number of keep-alives sent so far (successful sends only).
    pub fn send_count(&self) -> u64 {
        self.send_count.load(Ordering::SeqCst)
    }

    pub fn start(&mut self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let running = self.running.clone();
        let stop_signal = self.stop_signal.clone();
        let send_count = self.send_count.clone();
        let api = self.api.clone();
        let relay_id = self.relay_id.clone();
        let relay_addr = self.relay_addr;
        let interval = self.interval;
        self.thread = Some(thread::spawn(move || {
            // Consecutive failed sends; drives the failure backoff and resets to 0 on success.
            let mut consecutive_failures: u32 = 0;
            loop {
                // Wait before the next send. Normally a full interval (registration/last send just
                // refreshed the mapping); after a failure, a short backoff so a stale session is
                // rebuilt promptly instead of leaving inbound dark for ~10 min (issue #50). Use a
                // deadline so spurious condvar wakeups don't shorten the wait.
                let wait = next_wait(
                    interval,
                    RELAY_KEEP_ALIVE_FAILURE_BASE_BACKOFF,
                    consecutive_failures,
                );
                let deadline = Instant::now() + wait;
                loop {
                    let now = Instant::now();
                    if now >= deadline {
                        break;
                    }
                    // Check `running` while holding the stop_signal lock, then wait under the
                    // same lock. stop() flips `running` to false and notifies under this lock,
                    // so a notification cannot slip in between the check and the wait. Without
                    // this, a lost wakeup would leave us parked for the full (possibly very
                    // long) interval, hanging the join() in stop().
                    let guard = stop_signal.0.lock().unwrap();
                    if !running.load(Ordering::SeqCst) {
                        return;
                    }
                    let _ = stop_signal.1.wait_timeout(guard, deadline - now).unwrap();
                }
                if !running.load(Ordering::SeqCst) {
                    return;
                }

                let Some(api) = api.upgrade() else {
                    tracing::info!("[RelayKeepAliveSender] api dropped; exiting keep-alive loop");
                    return;
                };
                let msg = Message::Relay(RelayMessage::KeepAlive(RelayKeepAlive { app: None }));
                let nsk = NetworkEndpoint::new_direct(relay_addr);
                match api.send_message_to_network(&nsk, &relay_id, to_json_value(&msg), None) {
                    Ok(_) => {
                        consecutive_failures = 0;
                        send_count.fetch_add(1, Ordering::SeqCst);
                        tracing::info!(
                            "[RelayKeepAliveSender] sent KeepAlive to relay {} ({})",
                            relay_id,
                            relay_addr
                        );
                    }
                    Err(e) => {
                        // Don't stop refreshing on a transient failure, but retry soon (short
                        // backoff) rather than after a full interval: a failed keep-alive means the
                        // session went stale (e.g. resumed from background), and a prompt retry
                        // rebuilds the DTLS pipe — refreshing the relay mapping and driving
                        // re-registration — instead of leaving inbound dark for ~10 min (issue #50).
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        tracing::warn!(
                            "[RelayKeepAliveSender] KeepAlive send to relay {} ({}) failed (attempt {}): {}; retrying in {:?}",
                            relay_id,
                            relay_addr,
                            consecutive_failures,
                            e,
                            next_wait(
                                interval,
                                RELAY_KEEP_ALIVE_FAILURE_BASE_BACKOFF,
                                consecutive_failures
                            )
                        );
                    }
                }
            }
        }));
    }

    pub fn stop(&mut self) {
        if self.running.swap(false, Ordering::SeqCst) {
            // Notify under the stop_signal lock so this happens-after the waiter's
            // `running` check (which is also done under the lock): the waiter either
            // sees running=false before waiting, or is already parked and receives this
            // notification. This closes the lost-wakeup race that could otherwise hang
            // join() until the (possibly very long) keep-alive interval elapsed.
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

impl Drop for RelayKeepAliveSender {
    fn drop(&mut self) {
        self.stop();
    }
}
