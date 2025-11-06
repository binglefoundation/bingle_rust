use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::api::bingle_api::{BingleApi, Handle, NetworkSourceKey, StartOptions, UserId};
use crate::dtls::{Dtls, NetworkMux, UdpNetworkMux};
use crate::messages::handlers::MessageHandler;
use crate::messages::marshal::to_json_string;
use crate::messages::types::{Message, RelayMessage, RelayTriangleTest1};
use crate::messages::{from_json_str, route, DefaultPrintingHandler};
use crate::relay::relay_finder::{RelayFinder, RootRelayInfo};
use crate::stun::endpoint_finder::StunEndpointFinder;
use crate::stun::endpoint_finder_impl::StunEndpointFinderImpl;
use serde_json::json;
use base64::Engine as _;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    StunIdentify,
    TrianglePing,
    EndpointAvailable,
}

/// Minimal Engine implementation that wires UDP mux + DTLS and routes inbound JSON messages.
pub struct Engine {
    options: Option<StartOptions>,
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
    // Async readiness flag: once set, engine_state_for_tests should report EndpointAvailable
    endpoint_ready: std::sync::atomic::AtomicBool,
    // Per-connection state tracked at the Engine level (keyed by remote SocketAddr)
    connections: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<SocketAddr, ConnectionEntry>>>,
}

// Per-connection state holding a DTLS adapter bound to a specific peer
struct ConnectionEntry {
    last_seen: Instant,
    dtls: std::sync::Arc<PeerDtlsAdapter>,
}

// Lightweight per-connection DTLS adapter that delegates to a shared listener but fixes the peer address
struct PeerDtlsAdapter {
    engine_ptr: std::sync::atomic::AtomicPtr<Engine>,
    peer: SocketAddr,
}

impl PeerDtlsAdapter {
    fn new(engine_ptr: std::sync::atomic::AtomicPtr<Engine>, peer: SocketAddr) -> Self {
        Self { engine_ptr, peer }
    }
}

impl Dtls for PeerDtlsAdapter {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> crate::dtls::Result<()> { Ok(()) }
    fn stop(&mut self) -> crate::dtls::Result<()> { Ok(()) }
    fn send(&self, _to: SocketAddr, data: &[u8]) -> crate::dtls::Result<()> {
        use std::sync::atomic::Ordering;
        let p = self.engine_ptr.load(Ordering::SeqCst);
        if p.is_null() { return Err("engine ptr null".to_string()); }
        unsafe {
            let eng = &*p;
            if let Some(dtls) = &eng.dtls { dtls.send(self.peer, data) } else { Err("DTLS instance not provided".to_string()) }
        }
    }
    fn get_handle_message(&self) -> Option<crate::dtls::HandleMessage> { None }
    fn set_handle_message(&mut self, _handler: Option<crate::dtls::HandleMessage>) { }
    fn with_handle_message(self, _handler: crate::dtls::HandleMessage) -> Self where Self: Sized { self }
    fn get_handle_peer_certificate(&self) -> Option<crate::dtls::HandlePeerCertificate> { None }
    fn set_handle_peer_certificate(&mut self, _handler: Option<crate::dtls::HandlePeerCertificate>) { }
    fn with_handle_peer_certificate(self, _handler: crate::dtls::HandlePeerCertificate) -> Self where Self: Sized { self }
    fn get_ca_cert(&self) -> Option<&[u8]> { None }
    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) { }
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_client_cert(&self) -> Option<&[u8]> { None }
    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) { }
    fn with_client_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_client_private_key(&self) -> Option<&[u8]> { None }
    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) { }
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_server_signing_cert(&self) -> Option<&[u8]> { None }
    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) { }
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> { None }
    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) { }
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn set_app_layer_only_verification(&mut self, _enabled: bool) { }
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self where Self: Sized { self }
}


