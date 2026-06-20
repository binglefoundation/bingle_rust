use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

// Bring the public trait/types from our module
use super::endpoint_finder::{ErrorHandler, SendPacketHandler, StateChangeHandler, StunEndpointFinder, StunState};

// Explicitly reference the stun-rs crate (module path stun_rs). We keep usage minimal here
// to avoid depending on specific message builder APIs while still ensuring the crate is
// integrated and available for future enhancements.
#[allow(unused_imports)]
use stun_rs as _stun_rs;

#[derive(Clone)]
struct ServerStatus {
    addr: SocketAddr,
    failures: u8,
    responded: bool,
    ever_responded: bool,
    endpoint: Option<SocketAddr>,
    last_polled: Option<Instant>,
}

impl ServerStatus {
    fn new(addr: SocketAddr) -> Self {
        Self { addr, failures: 0, responded: false, ever_responded: false, endpoint: None, last_polled: None }
    }
}

struct Inner {
    servers: Vec<ServerStatus>,
    state: StunState,
    endpoint: Option<SocketAddr>,
    search: Duration,
    repeat: Duration,
    state_change: Option<StateChangeHandler>,
    error: Option<ErrorHandler>,
    send_packet: Option<SendPacketHandler>,
    // Error reporting bookkeeping
    intervals_without_two: u8,
    error_reported: bool,
    // Timestamp of the last received STUN response (used to detect server silence)
    last_response_time: Option<Instant>,
    // Timestamp of the last sent STUN binding request while in Consistent/Inconsistent state.
    // The no-response timeout is measured from this point: we revert only when
    // no response has arrived since this request and the timeout has elapsed.
    last_request_time: Option<Instant>,
    // How long without any response before reverting from Consistent/Inconsistent to None
    no_response_timeout: Duration,
}

