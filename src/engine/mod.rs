use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::api::bingle_api::{BingleApi, NetworkSourceKey, StartOptions, UserId};
use crate::dtls::{Dtls, NetworkMux, UdpNetworkMux};
use crate::messages::handlers::MessageHandler;
use crate::messages::types::{Message, RelayMessage, RelayTriangleTest1};
use crate::messages::{from_json_str, route, DefaultPrintingHandler};
use crate::relay::relay_finder::{RelayFinder, RootRelayInfo};
use crate::stun::endpoint_finder::StunEndpointFinder;
use crate::stun::endpoint_finder_impl::StunEndpointFinderImpl;
use base64::Engine as _;
use uuid::Uuid;

#[derive(Debug, Default)]
struct ResponseWait {
    responded: bool,
    response: Option<serde_json::Value>,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    StunIdentify,
    TrianglePing,
    EndpointAvailable,
    NATRestricted
}

/// Minimal Engine implementation that wires UDP mux + DTLS and routes inbound JSON messages.
pub struct Engine {
    options: StartOptions,
    mux: Option<Arc<UdpNetworkMux>>, // concrete to access start/stop helpers
    // Underlying DTLS listener; per-connection adapters delegate to this
    dtls: Option<Box<dyn Dtls + Send + Sync>>,
    state: EngineState,
    last_public_addr: Option<SocketAddr>,
    stun: Option<Arc<Mutex<Box<dyn StunEndpointFinder + Send + Sync>>>>, // background STUN
    relay_finder: Option<Arc<RelayFinder>>, // used to locate peer relay
    triangle_wait: Option<(Arc<(Mutex<bool>, Condvar)>, Instant)>, // wait for TriangleTest3
    // Callback to send messages via the Bingle protocol (API surface) instead of direct DTLS
    send_via_bingle: Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync>>,
    // BingleApi handle for handlers (e.g., RelayFinder) to use a real API bound to this engine instance
    bingle_api_for_handlers: Option<Arc<dyn BingleApi>>,
    // Back-reference to creating BingleApiImpl instance (non-global) for inbound dispatch
    api_ptr: Option<Arc<std::sync::atomic::AtomicPtr<crate::api::bingle_api_impl::BingleApiImpl>>>,
    // Async readiness flag: once set, engine_state_for_tests should report EndpointAvailable
    endpoint_ready: std::sync::atomic::AtomicBool,
    // Flag indicating NAT restricted state when endpoint is not yet available
    nat_restricted: std::sync::atomic::AtomicBool,
    // Per-connection state tracked at the Engine level (keyed by remote SocketAddr)
    connections: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<SocketAddr, ConnectionEntry>>>,
    // Pending responses map and issuer state moved from BingleApiImpl
    pending_responses: Arc<Mutex<HashMap<Uuid, Arc<(Mutex<ResponseWait>, Condvar)>>>>,
    issuer: Option<String>,
}

// Per-connection state holding a DTLS adapter bound to a specific peer
struct ConnectionEntry {
    last_seen: Instant,
}



impl Engine {
    pub fn new(options: StartOptions) -> Self {
        log::info!("[Engine::new] options={:?}", options);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[Engine::new] options={:?}", options)); }
        Self {
            options: options.clone(),
            mux: None,
            dtls: None,
            state: EngineState::StunIdentify,
            last_public_addr: None,
            stun: None,
            relay_finder: None,
            triangle_wait: None,
            send_via_bingle: None,
            bingle_api_for_handlers: None,
            api_ptr: None,
            endpoint_ready: std::sync::atomic::AtomicBool::new(false),
            nat_restricted: std::sync::atomic::AtomicBool::new(false),
            connections: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            pending_responses: Arc::new(Mutex::new(HashMap::new())),
            issuer: None,
        }
    }

    /// Provide a pre-configured DTLS instance (with server certificate material) from the API layer.
    pub fn set_dtls(&mut self, dtls: Box<dyn Dtls + Send + Sync>) {
        self.dtls = Some(dtls);
    }

    /// Access the configured DTLS instance, if any (read-only).
    pub fn dtls(&self) -> Option<&(dyn Dtls + Send + Sync)> {
        self.dtls.as_deref()
    }

