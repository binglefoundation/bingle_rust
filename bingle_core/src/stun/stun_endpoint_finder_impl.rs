use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// The STUN magic cookie (RFC 8489 §5), also the mask for XOR-MAPPED-ADDRESS ports and addresses.
pub const STUN_MAGIC_COOKIE: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

// Bring the public trait/types from our module
use super::stun_endpoint_finder::{
    AddrFamily, ErrorHandler, SendPacketHandler, StateChangeHandler, StunEndpointFinder, StunState,
};

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
        Self {
            addr,
            failures: 0,
            responded: false,
            ever_responded: false,
            endpoint: None,
            last_polled_tick: None,
        }
    }
}

/// The state and endpoint of one address family (issue #498).
#[derive(Clone, Copy, PartialEq, Eq)]
struct FamilyState {
    state: StunState,
    endpoint: Option<SocketAddr>,
}

impl FamilyState {
    const NONE: FamilyState = FamilyState {
        state: StunState::None,
        endpoint: None,
    };
    const BLOCKED: FamilyState = FamilyState {
        state: StunState::Blocked,
        endpoint: None,
    };

    /// The state of one family given the mapped endpoints its servers reported.
    fn from_endpoints(endpoints: &[SocketAddr]) -> Self {
        match endpoints {
            [] => FamilyState::NONE,
            [only] => FamilyState {
                state: StunState::Single,
                endpoint: Some(*only),
            },
            [first, rest @ ..] if rest.iter().all(|e| e == first) => FamilyState {
                state: StunState::Consistent,
                endpoint: Some(*first),
            },
            _ => FamilyState {
                state: StunState::Inconsistent,
                endpoint: None,
            },
        }
    }

    /// Whether the family has had an answer since the last reset: Single, Consistent or
    /// Inconsistent.
    fn is_settled(&self) -> bool {
        matches!(
            self.state,
            StunState::Single | StunState::Consistent | StunState::Inconsistent
        )
    }

    /// Rank used to combine the families into the polling state: the more settled family wins.
    fn rank(&self) -> u8 {
        match self.state {
            StunState::Blocked => 0,
            StunState::None => 1,
            StunState::Single => 2,
            StunState::Inconsistent => 3,
            StunState::Consistent => 4,
        }
    }
}

/// What a recompute of the family states may conclude (issue #549).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Round {
    /// A reply arrived during a re-poll round. A re-poll clears every server's answer before
    /// sending, so a family's missing answers only mean its replies have not arrived yet: a settled
    /// family keeps its state until the round holds at least two of its answers.
    InProgress,
    /// The round is over (the next poll, or the silence timeout): each family's state is what its
    /// servers answered, so a family with no answers goes to None.
    Complete,
}