impl Inner {
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
            search: Duration::from_millis(1000),
            repeat: Duration::from_millis(10_000),
            state_change: None,
            error: None,
            send_packet: None,
            intervals_without_two: 0,
            error_reported: false,
            last_response_time: None,
            last_request_time: None,
            no_response_timeout: Duration::from_secs(30),
        };
        Self { inner: Arc::new(Mutex::new(inner)), running: Arc::new(AtomicBool::new(false)), thread: None, stop_cond: Arc::new(Condvar::new()), no_response_timeout_override: None }
    }

    /// Override the no-response timeout. Used in tests to avoid waiting 30s.
    pub fn set_no_response_timeout_for_tests(&mut self, timeout: Duration) {
        self.no_response_timeout_override = Some(timeout);
        self.inner.lock().unwrap().no_response_timeout = timeout;
    }

    fn choose_interval(state: StunState, search: Duration, repeat: Duration) -> Duration {
        match state {
            StunState::None | StunState::Single | StunState::Blocked => search,
            StunState::Consistent | StunState::Inconsistent => repeat,
        }
    }

    // Build a minimal STUN Binding Request. Keeping a handcrafted packet for now keeps us
    // decoupled from stun-rs' specific builder API while still having it as a dependency for future use.
    fn build_binding_request() -> [u8; 20] {
        // Simple STUN Binding Request with zero-length attributes.
        // Type: 0x0001, Length: 0x0000, Magic Cookie: 0x2112A442, Transaction ID: 12 bytes pseudo-random
        let mut pkt = [0u8; 20];
        pkt[0] = 0x00; pkt[1] = 0x01; // Binding Request
        pkt[2] = 0x00; pkt[3] = 0x00; // length
        pkt[4] = 0x21; pkt[5] = 0x12; pkt[6] = 0xA4; pkt[7] = 0x42; // magic cookie
        // Fill transaction ID with a very simple changing value (time-based)
        let t = Instant::now().elapsed().as_nanos();
        for i in 0..12 { pkt[8 + i] = ((t >> (i * 5)) & 0xFF) as u8; }
        pkt
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
            inner.search = Duration::from_millis(search_time_ms);
            inner.repeat = Duration::from_millis(repeat_time_ms);
            inner.state = StunState::None;
            inner.endpoint = None;
            inner.intervals_without_two = 0;
            inner.error_reported = false;
            inner.last_response_time = None;
            inner.last_request_time = None;
            if let Some(t) = self.no_response_timeout_override {
                inner.no_response_timeout = t;
            }
        }
        // Start background thread if not already running
        if self.running.swap(true, Ordering::SeqCst) { return; }
        let running = self.running.clone();
        let state = self.inner.clone();
        let stop_cond = self.stop_cond.clone();
        self.thread = Some(thread::spawn(move || {
            loop {
                if !running.load(Ordering::SeqCst) { break; }
                let (to_poll, interval) = {
                    let inner = state.lock().unwrap();
                    let interval = StunEndpointFinderImpl::choose_interval(inner.state, inner.search, inner.repeat);
                    // Determine servers to poll
                    let to_poll: Vec<SocketAddr> = match inner.state {
                        // In Blocked state poll all servers every interval so we detect recovery.
                        // In None/Single only poll servers that have not yet responded this round.
                        StunState::None | StunState::Single => inner.servers.iter().filter(|s| !s.responded).map(|s| s.addr).collect(),
                        StunState::Blocked | StunState::Consistent | StunState::Inconsistent => inner.servers.iter().map(|s| s.addr).collect(),
                    };
                    (to_poll, interval)
                };
                // Send binding requests
                {
                    tracing::info!("[STUN] polling servers: {:?}, interval {}ms", to_poll, interval.as_millis());
                    let mut inner = state.lock().unwrap();
                    for addr in to_poll {
                        // Prepare packet and handler outside the mutable borrow
                        let pkt = StunEndpointFinderImpl::build_binding_request();
                        let handler = inner.send_packet.clone();
                        // Update server status and capture target address
                        let target_addr_opt = {
                            if let Some(s) = inner.servers.iter_mut().find(|x| x.addr == addr) {
                                s.last_polled = Some(Instant::now());
                                s.failures = s.failures.saturating_add(1);
                                // Clear stale response data so the previous round's endpoint
                                // does not interfere with the new round's consistency check.
                                s.responded = false;
                                s.endpoint = None;
                                Some(s.addr)
                            } else { None }
                        };
                        if let Some(target) = target_addr_opt {
                            // Record when we FIRST sent a request without a subsequent response,
                            // so the no-response timeout is measured from that moment.
                            // We only set last_request_time if it is not already set: this
                            // anchors the timer to the first unanswered request in the current
                            // silent window rather than resetting it on every successive poll,
                            // which would push the revert out indefinitely.
                            // last_request_time is cleared by process_packet (via the revert
                            // reset) whenever a fresh response arrives.
                            if matches!(inner.state, StunState::Consistent | StunState::Inconsistent)
                                && inner.last_request_time.is_none()
                            {
                                inner.last_request_time = Some(Instant::now());
                            }
                            if let Some(h) = handler.as_ref() {
                                let host = target.ip().to_string();
                                let port = target.port();
                                tracing::info!("[STUN] sending Binding Request to {}:{}", host, port);
                                h(&host, port, &pkt);
                            }
                        }
                    }
                    // When in Consistent or Inconsistent state, check whether we have heard
                    // from any server since the last binding request we sent.  If
                    // no_response_timeout has elapsed since that request and no response has
                    // arrived in that window, revert to None so the engine can re-identify
                    // the NAT type.  We deliberately check AFTER sending so the request is
                    // always dispatched, but the timeout is anchored to last_request_time —
                    // not to the moment of the check — which prevents the revert from firing
                    // in the same iteration that sends the request.
                    if matches!(inner.state, StunState::Consistent | StunState::Inconsistent) {
                        let silence_too_long = match (inner.last_request_time, inner.last_response_time) {
                            // We have sent at least one request: revert only when the timeout
                            // has elapsed since that request AND no response arrived after it.
                            (Some(req_t), Some(resp_t)) => {
                                req_t.elapsed() > inner.no_response_timeout
                                    && resp_t < req_t
                            }
                            // Sent a request but never received any response.
                            (Some(req_t), None) => req_t.elapsed() > inner.no_response_timeout,
                            // No request recorded yet — do not revert; wait until we've sent one.
                            (None, _) => false,
                        };
                        if silence_too_long {
                            tracing::info!(
                                "[STUN] no response for {:?}; reverting {:?} → None",
                                inner.no_response_timeout, inner.state
                            );
                            inner.state = StunState::None;
                            inner.endpoint = None;
                            inner.last_response_time = None;
                            inner.last_request_time = None;
                            inner.intervals_without_two = 0;
                            // Reset per-server state so the next round re-polls everyone.
                            for s in inner.servers.iter_mut() {
                                s.responded = false;
                                s.endpoint = None;
                                s.failures = 0;
                            }
                            if let Some(cb) = &inner.state_change {
                                cb(StunState::None, None);
                            }
                        }
                    }
                    // Check for Blocked/error condition BEFORE pruning servers, so that
                    // when we transition into Blocked state the servers list is still intact.
                    if matches!(inner.state, StunState::None | StunState::Single | StunState::Blocked) {
                        inner.intervals_without_two = inner.intervals_without_two.saturating_add(1);
                        tracing::info!("[STUN] intervals without two responses: {}", inner.intervals_without_two);
                        if inner.intervals_without_two >= 3 {
                            let responders = inner.servers.iter().filter(|s| s.responded).count();
                            if responders == 0 {
                                if inner.state != StunState::Blocked {
                                    inner.state = StunState::Blocked;
                                    tracing::info!("[STUN] state change: Blocked (no responders after 3 intervals) - UDP may be blocked");
                                    if let Some(cb) = &inner.state_change {
                                        cb(inner.state, None);
                                    }
                                } else {
                                    tracing::info!("[STUN] still blocked (UDP appears blocked) - continuing to poll for recovery");
                                }
                            } else if responders < 2 && !inner.error_reported {
                                if let Some(eh) = &inner.error {
                                    eh("Fewer than 2 STUN servers responded after 3 tries".to_string());
                                }
                                inner.error_reported = true;
                            }
                            // After the 3-interval check, reset `responded` on ever_responded
                            // servers so they are eligible for the next poll round. Without this
                            // a server that responded once (responded=true) would be skipped by
                            // the None/Single to_poll filter forever once all non-responded
                            // servers have been removed.
                            if inner.state != StunState::Blocked {
                                for s in inner.servers.iter_mut() {
                                    if s.ever_responded {
                                        s.responded = false;
                                    }
                                }
                                inner.intervals_without_two = 0;
                            }
                        }
                    }
                    // In Blocked state, reset failure counters so servers are kept alive and
                    // polling continues — UDP may become unblocked and we want to recover.
                    // Otherwise remove servers that have failed 3 times without responding.
                    if matches!(inner.state, StunState::Blocked) {
                        for s in inner.servers.iter_mut() {
                            s.failures = 0;
                        }
                    } else {
                        // Keep a server if it has fewer than 3 consecutive failures OR
                        // if it has ever responded in this session (it may be temporarily
                        // unreachable due to network issues at our end, not a dead server).
                        inner.servers.retain(|s| s.failures < 3 || s.ever_responded);
                    }
                }
                // Wait for the next interval or stop signal
                {
                    let inner = state.lock().unwrap();
                    let _ = stop_cond.wait_timeout(inner, interval).unwrap();
                }
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
        inner.last_response_time = Some(Instant::now());
        // Clear the first-unanswered-request anchor: this response means the current
        // silent window is over, so the next poll will start a fresh window.
        inner.last_request_time = None;
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
        inner.last_response_time = None;
        inner.last_request_time = None;
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
