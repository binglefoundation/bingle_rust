use std::net::SocketAddr;
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
    dtls: Option<Box<dyn Dtls + Send + Sync>>,
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

    /// Provide a pre-configured DTLS instance (with server certificate material) from the API layer.
    pub fn set_dtls(&mut self, dtls: Box<dyn Dtls + Send + Sync>) {
        self.dtls = Some(dtls);
    }

    /// Convenience for API to send over DTLS managed by the engine.
    pub fn dtls_send(&self, to: SocketAddr, data: &[u8]) -> Result<(), String> {
        match &self.dtls {
            Some(d) => d.send(to, data),
            None => Err("DTLS not started".to_string()),
        }
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

        // Use the pre-configured DTLS instance provided by the API and install message handler
        let dtls = self.dtls.as_mut().ok_or_else(|| "DTLS instance not provided".to_string())?;
        // We'll detect RelayTriangleTest3 to unblock waiters while still routing to default
        let triangle_signal: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
        let triangle_signal_clone = triangle_signal.clone();
        let existing = dtls.get_handle_message();
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

        // Use pre-configured DTLS from API; install engine handler while preserving any existing API handler.
        let dtls = self.dtls.as_mut().ok_or_else(|| "DTLS instance not provided".to_string())?;
        let existing = dtls.get_handle_message();
        dtls.set_handle_message(Some(Arc::new(move |server, from, issuer, data| {
            Self::handle_dtls_message(server, from, issuer, data);
            if let Some(h) = &existing { h(server, from, issuer, data); }
        })));

        // Start DTLS accept loop with the mux and the concrete local address
        dtls.start(local_addr, Some(mux.clone() as Arc<dyn NetworkMux + Send + Sync>))
            .map_err(|e| format!("Failed to start DTLS: {}", e))?;

        // Start the UDP mux background loop
        mux.start().map_err(|_| "Failed to start UDP mux")?;

        self.mux = Some(mux);
        Ok(())
    }

    fn on_stun_consistent(&mut self, public_addr: Option<SocketAddr>) {
        println!("[Engine] on_stun_consistent: public_addr={:?}", public_addr);
        // Save last known public address (for validation/tests)
        self.last_public_addr = public_addr;
        // Per requirement: if we have a public address, do nothing further.
        if self.last_public_addr.is_some() {
            let prev = self.state;
            self.state = EngineState::EndpointAvailable;
            println!(
                "[Engine] STUN consistent with public address {:?}. State change: {:?} -> EndpointAvailable",
                self.last_public_addr, prev
            );
            // Minimal path: we do not send TriangleTest1 when a public address is known.
            return;
        }

        // Transition to TrianglePing and perform relay triangle test
        let prev = self.state;
        self.state = EngineState::TrianglePing;
        println!("[Engine] state change: {:?} -> TrianglePing (no public addr provided)", prev);
        // If DTLS isn't started (e.g., in tests), this is an error: we cannot proceed with triangle ping
        if self.dtls.is_none() {
            panic!("DTLS not started: cannot proceed with triangle ping after STUN consistent");
        }

        // Create/use a RelayFinder and use find_relay to obtain our relay address.
        // For now, discovery is stubbed to the provided public_addr (if any) and RelayCheck always returns available.
        let mut relay_target: Option<SocketAddr> = None;
        if let Some(addr) = public_addr {
            struct MockFinderApi;
            impl BingleApi for MockFinderApi {
                fn start(&mut self, _options: StartOptions) -> Result<(), String> { Ok(()) }
                fn stop(&mut self) {}
                fn network_change(&mut self) {}
                fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { false }
                fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { false }
                fn send_message_to_network(&self, _network_source_key: &NetworkSourceKey, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { true }
                fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not implemented".to_string()) }
                fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not implemented".to_string()) }
                fn send_message_to_network_with_response(&self, _network_source_key: &NetworkSourceKey, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> {
                    Ok(json!({"app": null, "type": "CheckResponse", "available": true}))
                }
                fn set_on_message(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnMessageHandler>>) {}
                fn set_on_connect(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnConnectHandler>>) {}
            }
            let api: Arc<dyn BingleApi> = Arc::new(MockFinderApi);
            let a2 = addr.clone();
            let discover = Arc::new(move || vec![RootRelayInfo { id: "dummy".to_string(), address: a2 }]);
            let finder = RelayFinder::new(api, Duration::from_secs(60), discover);
            let relay = finder.find_relay("");
            if let Ok(r) = relay { relay_target = Some(r); }
            self.relay_finder = Some(Arc::new(finder));
        }

        // Send TriangleTest1 either to the discovered relay or fall back to the provided public address
        if let (Some(dtls), Some(to_addr)) = (self.dtls.as_ref(), relay_target.or(public_addr)) {
            let msg = Message::Relay(RelayMessage::TriangleTest1(RelayTriangleTest1 { app: None, checkingEndpoint: to_addr }));
            let json = to_json_string(&msg);
            println!("[Engine] sending TriangleTest1 to {} ({} bytes)", to_addr, json.len());
            dtls.send(to_addr, json.as_bytes()).expect("DTLS send failed in Engine triangle ping");
            // Wait up to 10 seconds for TriangleTest3 indicated by DTLS handler
            if let Some((handle, _ts)) = &self.triangle_wait {
                let (lock, cvar) = (&handle.0, &handle.1);
                let mut done = lock.lock().unwrap();
                let timeout = Duration::from_secs(10);
                let (g, _res) = cvar.wait_timeout(done, timeout).unwrap();
                if *g {
                    println!("[Engine] received TriangleTest3 signal; state -> EndpointAvailable");
                    self.state = EngineState::EndpointAvailable;
                } else {
                    println!("[Engine][WARN] no TriangleTest3 within 10s; panic follows (NotImplemented)");
                    panic!("NotImplemented: STUN Consistent but no RelayTriangleTest3 received within 10 seconds");
                }
            }
        } else {
            println!("[Engine][WARN] TrianglePing path active but no destination to send TriangleTest1");
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