struct Inner {
    servers: Vec<ServerStatus>,
    // Combined state of both families; it drives the polling interval, the silence timeout and the
    // blocked check, which are about the UDP path as a whole rather than one family.
    state: StunState,
    v4: FamilyState,
    v6: FamilyState,
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
                    self.no_response_ticks,
                    self.state
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
                self.recompute_state_and_notify(Round::Complete);
            }
        }

        // 2. Decide if it is time to poll based on the current state's interval
        let interval_ticks = self.choose_interval();

        if self
            .last_poll_tick
            .is_none_or(|last| now.saturating_sub(last) >= interval_ticks)
        {
            self.last_poll_tick = Some(now);

            // Close the re-poll round that ends here before this poll clears its answers: a family
            // that did not answer during the whole round goes to None now (issue #549).
            if matches!(self.state, StunState::Consistent | StunState::Inconsistent) {
                self.recompute_state_and_notify(Round::Complete);
            }

            // Determine servers to poll
            let to_poll: Vec<SocketAddr> = match self.state {
                StunState::None | StunState::Single => self
                    .servers
                    .iter()
                    .filter(|s| !s.responded)
                    .map(|s| s.addr)
                    .collect(),
                StunState::Blocked | StunState::Consistent | StunState::Inconsistent => {
                    self.servers.iter().map(|s| s.addr).collect()
                }
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
                if matches!(
                    self.state,
                    StunState::None | StunState::Single | StunState::Blocked
                ) {
                    self.intervals_without_two = self.intervals_without_two.saturating_add(1);
                    if self.intervals_without_two >= 3 {
                        let responders = self.servers.iter().filter(|s| s.responded).count();
                        if responders == 0 {
                            if self.state != StunState::Blocked {
                                tracing::warn!(
                                    "[STUN] UDP appears blocked (no responses from {} servers after 3 intervals)",
                                    self.servers.len()
                                );
                                self.state = StunState::Blocked;
                                self.v4 = FamilyState::BLOCKED;
                                self.v6 = FamilyState::BLOCKED;
                                if let Some(cb) = &self.state_change {
                                    cb(AddrFamily::V4, StunState::Blocked, None);
                                    cb(AddrFamily::V6, StunState::Blocked, None);
                                }
                            } else {
                                tracing::info!(
                                    "[STUN] still blocked (UDP appears blocked) - continuing to poll for recovery"
                                );
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

    /// Recompute each family's state from the servers' latest mapped endpoints, and notify the
    /// families whose state changed, or whose endpoint changed (a NAT port change both servers
    /// agree on in the same round keeps the state Consistent but must still be reported).
    ///
    /// With `Round::InProgress` during a re-poll, a settled family with fewer than two answers so
    /// far keeps its state: otherwise the first reply of a round, from the other family, would
    /// report this family as None (issue #549).
    fn recompute_state_and_notify(&mut self, round: Round) {
        let repolling = matches!(self.state, StunState::Consistent | StunState::Inconsistent);
        let mut changed = Vec::new();
        for family in [AddrFamily::V4, AddrFamily::V6] {
            let endpoints: Vec<SocketAddr> = self
                .servers
                .iter()
                .filter_map(|s| s.endpoint)
                .filter(|e| AddrFamily::of(e) == family)
                .collect();
            let slot = match family {
                AddrFamily::V4 => &mut self.v4,
                AddrFamily::V6 => &mut self.v6,
            };
            if round == Round::InProgress && repolling && slot.is_settled() && endpoints.len() < 2 {
                continue;
            }
            let new = FamilyState::from_endpoints(&endpoints);
            if *slot != new {
                *slot = new;
                changed.push((family, new));
            }
        }
        self.state = if self.v4.rank() >= self.v6.rank() {
            self.v4.state
        } else {
            self.v6.state
        };
        for (family, new) in changed {
            tracing::info!(
                "[STUN] state change: {:?} {:?} endpoint={:?}",
                family,
                new.state,
                new.endpoint
            );
            if let Some(cb) = &self.state_change {
                cb(family, new.state, new.endpoint);
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

impl Default for StunEndpointFinderImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl StunEndpointFinderImpl {
    pub fn new() -> Self {
        let inner = Inner {
            servers: Vec::new(),
            state: StunState::None,
            v4: FamilyState::NONE,
            v6: FamilyState::NONE,
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
        Self {
            inner: Arc::new(Mutex::new(inner)),
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
            stop_cond: Arc::new(Condvar::new()),
            no_response_timeout_override: None,
        }
    }

    /// Override the no-response timeout. Used in tests to avoid waiting 30s.
    pub fn set_no_response_timeout_for_tests(&mut self, timeout: Duration) {
        self.no_response_timeout_override = Some(timeout);
        self.inner.lock().unwrap().no_response_ticks =
            (timeout.as_millis() as u64 / TICK_MS).max(1);
    }

    // Build a minimal STUN Binding Request. Keeping a handcrafted packet for now keeps us
    // decoupled from stun-rs' specific builder API while still having it as a dependency for future use.
    fn build_binding_request(tick: u64) -> [u8; 20] {
        // Simple STUN Binding Request with zero-length attributes.
        // Type: 0x0001, Length: 0x0000, Magic Cookie: 0x2112A442, Transaction ID: 12 bytes pseudo-random
        let mut pkt = [0u8; 20];
        pkt[0] = 0x00;
        pkt[1] = 0x01; // Binding Request
        pkt[2] = 0x00;
        pkt[3] = 0x00; // length
        pkt[4] = 0x21;
        pkt[5] = 0x12;
        pkt[6] = 0xA4;
        pkt[7] = 0x42; // magic cookie
        // Fill transaction ID deterministically from tick
        let mut seed = tick;
        for i in 0..3 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let bytes = seed.to_be_bytes();
            pkt[8 + i * 4..12 + i * 4].copy_from_slice(&bytes[0..4]);
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

    /// Get the current tick count. Used in tests.
    pub fn current_tick_for_test(&self) -> u64 {
        self.inner.lock().unwrap().current_tick
    }

    /// The mapped address in a STUN Binding response: XOR-MAPPED-ADDRESS (0x0020) or, from older
    /// servers, MAPPED-ADDRESS (0x0001), in either address family (RFC 8489 §14.1–14.2). Family 0x01
    /// is IPv4 (8-byte value); family 0x02 is IPv6 (20-byte value), whose XOR-MAPPED form is masked
    /// with the magic cookie followed by the 12-byte transaction ID. Returns `None` for anything that
    /// isn't a well-formed response carrying one of these attributes.
    pub fn parse_xor_mapped_address(data: &[u8]) -> Option<SocketAddr> {
        if data.len() < 20 {
            return None;
        }
        // Verify magic cookie
        if data[4..8] != STUN_MAGIC_COOKIE {
            return None;
        }
        let transaction_id = &data[8..20];
        let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
        let mut idx = 20;
        let end = 20 + msg_len;
        while idx + 4 <= data.len() && idx + 4 <= end {
            let atype = u16::from_be_bytes([data[idx], data[idx + 1]]);
            let alen = u16::from_be_bytes([data[idx + 2], data[idx + 3]]) as usize;
            idx += 4;
            if idx + alen > data.len() {
                break;
            }
            if atype == 0x0020 /* XOR-MAPPED-ADDRESS */ || atype == 0x0001
            /* MAPPED-ADDRESS */
            {
                return Self::decode_mapped_address(
                    &data[idx..idx + alen],
                    atype == 0x0020,
                    transaction_id,
                );
            }
            // Move to next attribute (4-byte alignment)
            let pad = (4 - (alen % 4)) % 4;
            idx += alen + pad;
        }
        None
    }

    /// Decode one (XOR-)MAPPED-ADDRESS attribute value. `None` for an unknown family or a value too
    /// short for its family.
    fn decode_mapped_address(
        value: &[u8],
        xored: bool,
        transaction_id: &[u8],
    ) -> Option<SocketAddr> {
        if value.len() < 4 {
            return None;
        }
        let family = value[1];
        let raw_port = u16::from_be_bytes([value[2], value[3]]);
        let port = if xored { raw_port ^ 0x2112 } else { raw_port };
        match family {
            0x01 => {
                let mut addr: [u8; 4] = value.get(4..8)?.try_into().ok()?;
                if xored {
                    for (b, m) in addr.iter_mut().zip(STUN_MAGIC_COOKIE) {
                        *b ^= m;
                    }
                }
                Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(addr)), port))
            }
            0x02 => {
                let mut addr: [u8; 16] = value.get(4..20)?.try_into().ok()?;
                if xored {
                    let mask = STUN_MAGIC_COOKIE.iter().chain(transaction_id);
                    for (b, m) in addr.iter_mut().zip(mask) {
                        *b ^= *m;
                    }
                }
                Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(addr)), port))
            }
            _ => None,
        }
    }
}

impl StunEndpointFinder for StunEndpointFinderImpl {
    fn init(&mut self, servers: Vec<SocketAddr>, search_time_ms: u64, repeat_time_ms: u64) {
        let mut inner = self.inner.lock().unwrap();
        inner.servers = servers.into_iter().map(ServerStatus::new).collect();
        inner.search_ticks = (search_time_ms / TICK_MS).max(1);
        inner.repeat_ticks = (repeat_time_ms / TICK_MS).max(1);
        inner.state = StunState::None;
        inner.v4 = FamilyState::NONE;
        inner.v6 = FamilyState::NONE;
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

    fn start(&mut self, servers: Vec<SocketAddr>, search_time_ms: u64, repeat_time_ms: u64) {
        // Initialize internal state
        self.init(servers, search_time_ms, repeat_time_ms);

        // Start background thread if not already running
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let running = self.running.clone();
        let state = self.inner.clone();
        let stop_cond = self.stop_cond.clone();
        self.thread = Some(thread::spawn(move || {
            let tick_rate = Duration::from_millis(TICK_MS);
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }

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
                tracing::info!(
                    "[StunEndpointFinder] join on background thread, waiting for it to finish..."
                );
                let _ = h.join();
            } else {
                tracing::warn!(
                    "[StunEndpointFinder] background thread not running, nothing to stop"
                );
            }
        } else {
            tracing::warn!("[StunEndpointFinder] stop called without start");
        }
        tracing::info!("[StunEndpointFinder] stopped background thread");
    }

    fn process_packet(&mut self, from: SocketAddr, data: &[u8]) {
        let endpoint = StunEndpointFinderImpl::parse_xor_mapped_address(data);
        tracing::info!(
            "[STUN] received response from {} mapped={:?}",
            from,
            endpoint
        );
        let mut inner = self.inner.lock().unwrap();
        // Find server by source address
        if let Some(s) = inner.servers.iter_mut().find(|s| s.addr == from) {
            s.responded = true;
            s.ever_responded = true;
            s.failures = 0; // reset on any response
            if let Some(ep) = endpoint {
                s.endpoint = Some(ep);
            }
        }
        // Track when the last response arrived so the poll loop can detect silence.
        inner.last_response_tick = Some(inner.current_tick);
        // Clear the first-unanswered-request anchor: this response means the current
        // silent window is over, so the next poll will start a fresh window.
        inner.last_request_tick = None;
        // Recompute state and notify
        inner.recompute_state_and_notify(Round::InProgress);
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
        tracing::info!(
            "[STUN] reset_state: reverting state {:?} -> None",
            inner.state
        );
        inner.state = StunState::None;
        inner.v4 = FamilyState::NONE;
        inner.v6 = FamilyState::NONE;
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