    /// Test helper: get the local UDP bind address of the mux, if started.
    pub fn local_bind_addr_for_tests(&self) -> Option<SocketAddr> {
        if let Some(m) = &self.mux { m.local_addr().ok() } else { None }
    }

    /// Apply a closure to the DTLS instance if configured.
    pub fn with_dtls_mut<F: FnOnce(&mut (dyn Dtls + Send + Sync))>(&mut self, f: F) {
        if let Some(d) = self.dtls.as_deref_mut() { f(d); }
    }

    /// Set and get issuer moved from API layer.
    pub fn set_issuer(&mut self, issuer: String) { self.issuer = Some(issuer); }
    pub fn issuer(&self) -> Option<&str> { self.issuer.as_deref() }

    /// Pending response registration/fulfillment helpers
    pub fn register_pending(&self, tag: Uuid) {
        let pair = Arc::new((Mutex::new(ResponseWait::default()), Condvar::new()));
        if let Ok(mut m) = self.pending_responses.lock() { m.insert(tag, pair); }
    }
    pub fn fulfill_pending(&self, tag: &Uuid, response: serde_json::Value) -> bool {
        let pair_opt = {
            match self.pending_responses.lock() { Ok(m) => m.get(tag).cloned(), Err(_) => None }
        };
        if let Some(pair) = pair_opt {
            let (lock, cvar) = (&pair.0, &pair.1);
            if let Ok(mut g) = lock.lock() { g.responded = true; g.response = Some(response); cvar.notify_all(); }
            true
        } else { false }
    }
    pub fn wait_for_response(&self, tag: &Uuid, timeout: Duration) -> Option<serde_json::Value> {
        let pair_opt = {
            match self.pending_responses.lock() { Ok(m) => m.get(tag).cloned(), Err(_) => None }
        };
        if let Some(pair) = pair_opt {
            let (lock, cvar) = (&pair.0, &pair.1);
            if let Ok(mut g) = lock.lock() {
                let start = Instant::now();
                loop {
                    if g.responded { break; }
                    let remaining = timeout.saturating_sub(start.elapsed());
                    if remaining.is_zero() { break; }
                    let (gg, res) = cvar.wait_timeout(g, remaining).expect("condvar wait failed");
                    g = gg;
                    if res.timed_out() && !g.responded { break; }
                }
                let out = if g.responded { g.response.take() } else { None };
                drop(g);
                // cleanup
                if let Ok(mut m) = self.pending_responses.lock() { m.remove(tag); }
                out
            } else { None }
        } else { None }
    }
    pub fn remove_pending(&self, tag: &Uuid) -> bool {
        if let Ok(mut m) = self.pending_responses.lock() { m.remove(tag).is_some() } else { false }
    }

    /// Install a Bingle protocol sender callback for Engine-initiated messages.
    pub fn set_send_via_bingle(&mut self, cb: Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync>>) {
        self.send_via_bingle = cb;
    }

    /// Provide a BingleApi handle to be used by handlers and relay discovery bound to this Engine instance.
    pub fn set_bingle_api_for_handlers(&mut self, api: Arc<dyn BingleApi>) {
        self.bingle_api_for_handlers = Some(api);
    }

    /// Set back-reference to the creating BingleApiImpl.
    pub fn set_api_ptr(&mut self, ptr: Arc<std::sync::atomic::AtomicPtr<crate::api::bingle_api_impl::BingleApiImpl>>) {
        self.api_ptr = Some(ptr);
    }

    /// Register a connection in the engine's per-connection registry.
    fn register_connection(&mut self, addr: SocketAddr) {
        if let Ok(mut m) = self.connections.lock() {
            if let Some(entry) = m.get_mut(&addr) {
                entry.last_seen = Instant::now();
            } else {
                m.insert(addr, ConnectionEntry { last_seen: Instant::now() });
            }
        }
    }

    /// Check whether the engine believes a connection to addr exists.
    pub fn has_connection(&self, addr: &SocketAddr) -> bool {
        self.connections.lock().map(|m| m.contains_key(addr)).unwrap_or(false)
    }

    /// Testing helper: number of tracked connections.
    pub fn connections_len_for_tests(&self) -> usize {
        self.connections.lock().map(|m| m.len()).unwrap_or(0)
    }

