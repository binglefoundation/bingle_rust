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
    pub fn new(api: BingleApiBothType, relay_id: String, relay_addr: SocketAddr, interval: Duration) -> Self {
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
            loop {
                // Wait one full interval before the first send: registration itself
                // just sent a Listen, so the mapping is fresh. Use a deadline so
                // spurious condvar wakeups don't shorten the interval.
                let deadline = Instant::now() + interval;
                loop {
                    if !running.load(Ordering::SeqCst) {
                        return;
                    }
                    let now = Instant::now();
                    if now >= deadline {
                        break;
                    }
                    let guard = stop_signal.0.lock().unwrap();
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
                        send_count.fetch_add(1, Ordering::SeqCst);
                        tracing::info!("[RelayKeepAliveSender] sent KeepAlive to relay {} ({})", relay_id, relay_addr);
                    }
                    Err(e) => {
                        // Keep looping: a transient failure should not stop refreshes,
                        // and dead-relay failover is handled elsewhere.
                        tracing::warn!("[RelayKeepAliveSender] KeepAlive send to relay {} ({}) failed: {}", relay_id, relay_addr, e);
                    }
                }
            }
        }));
    }

    pub fn stop(&mut self) {
        if self.running.swap(false, Ordering::SeqCst) {
            self.stop_signal.1.notify_all();
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
