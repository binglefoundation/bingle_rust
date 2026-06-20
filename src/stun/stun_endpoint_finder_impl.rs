use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

// Bring the public trait/types from our module
use super::stun_endpoint_finder::{ErrorHandler, SendPacketHandler, StateChangeHandler, StunEndpointFinder, StunState};

// Explicitly reference the stun-rs crate (module path stun_rs). We keep usage minimal here
// to avoid depending on specific message builder APIs while still ensuring the crate is
// integrated and available for future enhancements.
#[allow(unused_imports)]
use stun_rs as _stun_rs;

const TICK_MS: u64 = 100;

#[derive(Clone)]
struct ServerStatus {
    addr: SocketAddr,
    failures: u8,
    responded: bool,
    ever_responded: bool,
    endpoint: Option<SocketAddr>,
    last_polled_tick: Option<u64>,
}

impl ServerStatus {
    fn new(addr: SocketAddr) -> Self {
        Self { addr, failures: 0, responded: false, ever_responded: false, endpoint: None, last_polled_tick: None }
    }
}

struct Inner {
    servers: Vec<ServerStatus>,
    state: StunState,
    endpoint: Option<SocketAddr>,
    search_ticks: u64,
    repeat_ticks: u64,
    state_change: Option<StateChangeHandler>,
    error: Option<ErrorHandler>,
    send_packet: Option<SendPacketHandler>,
    // Error reporting bookkeeping
    intervals_without_two: u8,
    error_reported: bool,
    // Tick of the last received STUN response
    last_response_tick: Option<u64>,
    // Tick of the last sent STUN binding request while in Consistent/Inconsistent state.
    // The no-response timeout is measured from this point: we revert only when
    // no response has arrived since this request and the timeout has elapsed.
    last_request_tick: Option<u64>,
    // How many ticks without any response before reverting from Consistent/Inconsistent to None
    no_response_ticks: u64,
    current_tick: u64,
    last_poll_tick: Option<u64>,
}

impl Inner {
    fn choose_interval(&self) -> u64 {
        match self.state {
            StunState::None | StunState::Single | StunState::Blocked => self.search_ticks,
            StunState::Consistent | StunState::Inconsistent => self.repeat_ticks,
        }
    }