    /// Send bytes to a peer and track the connection's last_seen. Avoid per-connection DTLS adapters to prevent dangling pointers.
    pub fn send_to_peer(&self, addr: SocketAddr, data: &[u8]) -> Result<(), String> {
        // Update or insert connection entry
        {
            let mut map = self.connections.lock().map_err(|_| "connections lock poisoned".to_string())?;
            if let Some(entry) = map.get_mut(&addr) {
                entry.last_seen = Instant::now();
            } else {
                map.insert(addr, ConnectionEntry { last_seen: Instant::now() });
            }
        }
        let dtls = self.dtls.as_ref().ok_or_else(|| "DTLS instance not provided".to_string())?;
        let res = dtls.send(addr, data);
        if res.is_ok() {
            if let Ok(mut m) = self.connections.lock() { if let Some(e) = m.get_mut(&addr) { e.last_seen = Instant::now(); } }
        }
        res
    }

    /// Start the engine using the provided StartOptions.
    /// Implements static endpoint path or STUN-based discovery when not provided.
    pub fn start(&mut self, options: StartOptions) -> Result<(), String> {
        // Keep a copy of options
        self.options = options.clone();

        if let Some(static_addr) = options.static_ip {
            return self.start_with_addr(options, static_addr);
        }

        // STUN path
        self.state = EngineState::StunIdentify;

        // Bind UDP on 0.0.0.0:0 and create mux (OS assigns an ephemeral port)
        let mut mux0 = UdpNetworkMux::bind("0.0.0.0:0").map_err(|e| format!("Failed to bind UDP mux: {}", e))?;
        let _local_addr: SocketAddr = mux0.local_addr().map_err(|e| format!("Failed to get local addr: {}", e))?;

        // Create an AtomicPtr to this Engine for cross-thread STUN callbacks (see handler below)
        let self_ptr = std::sync::atomic::AtomicPtr::new(self as *mut Engine);

        // Use the pre-configured DTLS instance provided by the API and install message handler
        let dtls = self.dtls.as_mut().ok_or_else(|| "DTLS instance not provided".to_string())?;
        // We'll detect RelayTriangleTest3 to unblock waiters while still routing to default
        let triangle_signal: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
        let _triangle_signal_clone = triangle_signal.clone();
        let existing = dtls.get_handle_message();
        // Create another atomic pointer for use inside the closure
        let self_ptr2 = std::sync::atomic::AtomicPtr::new(self_ptr.load(std::sync::atomic::Ordering::SeqCst));
        dtls.set_handle_message(Some(Arc::new(move |server, from, issuer, data| {
            use std::sync::atomic::Ordering;
            // Register/refresh connection for this peer on any inbound data and refresh message sender binding
            let p = self_ptr2.load(Ordering::SeqCst);
            if !p.is_null() {
                unsafe {
                    let eng = &mut *p;
                    eng.register_connection(*from);
                    // Bind per-message sender so handlers send via the correct API/engine instance
                    if let Some(cb) = &eng.send_via_bingle {
                        crate::messages::router::set_sender(Some(cb.clone()));
                    }
                    // Bind the per-engine BingleApi to the router for this message context
                    if let Some(api) = &eng.bingle_api_for_handlers {
                        crate::messages::router::set_bingle_api(Some(api.clone()));
                    }
                }
            }
            // Route to engine default handler
            let p = self_ptr2.load(std::sync::atomic::Ordering::SeqCst);
            if !p.is_null() {
                unsafe { (&*p).handle_dtls_message(server, from, issuer, data); }
            }
            // Then delegate to any previously-registered handler from API
            if let Some(h) = &existing { h(server, from, issuer, data); }
        })));

        // Install STUN endpoint finder and hook into mux STUN handler
        let finder: Arc<Mutex<Box<dyn StunEndpointFinder + Send + Sync>>> = Arc::new(Mutex::new(Box::new(StunEndpointFinderImpl::new())));
        // Hook STUN packets directly to the finder via a capturing closure
        let finder_for_stun = finder.clone();
        mux0.set_handle_stun(Some(Arc::new(move |source: &dyn NetworkMux, from: &SocketAddr, data: &[u8]| {
            let _ = source.as_any(); // silence unused param warning
            if let Ok(mut guard) = finder_for_stun.lock() {
                guard.process_packet(*from, data);
            }
        })));

        // Now wrap mux in Arc
        let mux = Arc::new(mux0);

        // Start mux thread first so DTLS accept loop can receive
        mux.start().map_err(|e| format!("Failed to start UDP mux: {}", e))?;

        // Start DTLS with mux so that we can send/receive triangle messages over DTLS if needed later
        dtls.start(mux.clone())
            .map_err(|e| format!("Failed to start DTLS: {}", e))?;

        // Persist mux, STUN finder, and triangle wait handle before initializing STUN
        self.mux = Some(mux.clone());
        self.stun = Some(finder.clone());
        // Store triangle wait handle for later awaits
        self.triangle_wait = Some((triangle_signal, Instant::now()));

        // After DTLS and mux are running, configure and start STUN finder logic
        self.start_stun_find(&options, &finder, &mux)?;
        Ok(())
    }