impl Engine {
    pub fn new() -> Self {
        Self {
            options: None,
            mux: None,
            dtls: None,
            state: EngineState::StunIdentify,
            last_public_addr: None,
            stun: None,
            relay_finder: None,
            triangle_wait: None,
            send_via_bingle: None,
            bingle_api_for_handlers: None,
            endpoint_ready: std::sync::atomic::AtomicBool::new(false),
            connections: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
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

    /// Install a Bingle protocol sender callback for Engine-initiated messages.
    pub fn set_send_via_bingle(&mut self, cb: Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync>>) {
        self.send_via_bingle = cb;
    }

    /// Provide a BingleApi handle to be used by handlers and relay discovery bound to this Engine instance.
    pub fn set_bingle_api_for_handlers(&mut self, api: Arc<dyn BingleApi>) {
        self.bingle_api_for_handlers = Some(api);
    }

    /// Register a connection in the engine's per-connection registry.
    fn register_connection(&mut self, addr: SocketAddr) {
        if let Ok(mut m) = self.connections.lock() {
            if let Some(entry) = m.get_mut(&addr) {
                entry.last_seen = Instant::now();
                return;
            }
        }
        // Create outside the lock to avoid double-borrowing self
        let engine_ptr = std::sync::atomic::AtomicPtr::new(self as *mut Engine);
        let adapter = PeerDtlsAdapter::new(engine_ptr, addr);
        if let Ok(mut m) = self.connections.lock() {
            m.insert(addr, ConnectionEntry { last_seen: Instant::now(), dtls: Arc::new(adapter) });
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

    /// Send bytes to a peer, creating the connection adapter if needed, and track it per-connection.
    pub fn send_to_peer(&self, addr: SocketAddr, data: &[u8]) -> Result<(), String> {
        // Lookup or create a per-connection DTLS adapter
        let maybe_adapter = {
            let mut map = self.connections.lock().map_err(|_| "connections lock poisoned".to_string())?;
            if let Some(entry) = map.get(&addr) {
                Some(entry.dtls.clone())
            } else if self.dtls.is_some() {
                // We only have &self here; create a const pointer then cast to mut for storage.
                let engine_ptr = std::sync::atomic::AtomicPtr::new(self as *const Engine as *mut Engine);
                let adapter = Arc::new(PeerDtlsAdapter::new(engine_ptr, addr));
                map.insert(addr, ConnectionEntry { last_seen: Instant::now(), dtls: adapter.clone() });
                Some(adapter)
            } else { None }
        };
        let adapter = maybe_adapter.ok_or_else(|| "DTLS instance not provided".to_string())?;
        let res = adapter.send(addr, data);
        if res.is_ok() {
            if let Ok(mut m) = self.connections.lock() { if let Some(e) = m.get_mut(&addr) { e.last_seen = Instant::now(); } }
        }
        res
    }

    /// Start the engine using the provided StartOptions.
    /// Implements static endpoint path or STUN-based discovery when not provided.
    pub fn start(&mut self, options: StartOptions) -> Result<(), String> {
        // Keep a copy of options
        self.options = Some(options.clone());

        if let Some(static_addr) = options.static_ip {
            return self.start_with_addr(options, static_addr);
        }

        // STUN path
        self.state = EngineState::StunIdentify;

        // Bind UDP on 127.0.0.1:0 and create mux (OS assigns an ephemeral port)
        let mut mux0 = UdpNetworkMux::bind("127.0.0.1:0").map_err(|e| format!("Failed to bind UDP mux: {}", e))?;
        let local_addr: SocketAddr = mux0.local_addr().map_err(|e| format!("Failed to get local addr: {}", e))?;

        // Create an AtomicPtr to this Engine for cross-thread STUN callbacks (see handler below)
        let self_ptr = std::sync::atomic::AtomicPtr::new(self as *mut Engine);

        // Use the pre-configured DTLS instance provided by the API and install message handler
        let dtls = self.dtls.as_mut().ok_or_else(|| "DTLS instance not provided".to_string())?;
        // We'll detect RelayTriangleTest3 to unblock waiters while still routing to default
        let triangle_signal: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
        let triangle_signal_clone = triangle_signal.clone();
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
            // First try to detect TriangleTest3
            if let Ok(s) = std::str::from_utf8(data) {
                if let Ok(msg) = from_json_str(s) {
                    if let Message::Relay(RelayMessage::TriangleTest3(_)) = msg {
                        // signal
                        let (lock, cvar) = (&triangle_signal_clone.0, &triangle_signal_clone.1);
                        if let Ok(mut done) = lock.lock() { *done = true; cvar.notify_all(); }
                    }
                }
            }
            // Route to engine default handler
            Self::handle_dtls_message(server, from, issuer, data);
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
        // Create a UDP NetworkMux bound to the requested address (port may be 0 for OS-assigned)
        eprintln!("[Engine] start_with_addr: bind_addr={:?}", bind_addr);
        let mux = Arc::new(UdpNetworkMux::bind(bind_addr).map_err(|e| format!("Failed to bind UDP mux: {}", e))?);
        // Determine the concrete local address after bind (handles port 0)
        let local_addr: SocketAddr = mux.local_addr().map_err(|e| format!("Failed to get local addr: {}", e))?;

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
            Self::handle_dtls_message(server, from, issuer, data);
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
        println!("[Engine] on_stun_consistent: public_addr={:?}", public_addr);
        // Save last known public address (for validation/tests)
        self.last_public_addr = public_addr;

        // Transition to TrianglePing and perform relay triangle test
        let prev = self.state;
        self.state = EngineState::TrianglePing;
        println!("[Engine] state change: {:?} -> TrianglePing", prev);
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

            // TODO: use a real discovery function
            const ADDRESS_SPEND: &str = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
            let discover = Arc::new(move || vec![RootRelayInfo { id: ADDRESS_SPEND.parse().unwrap(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345) }]);

            let finder = RelayFinder::new(api_opt.unwrap(), Duration::from_secs(60), discover);
            let relay = finder.find_relay(&self.options.as_ref().unwrap().handle);
            if let Ok(r) = relay {
                relay_target = Some(r.clone());
                println!("[Engine] chosen relay {} (id={})", r.address, r.id);
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
            let msg = Message::Relay(RelayMessage::TriangleTest1(RelayTriangleTest1 { app: None, checkingEndpoint: checking_ep }));
            let nsk = NetworkSourceKey::new_direct(to_addr);
            // Build JSON value for the message
            let json_val = crate::messages::marshal::to_json_value(&msg);
            if let Some(cb) = &self.send_via_bingle {
                // Use the relay's actual id as the user id. Convert Algorand base32 address to base64(36) for API validation.
                let uid = match data_encoding::BASE32_NOPAD.decode(target.id.as_bytes()) {
                    Ok(bytes) if bytes.len() == 36 => base64::engine::general_purpose::STANDARD.encode(bytes),
                    Ok(bytes) => {
                        println!("[Engine][WARN] relay id base32 decoded to {} bytes (expected 36); using raw id which may fail validation", bytes.len());
                        target.id.clone()
                    }
                    Err(e) => {
                        println!("[Engine][WARN] failed to decode relay id as base32: {}; using raw id which may fail validation", e);
                        target.id.clone()
                    }
                };
                let ok = cb(&nsk, &uid, json_val);
                println!("[Engine] TriangleTest1 send_via_bingle to {} (uid from relay id) -> {}", to_addr, ok);
                #[allow(unused)] { crate::util::logging::log_line(&format!("[Engine] TriangleTest1 send_via_bingle to {} (uid from relay id) -> {}", to_addr, ok)); }
            } else {
                println!("[Engine][WARN] send_via_bingle not installed; cannot send TriangleTest1 to {}", to_addr);
                #[allow(unused)] { crate::util::logging::log_line(&format!("[Engine][WARN] send_via_bingle not installed; cannot send TriangleTest1 to {}", to_addr)); }
            }
            // Wait for TriangleTest3 completion before marking EndpointAvailable.
            if let Some((pair, _t0)) = &self.triangle_wait {
                let pair = pair.clone();
                let self_ptr = std::sync::atomic::AtomicPtr::new(self as *mut Engine);
                std::thread::spawn(move || {
                    let (lock, cvar) = (&pair.0, &pair.1);
                    let mut guard = lock.lock().unwrap();
                    while !*guard {
                        guard = cvar.wait(guard).unwrap();
                    }
                    // Signal received: promote to EndpointAvailable
                    drop(guard);
                    unsafe {
                        use std::sync::atomic::Ordering;
                        let eng = &mut *self_ptr.load(Ordering::SeqCst);
                        eng.state = EngineState::EndpointAvailable;
                        eng.endpoint_ready.store(true, Ordering::SeqCst);
                    }
                    println!("[Engine] TriangleTest3 observed; state -> EndpointAvailable");
                    #[allow(unused)] { crate::util::logging::log_line("[Engine] TriangleTest3 observed; state -> EndpointAvailable"); }
                });
            }
        } else {
            println!("[Engine][WARN] TrianglePing path active but no destination to send TriangleTest1");
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

    pub fn state(&self) -> EngineState { self.state }
    pub fn last_public_addr(&self) -> Option<SocketAddr> { self.last_public_addr }
    pub fn test_force_stun_consistent(&mut self, addr: SocketAddr) { self.on_stun_consistent(Some(addr)); }

    /// DTLS message handler: try to interpret payload as UTF-8 JSON and route.
    fn handle_dtls_message(_server: &dyn Dtls, from: &SocketAddr, issuer: &str, data: &[u8]) {
        // Debug: log inbound DTLS application message (best-effort UTF-8 preview)
        let preview = match std::str::from_utf8(data) {
            Ok(s) => {
                let trimmed = if s.len() > 120 { &s[..120] } else { s };
                format!("utf8:{} bytes: {}", s.len(), trimmed)
            }
            Err(_) => format!("non-utf8:{} bytes", data.len()),
        };
        println!("[Engine::handle_dtls_message] from={} issuer={} {}", from, issuer, preview);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[Engine::handle_dtls_message] from={} issuer={} {}", from, issuer, preview)); }

        // Record last sender address for handlers that need to reply directly
        crate::messages::router::set_last_from(Some(*from));
        // Best-effort decode; print unimplemented on failure via default handler
        let handler = DefaultPrintingHandler;
        match std::str::from_utf8(data) {
            Ok(s) => {
                // Capture responseTag from raw JSON if present for handlers that need to echo it
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                    if let Some(tag) = v.get("responseTag").and_then(|vv| vv.as_str()) {
                        crate::messages::router::set_last_response_tag(Some(tag.to_string()));
                    } else {
                        crate::messages::router::set_last_response_tag(None);
                    }
                } else {
                    crate::messages::router::set_last_response_tag(None);
                }
                match from_json_str(s) {
                    Ok(msg) => route(&handler, &msg, issuer),
                    Err(_) => {
                        // Not valid JSON per our schema; treat as plaintext with raw bytes
                        // For now, just print
                        handler.on_unimplemented(&crate::messages::Message::Unknown(serde_json::Value::String(s.to_string())));
                    }
                }
            }
            Err(_) => {
                // Not UTF-8; ignore or log
                handler.on_unimplemented(&crate::messages::Message::Unknown(serde_json::Value::Null));
            }
        }
    }
}