    fn stun_process_tick(&mut self) {
        self.current_tick += 1;
        let now = self.current_tick;

        // 1. Check for silence timeout (Revert to None)
        if matches!(self.state, StunState::Consistent | StunState::Inconsistent) {
            let silence_too_long = match (self.last_request_tick, self.last_response_tick) {
                (Some(req_t), Some(resp_t)) => {
                    now.saturating_sub(req_t) > self.no_response_ticks && resp_t < req_t
                }
                (Some(req_t), None) => now.saturating_sub(req_t) > self.no_response_ticks,
                (None, _) => false,
            };

            if silence_too_long {
                tracing::info!(
                    "[STUN] no response for {} ticks; reverting {:?} → None",
                    self.no_response_ticks, self.state
                );
                self.last_response_tick = None;
                self.last_request_tick = None;
                self.intervals_without_two = 0;
                // Reset per-server state so the next round re-polls everyone.
                for s in self.servers.iter_mut() {
                    s.responded = false;
                    s.failures = 0;
                    s.endpoint = None;
                }
                // Setting state to None BEFORE calling recompute_state_and_notify
                // was preventing the notification because it thought the state hadn't changed.
                // We let recompute_state_and_notify handle the transition from the old state.
                self.recompute_state_and_notify();
            }
        }

        // 2. Decide if it is time to poll based on the current state's interval
        let interval_ticks = self.choose_interval();

        if self.last_poll_tick.map_or(true, |last| now.saturating_sub(last) >= interval_ticks) {
            self.last_poll_tick = Some(now);

            // Determine servers to poll
            let to_poll: Vec<SocketAddr> = match self.state {
                StunState::None | StunState::Single => self.servers.iter().filter(|s| !s.responded).map(|s| s.addr).collect(),
                StunState::Blocked | StunState::Consistent | StunState::Inconsistent => self.servers.iter().map(|s| s.addr).collect(),
            };

            if !to_poll.is_empty() {
                tracing::info!("[STUN] polling servers: {:?}, tick {}", to_poll, now);
                for addr in to_poll {
                    let pkt = StunEndpointFinderImpl::build_binding_request(now);
                    let handler = self.send_packet.clone();

                    if let Some(s) = self.servers.iter_mut().find(|x| x.addr == addr) {
                        s.last_polled_tick = Some(now);
                        s.failures = s.failures.saturating_add(1);
                        // Clear stale response data so the previous round's endpoint
                        // does not interfere with the new round's consistency check.
                        s.responded = false;
                        s.endpoint = None;
                    }

                    if matches!(self.state, StunState::Consistent | StunState::Inconsistent)
                        && self.last_request_tick.is_none()
                    {
                        self.last_request_tick = Some(now);
                    }

                    if let Some(h) = handler.as_ref() {
                        let host = addr.ip().to_string();
                        h(&host, addr.port(), &pkt);
                    }
                }

                // 3. Blocked state check (using results of PREVIOUS interval)
                if matches!(self.state, StunState::None | StunState::Single | StunState::Blocked) {
                    self.intervals_without_two = self.intervals_without_two.saturating_add(1);
                    if self.intervals_without_two >= 3 {
                        let responders = self.servers.iter().filter(|s| s.responded).count();
                        if responders == 0 {
                            if self.state != StunState::Blocked {
                                tracing::warn!("[STUN] UDP appears blocked (no responses from {} servers after 3 intervals)", self.servers.len());
                                self.state = StunState::Blocked;
                                self.endpoint = None;
                                if let Some(cb) = &self.state_change {
                                    cb(self.state, None);
                                }
                            } else {
                                tracing::info!("[STUN] still blocked (UDP appears blocked) - continuing to poll for recovery");
                            }
                        } else if responders < 2 && !self.error_reported {
                            if let Some(eh) = &self.error {
                                eh("Fewer than 2 STUN servers responded after 3 tries".to_string());
                            }
                            self.error_reported = true;
                        }

                        if self.state != StunState::Blocked {
                            for s in self.servers.iter_mut() {
                                if s.ever_responded {
                                    s.responded = false;
                                }
                            }
                            self.intervals_without_two = 0;
                        }
                    }
                }
            }

            // 4. Pruning
            if matches!(self.state, StunState::Blocked) {
                for s in self.servers.iter_mut() {
                    s.failures = 0;
                }
            } else {
                self.servers.retain(|s| s.failures < 3 || s.ever_responded);
            }
        }
    }

    fn recompute_state_and_notify(&mut self) {
        // Save the old endpoint before the match so we can detect address changes
        // while the state stays Consistent (Bug 1 fix).
        let old_endpoint = self.endpoint;
        // Collect endpoints for all servers that have responded
        let mut endpoints = Vec::new();
        for s in &self.servers {
            if let Some(ep) = s.endpoint { endpoints.push(ep); }
        }
        let new_state = match endpoints.len() {
            0 => StunState::None,
            1 => StunState::Single,
            _ => {
                let first = endpoints[0];
                if endpoints.iter().all(|&e| e == first) {
                    self.endpoint = Some(first);
                    tracing::info!("[STUN] endpoint Consistent: {:?}", self.endpoint);
                    StunState::Consistent
                } else {
                    self.endpoint = None;
                    tracing::info!("[STUN] endpoint Inconsistent");
                    StunState::Inconsistent
                }
            }
        };
        // For None or Single, clear remembered endpoint
        if matches!(new_state, StunState::None | StunState::Single) {
            self.endpoint = None;
        }
        // Fire callback when the state variant changes OR when the endpoint address
        // changes while the state stays Consistent.  Without the endpoint check, a NAT
        // port change that both servers agree on in the same round would be silently
        // dropped because new_state == self.state == Consistent.
        let endpoint_changed = new_state == StunState::Consistent && self.endpoint != old_endpoint;
        if new_state != self.state || endpoint_changed {
            self.state = new_state;
            tracing::info!("[STUN] state change: {:?} endpoint={:?}", self.state, self.endpoint);
            if let Some(cb) = &self.state_change {
                cb(self.state, self.endpoint);
            }
        }
    }
}

/// Public implementation of the StunEndpointFinder trait using a background thread.
pub struct StunEndpointFinderImpl {
    inner: Arc<Mutex<Inner>>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    stop_cond: Arc<Condvar>,
    /// Override the no-response timeout (used in tests to avoid 30s waits).
    no_response_timeout_override: Option<Duration>,
}