    fn start_with_addr(&mut self, _options: StartOptions, bind_addr: SocketAddr) -> Result<(), String> {
        // Always bind UDP to 0.0.0.0:<port> so that we listen on all interfaces, even when a static external IP is configured.
        // The static address is used for signaling and routing outside any firewall, not for local bind.
        let port = bind_addr.port();
        let bind_all = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
        log::info!("[Engine] start_with_addr: requested={:?} binding={:?}", bind_addr, bind_all);
        log::warn!("[Engine] start_with_addr: requested={:?} binding={:?}", bind_addr, bind_all);
        let mux = Arc::new(UdpNetworkMux::bind(bind_all).map_err(|e| format!("Failed to bind UDP mux: {}", e))?);
        // Determine the concrete local address after bind (handles port 0)
        let _local_addr: SocketAddr = mux.local_addr().map_err(|e| format!("Failed to get local addr: {}", e))?;

        // Create self pointer for registration from closure
        let self_ptr = std::sync::atomic::AtomicPtr::new(self as *mut Engine);
        // Use pre-configured DTLS from API; install engine handler while preserving any existing API handler.
        let dtls = self.dtls.as_mut().ok_or_else(|| "DTLS instance not provided".to_string())?;
        let existing = dtls.get_handle_message();
        dtls.set_handle_message(Some(Arc::new(move |server, from, issuer, data| {
            use std::sync::atomic::Ordering;
            // Register inbound connection
            let p = self_ptr.load(Ordering::SeqCst);
            if !p.is_null() {
                unsafe {
                    let eng = &mut *p;
                    eng.register_connection(*from);
                    // Bind per-message sender for this engine instance
                    if let Some(cb) = &eng.send_via_bingle {
                        crate::messages::router::set_sender(Some(cb.clone()));
                    }
                    // Bind per-engine api
                    if let Some(api) = &eng.bingle_api_for_handlers {
                        crate::messages::router::set_bingle_api(Some(api.clone()));
                    }
                }
            }
            // Route to engine default handler
            let p = self_ptr.load(std::sync::atomic::Ordering::SeqCst);
            if !p.is_null() {
                unsafe { (&*p).handle_dtls_message(server, from, issuer, data); }
            }
            if let Some(h) = &existing { h(server, from, issuer, data); }
        })));

        // Start the UDP mux background loop first
        mux.start().map_err(|_| "Failed to start UDP mux")?;

        // Start DTLS accept loop with the mux
        dtls.start(mux.clone())
            .map_err(|e| format!("Failed to start DTLS: {}", e))?;

        self.mux = Some(mux);
        Ok(())
    }

    fn on_stun_consistent(&mut self, public_addr: Option<SocketAddr>) {
        // Spawn a worker thread to process STUN-consistent follow-up to avoid blocking inbound packet path
        let self_ptr = std::sync::atomic::AtomicPtr::new(self as *mut Engine);
        std::thread::spawn(move || {
            unsafe {
                let eng = &mut *self_ptr.load(std::sync::atomic::Ordering::SeqCst);
                eng.stun_consistent_process(public_addr);
            }
        });
    }

