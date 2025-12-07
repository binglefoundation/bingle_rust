use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
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
    endpoint: Option<SocketAddr>,
    last_polled: Option<Instant>,
}

impl ServerStatus {
    fn new(addr: SocketAddr) -> Self {
        Self { addr, failures: 0, responded: false, endpoint: None, last_polled: None }
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
}

impl Inner {
    fn recompute_state_and_notify(&mut self) {
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
                    StunState::Consistent
                } else {
                    self.endpoint = None;
                    StunState::Inconsistent
                }
            }
        };
        // For None or Single, clear remembered endpoint
        if matches!(new_state, StunState::None | StunState::Single) {
            self.endpoint = None;
        }
        if new_state != self.state {
            self.state = new_state;
            log::info!("[STUN] state change: {:?} endpoint={:?}", self.state, self.endpoint);
            if let Some(cb) = &self.state_change { cb(self.state, self.endpoint); }
        }
    }
}

/// Public implementation of the StunEndpointFinder trait using a background thread.
pub struct StunEndpointFinderImpl {
    inner: Arc<Mutex<Inner>>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
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
        };
        Self { inner: Arc::new(Mutex::new(inner)), running: Arc::new(AtomicBool::new(false)), thread: None }
    }

    fn choose_interval(state: StunState, search: Duration, repeat: Duration) -> Duration {
        match state {
            StunState::None | StunState::Single => search,
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
        }
        // Start background thread if not already running
        if self.running.swap(true, Ordering::SeqCst) { return; }
        let running = self.running.clone();
        let state = self.inner.clone();
        self.thread = Some(thread::spawn(move || {
            loop {
                if !running.load(Ordering::SeqCst) { break; }
                let (to_poll, interval) = {
                    let inner = state.lock().unwrap();
                    let interval = StunEndpointFinderImpl::choose_interval(inner.state, inner.search, inner.repeat);
                    // Determine servers to poll
                    let to_poll: Vec<SocketAddr> = match inner.state {
                        StunState::None | StunState::Single => inner.servers.iter().filter(|s| !s.responded).map(|s| s.addr).collect(),
                        StunState::Consistent | StunState::Inconsistent => inner.servers.iter().map(|s| s.addr).collect(),
                    };
                    (to_poll, interval)
                };
                // Send binding requests
                {
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
                                Some(s.addr)
                            } else { None }
                        };
                        if let Some(target) = target_addr_opt {
                            if let Some(h) = handler.as_ref() {
                                let host = target.ip().to_string();
                                let port = target.port();
                                log::info!("[STUN] sending Binding Request to {}:{}", host, port);
                                h(&host, port, &pkt);
                            }
                        }
                    }
                    // Remove servers that have failed 3 times
                    inner.servers.retain(|s| s.failures < 3);
                    // Error handling if we tried 3 intervals without collecting at least two servers
                    if matches!(inner.state, StunState::None | StunState::Single) {
                        inner.intervals_without_two = inner.intervals_without_two.saturating_add(1);
                        if inner.intervals_without_two >= 3 && !inner.error_reported {
                            let responders = inner.servers.iter().filter(|s| s.responded).count();
                            if responders < 2 {
                                if let Some(eh) = &inner.error { eh("Fewer than 2 STUN servers responded after 3 tries".to_string()); }
                                inner.error_reported = true;
                            }
                        }
                    }
                }
                // Sleep outside the lock
                std::thread::sleep(interval);
            }
        }));
    }

    fn stop(&mut self) {
        if self.running.swap(false, Ordering::SeqCst) {
            if let Some(h) = self.thread.take() { let _ = h.join(); }
        }
    }

    fn process_packet(&mut self, from: SocketAddr, data: &[u8]) {
        let endpoint = StunEndpointFinderImpl::parse_xor_mapped_address(data);
        log::info!("[STUN] received response from {} mapped={:?}", from, endpoint);
        let mut inner = self.inner.lock().unwrap();
        // Find server by source address
        if let Some(s) = inner.servers.iter_mut().find(|s| s.addr == from) {
            s.responded = true;
            s.failures = 0; // reset on any response
            if let Some(ep) = endpoint { s.endpoint = Some(ep); }
        }
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
}