impl StunEndpointFinderImpl {
    pub fn new() -> Self {
        let inner = Inner {
            servers: Vec::new(),
            state: StunState::None,
            endpoint: None,
            search_ticks: 1000 / TICK_MS,
            repeat_ticks: 10000 / TICK_MS,
            state_change: None,
            error: None,
            send_packet: None,
            intervals_without_two: 0,
            error_reported: false,
            last_response_tick: None,
            last_request_tick: None,
            no_response_ticks: 30000 / TICK_MS,
            current_tick: 0,
            last_poll_tick: None,
        };
        Self { inner: Arc::new(Mutex::new(inner)), running: Arc::new(AtomicBool::new(false)), thread: None, stop_cond: Arc::new(Condvar::new()), no_response_timeout_override: None }
    }

    /// Override the no-response timeout. Used in tests to avoid waiting 30s.
    pub fn set_no_response_timeout_for_tests(&mut self, timeout: Duration) {
        self.no_response_timeout_override = Some(timeout);
        self.inner.lock().unwrap().no_response_ticks = (timeout.as_millis() as u64 / TICK_MS).max(1);
    }


    // Build a minimal STUN Binding Request. Keeping a handcrafted packet for now keeps us
    // decoupled from stun-rs' specific builder API while still having it as a dependency for future use.
    fn build_binding_request(tick: u64) -> [u8; 20] {
        // Simple STUN Binding Request with zero-length attributes.
        // Type: 0x0001, Length: 0x0000, Magic Cookie: 0x2112A442, Transaction ID: 12 bytes pseudo-random
        let mut pkt = [0u8; 20];
        pkt[0] = 0x00; pkt[1] = 0x01; // Binding Request
        pkt[2] = 0x00; pkt[3] = 0x00; // length
        pkt[4] = 0x21; pkt[5] = 0x12; pkt[6] = 0xA4; pkt[7] = 0x42; // magic cookie
        // Fill transaction ID deterministically from tick
        let mut seed = tick;
        for i in 0..3 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let bytes = seed.to_be_bytes();
            pkt[8 + i * 4 .. 12 + i * 4].copy_from_slice(&bytes[0..4]);
        }
        pkt
    }

    /// Manually advance the state machine by one tick. Used in tests.
    pub fn tick_for_test(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.stun_process_tick();
    }

    /// Advance the state machine by multiple ticks. Used in tests.
    pub fn test_ticks(&self, count: u32) {
        for _ in 0..count {
            self.tick_for_test();
        }
    }

    fn parse_xor_mapped_address(data: &[u8]) -> Option<SocketAddr> {
        if data.len() < 20 { return None; }
        // Verify magic cookie
        if data[4..8] != [0x21, 0x12, 0xA4, 0x42] { return None; }
        let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
        let mut idx = 20;
        let end = 20 + msg_len;
        while idx + 4 <= data.len() && idx + 4 <= end {
            let atype = u16::from_be_bytes([data[idx], data[idx + 1]]);
            let alen = u16::from_be_bytes([data[idx + 2], data[idx + 3]]) as usize;
            idx += 4;
            if idx + alen > data.len() { break; }
            if atype == 0x0020 /* XOR-MAPPED-ADDRESS */ || atype == 0x0001 /* MAPPED-ADDRESS */ {
                if alen < 8 { return None; }
                let family = data[idx + 1];
                if family == 0x01 && alen >= 8 {
                    // IPv4
                    let xport = u16::from_be_bytes([data[idx + 2], data[idx + 3]]);
                    let port = if atype == 0x0020 { xport ^ 0x2112 } else { xport };
                    let mut addr = [0u8; 4];
                    addr.copy_from_slice(&data[idx + 4..idx + 8]);
                    if atype == 0x0020 {
                        let cookie = [0x21u8, 0x12, 0xA4, 0x42];
                        for i in 0..4 { addr[i] ^= cookie[i]; }
                    }
                    let ip = IpAddr::V4(Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]));
                    return Some(SocketAddr::new(ip, port));
                }
            }
            // Move to next attribute (4-byte alignment)
            let pad = (4 - (alen % 4)) % 4;
            idx += alen + pad;
        }
        None
    }
}