    fn stun_consistent_process(&mut self, public_addr: Option<SocketAddr>) {
        log::info!("[Engine] on_stun_consistent: public_addr={:?}", public_addr);
        // Save last known public address (for validation/tests)
        self.last_public_addr = public_addr;

        // Transition to TrianglePing and perform relay triangle test
        let prev = self.state;
        self.state = EngineState::TrianglePing;
        log::info!("[Engine] state change: {:?} -> TrianglePing", prev);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[Engine] state change: {:?} -> TrianglePing", prev)); }

        // Do NOT mark EndpointAvailable here; proceed with the triangle process and only
        // transition to EndpointAvailable once TriangleTest3 is observed.
        if self.dtls.is_none() {
            panic!("DTLS not started: cannot proceed with triangle ping after STUN consistent");
        }

        // Create/use a RelayFinder and use find_relay to obtain our relay address.
        // For now, discovery is stubbed to the provided public_addr (if any) and RelayCheck always returns available.
        let mut relay_target: Option<RootRelayInfo> = None;
        if let Some(addr) = public_addr {
            let _a2 = addr.clone();
            // Use the real BingleApi provided via router
            let api_opt = self.bingle_api_for_handlers.clone().or_else(|| crate::messages::router::get_bingle_api());
            if api_opt.is_none() { panic!("[Engine] No BingleApi available for relay check"); }

            // Use Indexer-based discovery when available via AlgoBingle::list_static_endpoints_via_indexer
            // Prefer app_id from StartOptions; fallback to env var for legacy tests; else use built-in localhost relays.
            let discover: Arc<dyn Fn() -> Vec<RootRelayInfo> + Send + Sync> = {
                #[cfg(not(target_os = "ios"))]
                {
                    // Capture app_id and provider config from options
                    let opt_app_id = self.options.app_id;
                    let opt_cfg = self.options.algo_provider_config.clone();
                    let app_id_opt = opt_app_id.or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
                    log::info!("[Engine] indexer discovery app_id={:?}", app_id_opt);
                    if let Some(app_id) = app_id_opt {
                        let ab = {
                            let ops = crate::blockchain::algo_ops::AlgoOps::new(None, None, opt_cfg);
                            crate::blockchain::algo_bingle::AlgoBingle::new(ops, app_id, 0)
                        };
                        Arc::new(move || {
                            match ab.list_static_endpoints_via_indexer(app_id) {
                                Ok(list) => {
                                    let mut out: Vec<RootRelayInfo> = Vec::new();
                                    for (id, ep) in list {
                                        if let Some(addr) = crate::blockchain::algo_bingle::AlgoBingle::parse_relay_ip(&ep) {
                                            out.push(RootRelayInfo { id, address: addr });
                                        }
                                    }
                                    if out.is_empty() {
                                        panic!("[Engine] indexer discovery returned empty static endpoints list");
                                    } else { out }
                                }
                                Err(e) => {
                                    panic!("[Engine] indexer discovery failed: {}", e);
                                }
                            }
                        })
                    } else {
                        // No app id set
                        panic!("[Engine] indexer discovery has no app id");
                    }
                }
                #[cfg(target_os = "ios")]
                {
                    Arc::new(|| vec![RootRelayInfo { id: "IOS-DUMMY".to_string(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345) }])
                }
            };

            let finder = RelayFinder::new(api_opt.unwrap(), Duration::from_secs(60), discover);
            // Use our id (Algorand address) for relay selection, not the user-visible handle.
            // Prefer the issuer set earlier by BingleApiImpl::start (issuer = id + ISSUER_SUFFIX).
            let my_id: String = if let Some(iss) = self.issuer.as_deref() {
                iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string()
            } else {
                // Fallback: if issuer is not set, use the handle (best-effort; may yield suboptimal selection).
                self.options.handle.clone()
            };
            let relay = finder.find_relay(&my_id);
            if let Ok(r) = relay {
                relay_target = Some(r.clone());
                log::info!("[Engine] chosen relay {} (id={})", r.address, r.id);
            }
            else {
                panic!("[Engine] no relay found");
            }
            self.relay_finder = Some(Arc::new(finder));
        }

        // Send TriangleTest1 to the discovered relay using the Bingle API callback if installed
        if let Some(target) = relay_target {
            let to_addr = target.address;
            let checking_ep = public_addr.unwrap_or(to_addr);
            let msg = Message::Relay(RelayMessage::TriangleTest1(RelayTriangleTest1 { app: None, checking_endpoint: checking_ep }));
            let nsk = NetworkSourceKey::new_direct(to_addr);
            // Build JSON value for the message
            let json_val = crate::messages::marshal::to_json_value(&msg);
            if let Some(cb) = &self.send_via_bingle {
                // Use the relay's actual id as the user id. Convert Algorand base32 address to base64(36) for API validation.
                let uid = match data_encoding::BASE32_NOPAD.decode(target.id.as_bytes()) {
                    Ok(bytes) if bytes.len() == 36 => base64::engine::general_purpose::STANDARD.encode(bytes),
                    Ok(bytes) => {
                        log::info!("[Engine][WARN] relay id base32 decoded to {} bytes (expected 36); using raw id which may fail validation", bytes.len());
                        target.id.clone()
                    }
                    Err(e) => {
                        log::info!("[Engine][WARN] failed to decode relay id as base32: {}; using raw id which may fail validation", e);
                        target.id.clone()
                    }
                };
                let ok = cb(&nsk, &uid, json_val);
                log::info!("[Engine] TriangleTest1 send_via_bingle to {} (uid from relay id) -> {}", to_addr, ok);
                #[allow(unused)] { crate::util::logging::log_line(&format!("[Engine] TriangleTest1 send_via_bingle to {} (uid from relay id) -> {}", to_addr, ok)); }
            } else {
                log::info!("[Engine][WARN] send_via_bingle not installed; cannot send TriangleTest1 to {}", to_addr);
                #[allow(unused)] { crate::util::logging::log_line(&format!("[Engine][WARN] send_via_bingle not installed; cannot send TriangleTest1 to {}", to_addr)); }
            }
        } else {
            log::info!("[Engine][WARN] TrianglePing path active but no destination to send TriangleTest1");
            panic!("[Engine][WARN] TrianglePing path active but no destination to send TriangleTest1");
        }
    }

