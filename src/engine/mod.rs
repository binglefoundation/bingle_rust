use std::net::SocketAddr;
use std::sync::{Arc, Mutex, Condvar};
use std::time::{Duration, Instant};

use crate::api::bingle_api::StartOptions;
use crate::dtls::{Dtls, DtlsOpenSsl, NetworkMux, UdpNetworkMux};
use crate::messages::{from_json_str, route, DefaultPrintingHandler};
use crate::messages::handlers::MessageHandler;
use crate::messages::types::{Message, RelayMessage, RelayTriangleTest1};
use crate::messages::marshal::to_json_string;
use crate::stun::endpoint_finder::{StunEndpointFinder, StunState};
use crate::stun::endpoint_finder_impl::StunEndpointFinderImpl;
use crate::relay::relay_finder::RelayFinder;


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
    dtls: Option<DtlsOpenSsl>,
    state: EngineState,
    last_public_addr: Option<SocketAddr>,
    stun: Option<Arc<Mutex<Box<dyn StunEndpointFinder + Send + Sync>>>>, // background STUN
    relay_finder: Option<Arc<RelayFinder>>, // used to locate peer relay
    triangle_wait: Option<(Arc<(Mutex<bool>, Condvar)>, Instant)>, // wait for TriangleTest3
}

impl Engine {
    pub fn new() -> Self {
        Self { options: None, mux: None, dtls: None, state: EngineState::StunIdentify, last_public_addr: None, stun: None, relay_finder: None, triangle_wait: None }
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

        // Bind UDP on 0.0.0.0:0 and create mux (we will install STUN handler before wrapping in Arc)
        let mut mux0 = UdpNetworkMux::bind("0.0.0.0:0").map_err(|e| format!("Failed to bind UDP mux: {}", e))?;
        let local_addr: SocketAddr = mux0.local_addr().map_err(|e| format!("Failed to get local addr: {}", e))?;

        // Create a DTLS instance and install message handler
        let mut dtls = DtlsOpenSsl::new();
        // We'll detect RelayTriangleTest3 to unblock waiters while still routing to default
        let triangle_signal: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
        let triangle_signal_clone = triangle_signal.clone();
        dtls.set_handle_message(Some(Arc::new(move |server, from, issuer, data| {
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
            // Route to default printer for now
            Self::handle_dtls_message(server, from, issuer, data)
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

        // Configure the send_packet handler to use mux.write and set state change handler
        if let Ok(mut f) = finder.lock() {
            let mux_clone = mux.clone();
            f.set_send_packet_handler(Some(Arc::new(move |host, port, payload| {
                mux_clone.write((host, port), payload).expect("UDP mux write failed in STUN send_packet_handler");
            })));
            // In this minimal engine, we do not mutate engine state from STUN thread; tests can drive via helpers
            f.set_state_change_handler(None);

            // Kick off STUN polling using provided servers
            let servers = options.stun_servers.clone().unwrap_or_default();
            if servers.is_empty() {
                return Err("No STUN servers provided".into());
            }
            f.start(servers, 2_000, 60_000);
        }

        // Start DTLS with mux so that we can send/receive triangle messages over DTLS if needed later
        dtls.start(local_addr, Some(mux.clone() as Arc<dyn NetworkMux + Send + Sync>))
            .map_err(|e| format!("Failed to start DTLS: {}", e))?;

        // Start mux thread
        mux.start().map_err(|e| format!("Failed to start UDP mux: {}", e))?;

        self.mux = Some(mux);
        self.dtls = Some(dtls);
        self.stun = Some(finder);
        // Store triangle wait handle for later awaits
        self.triangle_wait = Some((triangle_signal, Instant::now()));
        Ok(())
    }

    fn start_with_addr(&mut self, _options: StartOptions, bind_addr: SocketAddr) -> Result<(), String> {
        // Create a UDP NetworkMux bound to the requested address (port may be 0 for OS-assigned)
        let mux = Arc::new(UdpNetworkMux::bind(bind_addr).map_err(|e| format!("Failed to bind UDP mux: {}", e))?);
        // Determine the concrete local address after bind (handles port 0)
        let local_addr: SocketAddr = mux.local_addr().map_err(|e| format!("Failed to get local addr: {}", e))?;

        // Create a DTLS instance and install a message handler that decodes JSON and routes it.
        let mut dtls = DtlsOpenSsl::new();
        dtls.set_handle_message(Some(Arc::new(|server, from, issuer, data| Self::handle_dtls_message(server, from, issuer, data))));

        // Start DTLS accept loop with the mux and the concrete local address
        dtls.start(local_addr, Some(mux.clone() as Arc<dyn NetworkMux + Send + Sync>))
            .map_err(|e| format!("Failed to start DTLS: {}", e))?;

        // Start the UDP mux background loop
        mux.start().map_err(|_| "Failed to start UDP mux")?;

        self.mux = Some(mux);
        self.dtls = Some(dtls);
        Ok(())
    }

    fn on_stun_consistent(&mut self, public_addr: Option<SocketAddr>) {
        // Save last known public address (for validation/tests)
        self.last_public_addr = public_addr;
        // Transition to TrianglePing and perform relay triangle test
        self.state = EngineState::TrianglePing;
        // If DTLS isn't started (e.g., in tests), this is an error: we cannot proceed with triangle ping
        if self.dtls.is_none() {
            panic!("DTLS not started: cannot proceed with triangle ping after STUN consistent");
        }
        // Find peer relay - for now, use RelayFinder with empty discovery returning empty => will error; ignore for minimal path
        // If we cannot find, we cannot proceed; in a full implementation, discovery would be wired.
        // Here, just attempt TriangleTest1 to the public addr if available (loopback triangle)
        if let (Some(dtls), Some(addr)) = (self.dtls.as_ref(), public_addr) {
            let msg = Message::Relay(RelayMessage::TriangleTest1(RelayTriangleTest1 { app: None, checkingEndpoint: addr }));
            let json = to_json_string(&msg);
            dtls.send(addr, json.as_bytes()).expect("DTLS send failed in Engine triangle ping");
            // Wait up to 10 seconds for TriangleTest3 indicated by DTLS handler
            if let Some((handle, _ts)) = &self.triangle_wait {
                let (lock, cvar) = (&handle.0, &handle.1);
                let mut done = lock.lock().unwrap();
                let timeout = Duration::from_secs(10);
                let (g, _res) = cvar.wait_timeout(done, timeout).unwrap();
                if *g {
                    self.state = EngineState::EndpointAvailable;
                } else {
                    panic!("NotImplemented: STUN Consistent but no RelayTriangleTest3 received within 10 seconds");
                }
            }
        }
    }

    fn on_stun_inconsistent(&mut self) {
        panic!("NotImplemented: STUN reported Inconsistent public endpoint");
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
    fn handle_dtls_message(_server: &dyn Dtls, _from: &SocketAddr, issuer: &str, data: &[u8]) {
        // Best-effort decode; print unimplemented on failure via default handler
        let handler = DefaultPrintingHandler;
        match std::str::from_utf8(data) {
            Ok(s) => {
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