impl StunEndpointFinder for StunEndpointFinderImpl {
    fn start(&mut self, servers: Vec<SocketAddr>, search_time_ms: u64, repeat_time_ms: u64) {
        // Initialize internal state
        {
            let mut inner = self.inner.lock().unwrap();
            inner.servers = servers.into_iter().map(ServerStatus::new).collect();
            inner.search_ticks = (search_time_ms / TICK_MS).max(1);
            inner.repeat_ticks = (repeat_time_ms / TICK_MS).max(1);
            inner.state = StunState::None;
            inner.endpoint = None;
            inner.intervals_without_two = 0;
            inner.error_reported = false;
            inner.last_response_tick = None;
            inner.last_request_tick = None;
            inner.current_tick = 0;
            inner.last_poll_tick = None;
            if let Some(t) = self.no_response_timeout_override {
                inner.no_response_ticks = (t.as_millis() as u64 / TICK_MS).max(1);
            } else {
                inner.no_response_ticks = 30000 / TICK_MS;
            }
        }
        // Start background thread if not already running
        if self.running.swap(true, Ordering::SeqCst) { return; }
        let running = self.running.clone();
        let state = self.inner.clone();
        let stop_cond = self.stop_cond.clone();
        self.thread = Some(thread::spawn(move || {
            let tick_rate = Duration::from_millis(TICK_MS);
            loop {
                if !running.load(Ordering::SeqCst) { break; }

                {
                    let mut inner = state.lock().unwrap();
                    inner.stun_process_tick();
                }

                // Wait for the next tick or stop signal
                let inner_lock = state.lock().unwrap();
                let _ = stop_cond.wait_timeout(inner_lock, tick_rate).unwrap();
            }
        }));
    }

    fn stop(&mut self) {
        tracing::info!("[StunEndpointFinder] stopping background thread");
        if self.running.swap(false, Ordering::SeqCst) {
            self.stop_cond.notify_all();
            if let Some(h) = self.thread.take() {
                tracing::info!("[StunEndpointFinder] join on background thread, waiting for it to finish...");
                let _ = h.join();
            }
            else {
                tracing::warn!("[StunEndpointFinder] background thread not running, nothing to stop");
            }
        }
        else {
            tracing::warn!("[StunEndpointFinder] stop called without start");
        }
        tracing::info!("[StunEndpointFinder] stopped background thread");
    }

    fn process_packet(&mut self, from: SocketAddr, data: &[u8]) {
        let endpoint = StunEndpointFinderImpl::parse_xor_mapped_address(data);
        tracing::info!("[STUN] received response from {} mapped={:?}", from, endpoint);
        let mut inner = self.inner.lock().unwrap();
        // Find server by source address
        if let Some(s) = inner.servers.iter_mut().find(|s| s.addr == from) {
            s.responded = true;
            s.ever_responded = true;
            s.failures = 0; // reset on any response
            if let Some(ep) = endpoint { s.endpoint = Some(ep); }
        }
        // Track when the last response arrived so the poll loop can detect silence.
        inner.last_response_tick = Some(inner.current_tick);
        // Clear the first-unanswered-request anchor: this response means the current
        // silent window is over, so the next poll will start a fresh window.
        inner.last_request_tick = None;
        // Recompute state and notify
        inner.recompute_state_and_notify();
    }

    fn set_state_change_handler(&mut self, handler: Option<StateChangeHandler>) {
        let mut inner = self.inner.lock().unwrap();
        inner.state_change = handler;
    }

    fn set_error_handler(&mut self, handler: Option<ErrorHandler>) {
        let mut inner = self.inner.lock().unwrap();
        inner.error = handler;
    }

    fn set_send_packet_handler(&mut self, handler: Option<SendPacketHandler>) {
        let mut inner = self.inner.lock().unwrap();
        inner.send_packet = handler;
    }

    fn reset_state(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        tracing::info!("[STUN] reset_state: reverting state {:?} -> None", inner.state);
        inner.state = StunState::None;
        inner.endpoint = None;
        // Reset interval counter and error flag so the polling loop does not immediately
        // re-enter Blocked state (which would happen if intervals_without_two was still >= 3).
        inner.intervals_without_two = 0;
        inner.error_reported = false;
        // Clear the last-response and last-request timestamps so the no-response timeout
        // does not fire immediately after a reset (the clock starts fresh from now).
        inner.last_response_tick = None;
        inner.last_request_tick = None;
        inner.current_tick = 0;
        // Reset per-server state so the polling loop will re-poll all servers.
        // ever_responded is intentionally preserved: a server that was reachable before
        // the reset is still a known-good server and should continue to be kept alive.
        for s in inner.servers.iter_mut() {
            s.responded = false;
            s.endpoint = None;
            s.failures = 0;
        }
    }
}

impl Drop for StunEndpointFinderImpl {
    fn drop(&mut self) {
        self.stop();
    }
}