    fn on_stun_inconsistent(&mut self) {
        panic!("NotImplemented: STUN reported Inconsistent public endpoint");
    }

    /// Configure STUN send/state handlers and start the finder after DTLS and mux are running.
    fn start_stun_find(
        &mut self,
        options: &StartOptions,
        finder: &Arc<Mutex<Box<dyn StunEndpointFinder + Send + Sync>>>,
        mux: &Arc<UdpNetworkMux>,
    ) -> Result<(), String> {
        // Create a self pointer for callbacks invoked from STUN worker thread
        let self_ptr = std::sync::atomic::AtomicPtr::new(self as *mut Engine);
        if let Ok(mut f) = finder.lock() {
            // Route STUN outbound packets through the UDP mux
            let mux_clone = mux.clone();
            f.set_send_packet_handler(Some(Arc::new(move |host, port, payload| {
                mux_clone
                    .write((host, port), payload)
                    .expect("UDP mux write failed in STUN send_packet_handler");
            })));

            // Wire STUN state changes into Engine handlers.
            f.set_state_change_handler(Some(Arc::new(move |st, ep| {
                let p = self_ptr.load(std::sync::atomic::Ordering::SeqCst);
                if p.is_null() { return; }
                unsafe {
                    if st == crate::stun::endpoint_finder::StunState::Consistent {
                        (&mut *p).on_stun_consistent(ep);
                    } else if st == crate::stun::endpoint_finder::StunState::Inconsistent {
                        (&mut *p).on_stun_inconsistent();
                    }
                }
            })));

            // Kick off STUN polling using provided servers
            let servers = options.stun_servers.clone().unwrap_or_default();
            if servers.is_empty() {
                return Err("No STUN servers provided".into());
            }
            f.start(servers, 2_000, 60_000);
        }
        Ok(())
    }

    /// Stop the engine and background tasks if started.
    pub fn stop(&mut self) {
        if let Some(dtls) = &mut self.dtls {
            dtls.stop().expect("DTLS stop failed in Engine::stop");
        }
        if let Some(mux) = &self.mux {
            mux.stop();
        }
        if let Some(stun_arc) = &self.stun {
            if let Ok(mut finder) = stun_arc.lock() {
                finder.stop();
            }
        }
        self.dtls = None;
        self.mux = None;
        self.stun = None;
    }

    pub fn state(&self) -> EngineState {
        use std::sync::atomic::Ordering;
        if self.endpoint_ready.load(Ordering::SeqCst) {
            EngineState::EndpointAvailable
        } else if self.nat_restricted.load(Ordering::SeqCst) {
            EngineState::NATRestricted
        } else {
            self.state
        }
    }
    pub fn last_public_addr(&self) -> Option<SocketAddr> { self.last_public_addr }
    pub fn test_force_stun_consistent(&mut self, addr: SocketAddr) { self.on_stun_consistent(Some(addr)); }

    /// Internal setter used by BingleApiInternal to update engine state in a thread-safe way.
    /// Currently supports transitioning to EndpointAvailable.
    pub fn set_state_internal(&self, new_state: EngineState) -> bool {
        use std::sync::atomic::Ordering;
        match new_state {
            EngineState::EndpointAvailable => {
                self.endpoint_ready.store(true, Ordering::SeqCst);
                true
            }
            EngineState::NATRestricted => {
                // Only meaningful if endpoint is not yet available
                if !self.endpoint_ready.load(Ordering::SeqCst) {
                    self.nat_restricted.store(true, Ordering::SeqCst);
                    return true;
                }
                false
            }
            _ => {
                // Other transitions not supported concurrently
                false
            }
        }
    }

    /// DTLS message handler: try to interpret payload as UTF-8 JSON and route.
    fn handle_dtls_message(&self, _server: &dyn Dtls, from: &SocketAddr, issuer: &str, data: &[u8]) {
        // Debug: log inbound DTLS application message (best-effort UTF-8 preview)
        let preview = match std::str::from_utf8(data) {
            Ok(s) => {
                let trimmed = if s.len() > 120 { &s[..120] } else { s };
                format!("utf8:{} bytes: {}", s.len(), trimmed)
            }
            Err(_) => format!("non-utf8:{} bytes", data.len()),
        };
        log::info!("[Engine::handle_dtls_message] from={} issuer={} {}", from, issuer, preview);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[Engine::handle_dtls_message] from={} issuer={} {}", from, issuer, preview)); }

        // Record last sender address for handlers that need to reply directly
        crate::messages::router::set_last_from(Some(*from));

        // Try to parse as JSON first to handle tagged responses and set router hints
        if let Ok(s) = std::str::from_utf8(data) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                // Expose last responseTag (if this is a request). Handlers may echo it back.
                if let Some(tag) = v.get("responseTag").and_then(|vv| vv.as_str()) {
                    crate::messages::router::set_last_response_tag(Some(tag.to_string()));
                } else {
                    crate::messages::router::set_last_response_tag(None);
                }
                // If this is a response, fulfill any waiter registered with Engine (supports both keys)
                let tag_str_opt = v.get("responseTag").and_then(|vv| vv.as_str())
                    .or_else(|| v.get("tag").and_then(|vv| vv.as_str()));
                if let Some(tag_str) = tag_str_opt {
                    if let Ok(tag_uuid) = uuid::Uuid::parse_str(tag_str) {
                        if self.fulfill_pending(&tag_uuid, v.clone()) {
                            // Consumed by waiter; do not forward to API on_message
                            return;
                        }
                    }
                }
                // Not a tagged response we were waiting for: forward to API on_message if available
                if let Some(ptr) = &self.api_ptr {
                    use std::sync::atomic::Ordering;
                    let p = ptr.load(Ordering::SeqCst);
                    if !p.is_null() {
                        unsafe { (&*p).handle_incoming_network_message(issuer.to_string(), from.to_string(), v.clone()); }
                    }
                }
            }
        }

        // Route through the message framework for internal handlers (triangle tests etc.)
        let handler = DefaultPrintingHandler;
        match std::str::from_utf8(data) {
            Ok(s) => match from_json_str(s) {
                Ok(msg) => route(&handler, &msg, issuer),
                Err(_) => {
                    // Not valid JSON per our schema; treat as plaintext with raw bytes
                    handler.on_unimplemented(&crate::messages::Message::Unknown(serde_json::Value::String(s.to_string())));
                }
            },
            Err(_) => {
                handler.on_unimplemented(&crate::messages::Message::Unknown(serde_json::Value::Null));
            }
        }
    }
}
