use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use data_encoding::BASE32_NOPAD;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::api::bingle_api::{BingleApi, NetworkEndpoint, StartOptions, UserId, Handle, ProgressCallback};
use crate::dtls::{Dtls, NetworkMux, UdpNetworkMux};
use crate::messages::handlers::MessageHandler;
use crate::messages::types::{Message, RelayMessage, RelayTriangleTest1};
use crate::turn::turn_handler::TurnHandler;
use crate::messages::{from_json_str, DefaultPrintingHandler};
use crate::relay::relay_finder::{RelayFinder, RelayInfo};
use crate::ddb::{AdvertRecord, InetSocketAddress, DdbBackend};
use crate::blockchain::algo_ops::AlgoChainConfig;
use crate::stun::endpoint_finder::StunEndpointFinder;
use crate::stun::endpoint_finder_impl::StunEndpointFinderImpl;
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
    Registered,
    NATRestricted
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatType {
    Unknown = 0,
    NoConnection = 1,
    Symmetric = 2,
    Restricted = 3,
    FullCone = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayState {
    Off,
    Starting,
    Available,
}

/// Minimal Engine implementation that wires UDP mux + DTLS and routes inbound JSON messages.
pub struct Engine {
    options: StartOptions,
    mux: Option<Arc<UdpNetworkMux>>, // concrete to access start/stop helpers
    // Underlying DTLS listener; per-connection adapters delegate to this
    dtls: Option<Box<dyn Dtls + Send + Sync>>,
    state: EngineState,
    relay_state: RelayState,
    last_public_addr: Option<SocketAddr>,
    stun: Option<Arc<Mutex<Box<dyn StunEndpointFinder + Send + Sync>>>>, // background STUN
    relay_finder: Option<Arc<RelayFinder>>, // used to locate peer relay
    triangle_wait: Option<(Arc<(Mutex<bool>, Condvar)>, Instant)>, // wait for TriangleTest3
    // Callback to send messages via the Bingle protocol (API surface) instead of direct DTLS
    send_via_bingle: Option<Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync>>,
    // Unified BingleApi handle bound to this engine instance (non-optional)
    bingle_api: Arc<dyn BingleApi>,
    // Async readiness flag: once set, engine_state_for_tests should report EndpointAvailable
    endpoint_ready: std::sync::atomic::AtomicBool,
    // Flag indicating NAT restricted state when endpoint is not yet available
    nat_restricted: std::sync::atomic::AtomicBool,
    // Flag indicating we have registered our endpoint in the DDB
    registered: std::sync::atomic::AtomicBool,
    // Current NAT type classification
    nat_type: std::sync::atomic::AtomicU8,
    // Per-connection state tracked at the Engine level (keyed by NetworkEndpointKey)
    connections: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<crate::api::bingle_api::NetworkEndpointKey, ConnectionEntry>>>, 
    // Pending responses map and issuer state moved from BingleApiImpl
    pending_responses: Arc<Mutex<HashMap<Uuid, Arc<(Mutex<ResponseWait>, Condvar)>>>>,
    issuer: Option<String>,
    // In-memory DDB backend used by relay nodes (and for tests)
    ddb_backend: std::sync::Arc<std::sync::Mutex<crate::ddb::InMemoryDdbBackend> >,
    // Per-API router instance used to avoid global mutable state
    router: Option<std::sync::Arc<crate::messages::router::Router>>,
    // DDB client bound to the API instance (always present; may be a NullDdbClient)
    ddb_client: std::sync::Arc<dyn crate::ddb::DdbClient>,
    // TURN handlers (split): client and relay variants
    turn_handler_client: std::sync::Arc<crate::turn::turn_client_handler_impl::TurnClientHandlerImpl>,
    turn_handler_relay: std::sync::Arc<crate::turn::turn_relay_handler_impl::TurnRelayHandlerImpl>,
    // Application-level callback for listening state changes (set by API)
    on_listening_cb: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<crate::api::bingle_api::OnListeningHandler>>>>,
}

impl Engine {
    pub fn relay_state_str(&self) -> String {
        match self.relay_state {
            RelayState::Off => "off".to_string(),
            RelayState::Starting => "starting".to_string(),
            RelayState::Available => "available".to_string(),
        }
    }

    /// Return the appropriate TURN handler for current role (client vs relay)
    pub fn get_approp_turn_handler(&self) -> std::sync::Arc<dyn TurnHandler + Send + Sync> {
        if self.options.am_relay {
            self.turn_handler_relay.clone()
        } else {
            self.turn_handler_client.clone()
        }
    }

    /// Upsert a list of root relays into the in-memory DDB backend (as am_relay=true records).
    fn upsert_roots_into_backend(&self, roots: &[RelayInfo]) {
        if roots.is_empty() {
            log::debug!("[Engine::upsert_roots_into_backend] no roots to upsert");
            return;
        }
        log::info!(
            "[Engine::upsert_roots_into_backend] upserting {} root relay record(s)",
            roots.len()
        );
        if let Ok(mut b) = self.ddb_backend.lock() {
            for r in roots {
                let host = match r.address.ip() { IpAddr::V4(v4) => v4.to_string(), IpAddr::V6(v6) => v6.to_string() };
                log::debug!(
                    "[Engine::upsert_roots_into_backend] upsert id={} addr={}:{}",
                    r.id,
                    host,
                    r.address.port()
                );
                let rec = AdvertRecord {
                    id: r.id.clone(),
                    endpoint: Some(InetSocketAddress { host, port: r.address.port() }),
                    am_relay: Some(true),
                    relay_id: None,
                    relay_sig: None,
                    date: "1970-01-01T00:00:00Z".to_string(),
                    sig: None,
                };
                b.upsert(rec);
            }
            log::info!("[Engine::upsert_roots_into_backend] upsert complete");
        } else {
            log::warn!("[Engine::upsert_roots_into_backend] failed to lock ddb_backend for upsert");
        }
    }

    /// Test helper to upsert provided roots into backend.
    pub fn upsert_root_relays_for_tests(&mut self, roots: Vec<RelayInfo>) {
        self.upsert_roots_into_backend(&roots);
    }

    /// Test helper to query backend for a given id.
    pub fn ddb_backend_lookup_for_tests(&self, id: &str) -> Option<crate::ddb::AdvertRecord> {
        self.ddb_backend.lock().ok().and_then(|b| b.lookup(id))
    }
    pub fn ddb_client(&self) -> std::sync::Arc<dyn crate::ddb::DdbClient> {
        self.ddb_client.clone()
    }
    pub fn app_id(&self) -> Option<u64> { self.options.app_id }
    pub fn algo_provider_config(&self) -> Option<AlgoChainConfig> { self.options.algo_provider_config.clone() }

    /// Install or clear the application-level OnListening handler (set by API).
    pub fn set_on_listening_handler(&mut self, cb: Option<std::sync::Arc<crate::api::bingle_api::OnListeningHandler>>) {
        if let Ok(mut g) = self.on_listening_cb.lock() { *g = cb; }
    }

    /// Notify the application-level OnListening handler, if installed.
    pub fn notify_listening(&self, listening: bool) {
        if let Ok(g) = self.on_listening_cb.lock() {
            if let Some(cb) = &*g { cb(listening); }
        }
    }

    /// Create a common TURN handler for both relay and client modes
    fn create_turn_handler(&self) -> std::sync::Arc<dyn Fn(&dyn NetworkMux, &SocketAddr, &[u8]) + Send + Sync> {
        let am_relay = self.options.am_relay;
        let turn: std::sync::Arc<dyn TurnHandler + Send + Sync> = self.get_approp_turn_handler();

        let local_public_addr = self.last_public_addr().clone();
        Arc::new(move |source: &dyn NetworkMux, from: &SocketAddr, packet: &[u8]| {
            // Parse/unwrap the TURN ChannelData using our handler
            if let Some(wrapped) = turn.handle_turn_incoming(Some(from), local_public_addr, packet) {
                if am_relay {
                    log::info!("[Engine][TURN] handle_turn_incoming (relay) {} bytes from {}:", wrapped.message.len(), wrapped.network_endpoint);
                    // Relay role: forward stripped payload to resolved ip_address via concrete UDP mux
                    if let Some(udp) = source.as_any().downcast_ref::<crate::dtls::network_mux_udp::UdpNetworkMux>() {
                        let nsk = crate::api::bingle_api::NetworkEndpoint::new_direct(wrapped.ip_address);
                        // Here we forward the TURN packet including channel number to the resolved ip_address
                        if let Err(e) = udp.write(&nsk, &packet) {
                            log::warn!("[Engine][TURN relay] forward to {} failed: {}", wrapped.ip_address, e);
                        } else {
                            log::info!("[Engine][TURN relay] forwarded {} bytes to {}", wrapped.message.len(), wrapped.ip_address);
                        }
                    } else {
                        log::warn!("[Engine][TURN relay] source is not UdpNetworkMux; cannot forward");
                    }
                } else {
                    log::info!("[Engine][TURN] handle_turn_incoming (not relay) {} bytes from {}:", wrapped.message.len(), wrapped.network_endpoint);
                    // Non-relay role: this packet is for us. Re-inject the stripped payload into the UDP mux
                    if let Some(udp) = source.as_any().downcast_ref::<crate::dtls::network_mux_udp::UdpNetworkMux>() {
                        udp.reprocess(&wrapped.network_endpoint, &wrapped.message);
                        log::info!("[Engine][TURN client] reprocessed {} bytes from {}", wrapped.message.len(), wrapped.network_endpoint);
                    } else {
                        log::warn!("[Engine][TURN client] source is not UdpNetworkMux; cannot reprocess");
                    }
                }
            } else {
                log::warn!("[Engine][TURN] handle_turn_incoming returned None (ignored)");
            }
        })
    }
}

// Adapter that exposes Engine as a BingleApi implementation so that handlers and discovery
// can call back into the Engine directly without going through BingleApiImpl.
pub struct EngineBingleApiHandle(pub std::sync::Arc<std::sync::atomic::AtomicPtr<Engine>>);

impl BingleApi for EngineBingleApiHandle {
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return None; }
        unsafe { (*p).issuer().map(|iss| iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string()) }
    }
    fn get_handle(&self) -> Option<String> {
        // For safety, Engine-backed handle returns None; handle is available via high-level API.
        None
    }
    fn get_app_id(&self) -> Option<u64> {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return None; }
        unsafe { (*p).app_id() }
    }
    fn get_algo_provider_config(&self) -> Option<AlgoChainConfig> {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return None; }
        unsafe { (*p).algo_provider_config() }
    }

    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Err("not supported".into()) }
    fn stop(&mut self) { }
    fn network_change(&mut self) { }

    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<std::sync::Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<std::sync::Arc<ProgressCallback>>) -> bool { false }

    fn send_message_to_network(&self, nsk: &NetworkEndpoint, user_id: &UserId, message: serde_json::Value, _progress: Option<std::sync::Arc<ProgressCallback>>) -> bool {
        log::info!("[EngineBingleApiHandle::send_message_to_network] nsk={} user_id={} message={}", nsk, user_id, message);
        use std::sync::atomic::Ordering;
        if nsk.inet_socket_address().is_some() || (nsk.relay_channel().is_some() && nsk.relay_address().is_some()) {
            let valid = match BASE32_NOPAD.decode(user_id.as_bytes()) {
                Ok(bytes) if bytes.len() == 36 => true,
                _ => false,
            };
            if !valid { return false; }
            let bytes = match serde_json::to_vec(&message) { Ok(b) => b, Err(_) => return false };
            let p = self.0.load(Ordering::SeqCst);
            if p.is_null() { return false; }
            unsafe { (*p).send_to_peer(nsk, &bytes).is_ok() }
        } else { false }
    }

    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<std::sync::Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not implemented".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<std::sync::Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not implemented".into()) }

    fn send_message_to_network_with_response(&self, nsk: &NetworkEndpoint, user_id: &UserId, message: serde_json::Value, _progress: Option<std::sync::Arc<ProgressCallback>>) -> Result<serde_json::Value, String> {
        use std::sync::atomic::Ordering;
        use uuid::Uuid;
        let tag = Uuid::new_v4();
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return Err("null engine".into()); }
        unsafe { (*p).register_pending(tag); }
        let msg_with_tag = match message {
            serde_json::Value::Object(mut m) => { m.insert("responseTag".to_string(), serde_json::Value::String(tag.to_string())); serde_json::Value::Object(m) }
            other => { let mut m = serde_json::Map::new(); m.insert("payload".to_string(), other); m.insert("responseTag".to_string(), serde_json::Value::String(tag.to_string())); serde_json::Value::Object(m) }
        };
        let sent = self.send_message_to_network(nsk, user_id, msg_with_tag, None);
        let timeout = Duration::from_secs(10);
        let p2 = self.0.load(Ordering::SeqCst);
        if p2.is_null() { return Err("null engine".into()); }
        unsafe {
            if let Some(resp) = (*p2).wait_for_response(&tag, timeout) { Ok(resp) } else { Err(if sent { "timeout waiting for response" } else { "send failed" }.into()) }
        }
    }

    fn set_on_message(&mut self, _handler: Option<std::sync::Arc<crate::api::bingle_api::OnMessageHandler>>) { }
    fn set_on_connect(&mut self, _handler: Option<std::sync::Arc<crate::api::bingle_api::OnConnectHandler>>) { }
}

// Adapter exposing minimal internal controls for handlers -> engine without referencing BingleApiImpl
pub struct EngineInternalPtr(pub std::sync::Arc<std::sync::atomic::AtomicPtr<Engine>>);
impl crate::api::bingle_api::BingleApiInternal for EngineInternalPtr {
    fn get_relay_state(&self) -> String {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return "off".to_string(); }
        unsafe { (*p).relay_state_str() }
    }
    fn set_state(&self, state: EngineState) {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return; }
        unsafe { let _ = (*p).set_state_internal(state); }
    }
    fn get_state(&self) -> EngineState {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return EngineState::StunIdentify; }
        unsafe { (*p).state() }
    }
    fn set_nat_type(&self, nat: NatType) {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return; }
        unsafe { (*p).set_nat_type(nat); }
    }
    fn get_last_public_addr(&self) -> Option<SocketAddr> {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return None; }
        unsafe { (*p).last_public_addr() }
    }
    fn ddb_register_ip(&self, endpoint: SocketAddr) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return Err("null engine".into()); }
        unsafe { (*p).ddb_client().register_ip(endpoint) }
    }
    fn ddb_register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return Err("null engine".into()); }
        unsafe { (*p).ddb_client().register_relay(relay_id, relay_sig) }
    }
    fn notify_listening(&self, listening: bool) {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return; }
        unsafe { (*p).notify_listening(listening); }
    }
    fn update_turn_listener_relay(&self, relay_id: String, relay_addr: std::net::SocketAddr) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return Err("null engine".into()); }
        unsafe {
            let ok = (*p).turn_handler_relay.handle_listen(&relay_id, &relay_addr);
            if ok { Ok(()) } else { Err(format!("failed to update TURN listener mapping for {} -> {}", relay_id, relay_addr)) }
        }
    }
    fn turn_lookup_addr_by_id(&self, id: String) -> Option<std::net::SocketAddr> {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return None; }
        unsafe { (*p).turn_handler_relay.lookup_addr_by_id(&id) }
    }
    fn turn_handle_call(&self, source: std::net::SocketAddr, dest: std::net::SocketAddr) -> i32 {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return -1; }
        unsafe {
            crate::turn::turn_handler::TurnRelayHandler::handle_call(&*((*p).turn_handler_relay), &source, &dest)
        }
    }
    fn turn_handle_listen(&self, id: String, source: std::net::SocketAddr) -> bool {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return false; }
        unsafe { (*p).turn_handler_relay.handle_listen(&id, &source) }
    }
    fn turn_handle_called(&self, source: std::net::SocketAddr, dest: std::net::SocketAddr, channel: u16) {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return; }
        unsafe {
            crate::turn::turn_handler::TurnClientHandler::handle_called(&*((*p).turn_handler_client), &source, &dest, channel);
        }
    }
    fn turn_client_handle_listen_response(&self, relay_addr: std::net::SocketAddr, relay_id: String) {
        use std::sync::atomic::Ordering;
        let p = self.0.load(Ordering::SeqCst);
        if p.is_null() { return; }
        unsafe {
            (*p).turn_client_handle_listen_response(relay_addr, &relay_id);
        }
    }
}

// Per-connection state holding a DTLS adapter bound to a specific peer
struct ConnectionEntry {
    last_seen: Instant,
}



impl Engine {
    pub fn new(options: &StartOptions, api: Arc<dyn BingleApi>) -> Self {
        log::info!("[Engine::new] options={:?}", options);
        #[allow(unused)] {  }
        // Build a DDB client now (always present); choose real or null implementation
        #[cfg(not(target_os = "ios"))]
        let ddb: std::sync::Arc<dyn crate::ddb::DdbClient> = {
            let have_app = api.get_app_id().or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
            if have_app.is_none() { log::error!("[Engine::new] no BINGLE_APP_ID set will use NullDdbClient"); }
            if have_app.is_some() { std::sync::Arc::new(crate::ddb::DdbClientImpl::new(api.clone())) } else { std::sync::Arc::new(crate::ddb::NullDdbClient::new()) }
        };

        #[cfg(target_os = "ios")]
        let ddb: std::sync::Arc<dyn crate::ddb::DdbClient> = std::sync::Arc::new(crate::ddb::NullDdbClient::new());

        Self {
            options: options.clone(),
            mux: None,
            dtls: None,
            state: EngineState::StunIdentify,
            relay_state: RelayState::Off,
            last_public_addr: options.static_ip.clone(),
            stun: None,
            relay_finder: None,
            triangle_wait: None,
            send_via_bingle: None,
            bingle_api: api,
            endpoint_ready: std::sync::atomic::AtomicBool::new(false),
            nat_restricted: std::sync::atomic::AtomicBool::new(false),
            registered: std::sync::atomic::AtomicBool::new(false),
            nat_type: std::sync::atomic::AtomicU8::new(NatType::Unknown as u8),
            connections: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            pending_responses: Arc::new(Mutex::new(HashMap::new())),
            issuer: None,
            ddb_backend: std::sync::Arc::new(std::sync::Mutex::new(crate::ddb::InMemoryDdbBackend::new())),
            router: None,
            ddb_client: ddb,
            turn_handler_client: std::sync::Arc::new(crate::turn::turn_client_handler_impl::TurnClientHandlerImpl::new()),
            turn_handler_relay: std::sync::Arc::new(crate::turn::turn_relay_handler_impl::TurnRelayHandlerImpl::new()),
            on_listening_cb: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Create an Engine without binding a BingleApi; API can be provided later via set_bingle_api.
    pub fn new_unbound(options: &StartOptions) -> Self {
        // Use a placeholder API that returns None/false until a real API is bound.
        struct EmptyApi;
        impl crate::api::bingle_api::BingleApi for EmptyApi {
            fn debug_print_options(&self) {}
            fn get_my_id(&self) -> Option<String> { None }
            fn get_app_id(&self) -> Option<u64> { None }
            fn get_algo_provider_config(&self) -> Option<crate::blockchain::algo_ops::AlgoChainConfig> { None }
            fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
            fn stop(&mut self) {}
            fn network_change(&mut self) {}
            fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { false }
            fn send_message_to_handle(&self, _handle: &crate::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { false }
            fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { false }
            fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not implemented".into()) }
            fn send_message_to_handle_with_response(&self, _handle: &crate::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not implemented".into()) }
            fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not implemented".into()) }
            fn set_on_message(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnMessageHandler>>) {}
            fn set_on_connect(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnConnectHandler>>) {}
        }
        Self::new(options, Arc::new(EmptyApi))
    }

    /// Provide a pre-configured DTLS instance (with server certificate material) from the API layer.
    pub fn set_dtls(&mut self, dtls: Box<dyn Dtls + Send + Sync>) {
        self.dtls = Some(dtls);
    }

    /// Provide a per-API router instance to avoid global state collisions across APIs/tests.
    pub fn set_router(&mut self, router: std::sync::Arc<crate::messages::router::Router>) {
        self.router = Some(router);
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
    pub fn set_send_via_bingle(&mut self, cb: Option<Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync>>) {
        self.send_via_bingle = cb;
    }

    /// Set or replace the BingleApi handle bound to this Engine instance.
    pub fn set_bingle_api(&mut self, api: Arc<dyn BingleApi>) {
        self.bingle_api = api.clone();
        // Initialize a DDB client bound to this API instance (always set; may be Null)
        #[cfg(not(target_os = "ios"))]
        {
            let have_app = api.get_app_id().or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
            self.ddb_client = if have_app.is_some() {
                std::sync::Arc::new(crate::ddb::DdbClientImpl::new(api.clone()))
            } else {
                std::sync::Arc::new(crate::ddb::NullDdbClient::new())
            };
        }
        #[cfg(target_os = "ios")]
        {
            self.ddb_client = std::sync::Arc::new(crate::ddb::NullDdbClient::new());
        }
    }

    /// Clear bindings to API instance and global router callbacks to avoid dangling pointers between tests.
    pub fn clear_api_bindings(&mut self) {
        // Clear per-API router instance only (no global fallbacks)
        if let Some(r) = &self.router { r.clear_for_tests(); }
        // Also drop local references
        self.send_via_bingle = None;
    }

    /// Check whether the engine believes a connection to endpoint exists.
    pub fn has_connection(&self, endpoint: &crate::api::bingle_api::NetworkEndpoint) -> bool {
        if let Some(key) = endpoint.get_key() {
            self.connections.lock().map(|m| m.contains_key(&key)).unwrap_or(false)
        } else {
            false
        }
    }

    /// Testing helper: number of tracked connections.
    pub fn connections_len_for_tests(&self) -> usize {
        self.connections.lock().map(|m| m.len()).unwrap_or(0)
    }

    /// Send bytes to a peer and track the connection's last_seen.
    /// If this is the first interaction with the peer, create a connection entry on successful send.
    pub fn send_to_peer(&self, to: &crate::api::bingle_api::NetworkEndpoint, data: &[u8]) -> Result<(), String> {
        // Perform the DTLS send using the configured DTLS instance (avoid pre-locking connections to
        // prevent rare OS mutex EINVAL during early send paths). We update the connection map only
        // after a successful send.
        let dtls = self.dtls.as_ref().ok_or_else(|| "DTLS instance not provided".to_string())?;
        let res = dtls.send(to, data);
        if res.is_ok() {
            // Track connection using NetworkEndpointKey derived from `to`
            if let Some(key) = to.get_key() {
                if let Ok(mut m) = self.connections.lock() {
                    use std::collections::hash_map::Entry;
                    match m.entry(key) {
                        Entry::Occupied(mut e) => { e.get_mut().last_seen = Instant::now(); }
                        Entry::Vacant(v) => { v.insert(ConnectionEntry { last_seen: Instant::now() }); }
                    }
                }
            }
        }
        res
    }

    /// Install or wrap the DTLS handle_message callback to delegate into the Engine routing logic.
    /// This avoids duplicating the same closure in different Engine start paths.
    fn install_dtls_handler(&mut self) -> Result<(), String> {
        // Capture any existing handler without taking a mutable borrow to self.dtls
        let existing = {
            let dref = self
                .dtls
                .as_ref()
                .ok_or_else(|| "DTLS instance not provided".to_string())?;
            dref.get_handle_message()
        };

        // Capture safe, shareable state for the handler closure (avoid raw self pointers)
        let connections = self.connections.clone();
        let pending_responses = self.pending_responses.clone();
        let _ = self.send_via_bingle.clone();
        let bingle_api = self.bingle_api.clone();
        let am_relay = self.options.am_relay;
        let ddb_backend = self.ddb_backend.clone();

        // Now obtain a mutable reference to dtls only for installing the new handler
        if let Some(d) = self.dtls.as_mut() {
            let router_arc = self.router.clone();
            d.set_handle_message(Some(std::sync::Arc::new(move |server, from, issuer, data| {
                log::info!("[Engine::install_dtls_handler][cb] invoked from={} issuer={} bytes={}", from, issuer, data.len());
                let work = || {
                    // 1) Track connection last_seen using captured connections map
                    if let Ok(mut m) = connections.lock() {
                        use std::collections::hash_map::Entry;
                        let key_from = from
                            .get_key()
                            .expect("direct endpoint key");
                        match m.entry(key_from) {
                            Entry::Occupied(mut e) => { e.get_mut().last_seen = Instant::now(); }
                            Entry::Vacant(v) => { v.insert(ConnectionEntry { last_seen: Instant::now() }); }
                        }
                    }

                    // No inline DDB handling; use Router + handlers instead

                    // 2) Provide per-message API bindings to router; sender remains as configured by API layer
                    if let Some(r) = &router_arc { r.set_bingle_api(Some(bingle_api.clone())); }
                    // Provide DDB/relay context to router
                    if let Some(r) = &router_arc {
                        r.set_am_relay(am_relay);
                        r.set_ddb_backend(Some(ddb_backend.clone()));
                    }

                    // 3) Engine routing logic (inline to avoid &self)
                    // Record last sender for reply helpers
                    if let Some(r) = &router_arc { r.set_last_from(from.inet_socket_address()); }

                    // Try JSON parse to extract responseTag and fulfill waiters
                    if let Ok(s) = std::str::from_utf8(data) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                            log::info!("[Engine::install_dtls_handler][cb] checking for responseTag in {}", v);
                            // Expose last responseTag (if this is a request). Handlers may echo it back.
                            if let Some(tag) = v.get("responseTag").and_then(|vv| vv.as_str()) {
                                if let Some(r) = &router_arc { r.set_last_response_tag(Some(tag.to_string())); }
                            } else {
                                if let Some(r) = &router_arc { r.set_last_response_tag(None); }
                            }
                            // If this is a response, fulfill any waiter registered with Engine (supports both keys)
                            let tag_str_opt = v.get("responseTag").and_then(|vv| vv.as_str())
                                .or_else(|| v.get("tag").and_then(|vv| vv.as_str()));
                            if let Some(tag_str) = tag_str_opt {
                                if let Ok(tag_uuid) = uuid::Uuid::parse_str(tag_str) {
                                    if let Ok(map) = pending_responses.lock() {
                                        if let Some(wait) = map.get(&tag_uuid) {
                                            if let Ok(mut g) = wait.0.lock() {
                                                log::info!("[Engine::install_dtls_handler][cb] got response, returning");
                                                g.responded = true;
                                                g.response = Some(v.clone());
                                                wait.1.notify_all();
                                                return; // consumed by waiter; do not forward
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Route through the message framework for internal handlers (triangle tests etc.)
                    let handler = DefaultPrintingHandler;
                    match std::str::from_utf8(data) {
                        Ok(s) => match from_json_str(s) {
                            Ok(msg) => {
                                log::info!("[Engine::install_dtls_handler][cb] routing message {:?}", msg);
                                if let Some(r) = &router_arc {
                                    r.route_with_network(&handler, &msg, issuer, &from);
                                    if let Some(out) = r.take_outbound_response() {
                                        log::info!("[Engine::install_dtls_handler][cb] sending response {:?}", out);
                                        let bytes = serde_json::to_vec(&out).unwrap_or_else(|_| b"{}".to_vec());
                                        {
                                            if let Err(e) = server.send(&from, &bytes) { log::warn!("[Engine::install_dtls_handler][send outbound_response] failed: {}", e); }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                // Not valid JSON per our schema; treat as plaintext with raw bytes
                                log::warn!("[Engine::install_dtls_handler][cb] not valid json {} {:?}", s, e);
                                handler.on_unimplemented(&crate::messages::Message::Unknown(serde_json::Value::String(s.to_string())));
                            }
                        },
                        Err(e) => {
                            log::warn!("[Engine::install_dtls_handler][cb] not UTF-8 {:?}", e);
                            handler.on_unimplemented(&crate::messages::Message::Unknown(serde_json::Value::Null));
                        }
                    }

                    // Then delegate to any previously-registered handler from API
                    if let Some(h) = &existing {
                        h(server, from, issuer, data);
                    }
                };

                if let Some(r) = &router_arc { crate::messages::router::Router::with_current_router(r.clone(), || work()); } else { work(); }
            })));
            Ok(())
        } else {
            Err("DTLS instance not provided".to_string())
        }
    }

    /// Start the engine using the provided StartOptions.
    /// Implements static endpoint path or STUN-based discovery when not provided.
    pub fn start(&mut self, options: &StartOptions) -> Result<(), String> {
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

        // Use the pre-configured DTLS instance provided by the API and install message handler
        // We'll detect RelayTriangleTest3 to unblock waiters while still routing to default
        let triangle_signal: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
        let _triangle_signal_clone = triangle_signal.clone();
        // Install the common DTLS handler wrapper
        self.install_dtls_handler()?;

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

        // Configure TURN ChannelData handler based on role (relay vs client)
        log::info!("[Engine] set_handle_turn from start");
        let th = self.create_turn_handler();
        mux0.set_handle_turn(Some(&th));

        // Now wrap mux in Arc
        let mux = Arc::new(mux0);

        // Start mux thread first so DTLS accept loop can receive
        mux.start().map_err(|e| format!("Failed to start UDP mux: {}", e))?;

        // Start DTLS with mux so that we can send/receive triangle messages over DTLS if needed later
        if let Some(d) = self.dtls.as_mut() {
            d.start(mux.clone()).map_err(|e| format!("Failed to start DTLS: {}", e))?;
        } else {
            return Err("DTLS instance not provided".to_string());
        }

        // Persist mux, STUN finder, and triangle wait handle before initializing STUN
        self.mux = Some(mux.clone());
        self.stun = Some(finder.clone());
        // Store triangle wait handle for later awaits
        self.triangle_wait = Some((triangle_signal, Instant::now()));

        // After DTLS and mux are running, configure and start STUN finder logic
        self.start_stun_find(&options, &finder, &mux)?;
        Ok(())
    }

    fn start_with_addr(&mut self, _options: &StartOptions, bind_addr: SocketAddr) -> Result<(), String> {
        self.last_public_addr = Some(self.options.static_ip.clone().expect("start_with_address when no static address"));

        // Always bind UDP to 0.0.0.0:<port> so that we listen on all interfaces, even when a static external IP is configured.
        // The static address is used for signaling and routing outside any firewall, not for local bind.
        let port = bind_addr.port();
        let bind_all = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
        log::info!("[Engine] start_with_addr: requested={:?} binding={:?}", bind_addr, bind_all);
        log::warn!("[Engine] start_with_addr: requested={:?} binding={:?}", bind_addr, bind_all);
        let mut mux0 = UdpNetworkMux::bind(bind_all).map_err(|e| format!("Failed to bind UDP mux: {}", e))?;
        // Determine the concrete local address after bind (handles port 0)
        let _local_addr: SocketAddr = mux0.local_addr().map_err(|e| format!("Failed to get local addr: {}", e))?;

        // Install the common DTLS handler wrapper
        self.install_dtls_handler()?;

        // Configure TURN ChannelData handler based on role (relay vs client)
        log::info!("[Engine] set_handle_turn from start_with_addr");
        let th = self.create_turn_handler();
        mux0.set_handle_turn(Some(&th));

        // Now wrap mux in Arc
        let mux = Arc::new(mux0);

        // Start the UDP mux background loop first
        mux.start().map_err(|_| "Failed to start UDP mux")?;

        // Start DTLS accept loop with the mux
        if let Some(d) = self.dtls.as_mut() {
            d.start(mux.clone()).map_err(|e| format!("Failed to start DTLS: {}", e))?;
            // Static address path: once DTLS accept loop is running, notify that we are listening.
            if let Some(r) = &self.router {
                if let Some(internal) = r.get_bingle_api_internal() {
                    log::info!("[Engine] notifying internal listeners of listening state true");
                    internal.notify_listening(true);
                }
                else {
                    log::error!("[Engine] start_with_address: no internal API");
                }
            }
            else {
                log::error!("[Engine] start_with_address: no router");
            }

            // If we are configured as a relay, pre-populate the in-memory DDB with known root relays.
            if self.options.am_relay {
                // Before discovering peers, mark relay state as Starting
                self.relay_state = RelayState::Starting;
                // Build discovery closure using indexer when app_id is configured; else skip.
                #[cfg(not(target_os = "ios"))]
                {
                    let app_id_opt = self.options.app_id.or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
                    if let Some(app_id) = app_id_opt {
                        let cfg = self.options.algo_provider_config.clone();
                        let discover = crate::relay::discovery::indexer_discover_closure(app_id, cfg);
                        let finder = RelayFinder::new(self.bingle_api.clone(), Duration::from_secs(60), discover);
                        // Determine our id for exclusion
                        let my_id = if let Some(iss) = self.issuer.as_deref() { iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string() } else { self.options.handle.clone() };
                        let roots = finder.list_root_relays(&my_id, true);
                        log::info!("[Engine::start_with_addr] discovered {} root relays (excluding self)", roots.len());
                        self.upsert_roots_into_backend(&roots);
                        log::info!("[Engine::start_with_addr] upserted root relays into backend");
                        self.relay_finder = Some(Arc::new(finder));
                        // DDB is ready after upserting roots
                        self.relay_state = RelayState::Available;
                    } else {
                        log::warn!("[Engine::start_with_addr] am_relay set but app_id not configured; skipping root relay discovery");
                        // Even if discovery is skipped, the relay is operational for local tests; mark available.
                        self.relay_state = RelayState::Available;
                    }
                }
            }
        } else {
            log::error!("[Engine] start_with_address: no DTLS instance");
            return Err("DTLS instance not provided".to_string());
        }

        self.mux = Some(mux);
        log::info!("[Engine] start_with_addr: done");

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
        #[allow(unused)] {  }

        // Do NOT mark EndpointAvailable here; proceed with the triangle process and only
        // transition to EndpointAvailable once TriangleTest3 is observed.
        if self.dtls.is_none() {
            panic!("DTLS not started: cannot proceed with triangle ping after STUN consistent");
        }

        // Create/use a RelayFinder and use find_relay to obtain our relay address.
        // For now, discovery is stubbed to the provided public_addr (if any) and RelayCheck always returns available.
        let mut relay_target: Option<RelayInfo> = None;
        if let Some(addr) = public_addr {
            let _a2 = addr.clone();
            // Use the real BingleApi provided via router
            let api = self.bingle_api.clone();

            // Use Indexer-based discovery when available via AlgoBingle::list_static_endpoints_via_indexer
            // Prefer app_id from StartOptions; fallback to env var for legacy tests; else use built-in localhost relays.
            let discover: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync> = {
                #[cfg(not(target_os = "ios"))]
                {
                    // Capture app_id and provider config from options
                    let opt_app_id = self.options.app_id;
                    let opt_cfg = self.options.algo_provider_config.clone();
                    let app_id_opt = opt_app_id.or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
                    log::info!("[Engine] indexer discovery app_id={:?}", app_id_opt);
                    if let Some(app_id) = app_id_opt {
                        crate::relay::discovery::indexer_discover_closure(app_id, opt_cfg)
                    } else {
                        // No app id set
                        panic!("[Engine] indexer discovery has no app id");
                    }
                }
                #[cfg(target_os = "ios")]
                {
                    Arc::new(|| vec![RelayInfo { id: "IOS-DUMMY".to_string(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345) }])
                }
            };

            let finder = RelayFinder::new(api, Duration::from_secs(60), discover);
            // Use our id (Algorand address) for relay selection, not the user-visible handle.
            // Prefer the issuer set earlier by BingleApiImpl::start (issuer = id + ISSUER_SUFFIX).
            let my_id: String = if let Some(iss) = self.issuer.as_deref() {
                iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string()
            } else {
                // Fallback: if issuer is not set, use the handle (best-effort; may yield suboptimal selection).
                self.options.handle.clone()
            };
            // If configured as a relay, update the in-memory DDB with all root relays discovered
            if self.options.am_relay {
                let roots = finder.list_root_relays(&my_id, true);
                log::info!("[Engine::stun_consistent_process] discovered {} root relays (excluding self)", roots.len());
                self.upsert_roots_into_backend(&roots);
                log::info!("[Engine::stun_consistent_process] upserted root relays into backend");
            }

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
            let nsk = NetworkEndpoint::new_direct(to_addr);
            // Build JSON value for the message
            let json_val = crate::messages::marshal::to_json_value(&msg);
            if let Some(cb) = &self.send_via_bingle {
                // Use the relay's actual Algorand address (base32) as the user id.
                let uid = target.id.clone();
                let ok = cb(&nsk, &uid, json_val);
                log::info!("[Engine] TriangleTest1 send_via_bingle to {} (uid=base32 relay id) -> {}", to_addr, ok);
                #[allow(unused)] {  }
            } else {
                log::info!("[Engine][WARN] send_via_bingle not installed; cannot send TriangleTest1 to {}", to_addr);
                #[allow(unused)] {  }
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
                // Resolve host string to IP and wrap into NetworkSourceKey for direct UDP send
                match host.parse::<std::net::IpAddr>() {
                    Ok(ip) => {
                        let addr = std::net::SocketAddr::new(ip, port);
                        let nsk = crate::api::bingle_api::NetworkEndpoint::new_direct(addr);
                        mux_clone
                            .write(&nsk, payload)
                            .expect("UDP mux write failed in STUN send_packet_handler");
                    }
                    Err(e) => {
                        log::warn!("[Engine::start] STUN send_packet_handler: invalid host '{}': {}", host, e);
                    }
                }
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
        // First, clear any API pointers and global router callbacks to avoid dangling references across tests
        self.clear_api_bindings();
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
        if self.registered.load(Ordering::SeqCst) {
            EngineState::Registered
        } else if self.endpoint_ready.load(Ordering::SeqCst) {
            EngineState::EndpointAvailable
        } else if self.nat_restricted.load(Ordering::SeqCst) {
            EngineState::NATRestricted
        } else {
            self.state
        }
    }
    pub fn last_public_addr(&self) -> Option<SocketAddr> { self.last_public_addr }
    pub fn test_force_stun_consistent(&mut self, addr: SocketAddr) { self.on_stun_consistent(Some(addr)); }

    pub fn set_nat_type(&self, nat: NatType) {
        use std::sync::atomic::Ordering;
        self.nat_type.store(nat as u8, Ordering::SeqCst);
    }
    pub fn nat_type(&self) -> NatType {
        use std::sync::atomic::Ordering;
        match self.nat_type.load(Ordering::SeqCst) {
            1 => NatType::NoConnection,
            2 => NatType::Symmetric,
            3 => NatType::Restricted,
            4 => NatType::FullCone,
            _ => NatType::Unknown,
        }
    }

    /// Internal setter used by BingleApiInternal to update engine state in a thread-safe way.
    /// Currently supports transitioning to EndpointAvailable.
    pub fn set_state_internal(&self, new_state: EngineState) -> bool {
        use std::sync::atomic::Ordering;
        match new_state {
            EngineState::EndpointAvailable => {
                self.endpoint_ready.store(true, Ordering::SeqCst);
                true
            }
            EngineState::Registered => {
                self.registered.store(true, Ordering::SeqCst);
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
}


impl Engine {
    /// Test-only accessors to the TURN handler instances (exposed for integration tests).
    pub fn turn_client_handler_for_tests(&self) -> std::sync::Arc<crate::turn::turn_client_handler_impl::TurnClientHandlerImpl> {
        self.turn_handler_client.clone()
    }
    pub fn turn_relay_handler_for_tests(&self) -> std::sync::Arc<crate::turn::turn_relay_handler_impl::TurnRelayHandlerImpl> {
        self.turn_handler_relay.clone()
    }
}

impl Engine {
    /// Relay-side: register a listener relay id -> address mapping (non-test API)
    pub fn turn_relay_handle_listen(&self, relay_id: &str, relay_addr: &SocketAddr) -> bool {
        self.turn_handler_relay.handle_listen(relay_id, relay_addr)
    }

    /// Relay-side: lookup address by id (non-test API)
    pub fn turn_relay_lookup_addr_by_id(&self, relay_id: &str) -> Option<SocketAddr> {
        self.turn_handler_relay.lookup_addr_by_id(relay_id)
    }

    /// Relay-side: handle a Call by allocating channel (non-test API)
    pub fn turn_relay_handle_call(&self, source: SocketAddr, dest: SocketAddr) -> i32 {
        crate::turn::turn_handler::TurnRelayHandler::handle_call(&*self.turn_handler_relay, &source, &dest)
    }

    /// Client-side: record ListenResponse mapping (non-test API)
    pub fn turn_client_handle_listen_response(&self, relay_addr: SocketAddr, relay_id: &str) {
        crate::turn::turn_handler::TurnClientHandler::handle_listen_response(&*self.turn_handler_client, &relay_addr, relay_id);
    }

    /// Client-side: record CallResponse mapping (non-test API)
    pub fn turn_client_handle_call_response(&self, source: SocketAddr, dest: SocketAddr, channel: u16, relay_id: &str) {
        crate::turn::turn_handler::TurnClientHandler::handle_call_response(&*self.turn_handler_client, &source, &dest, channel, relay_id);
    }

    /// Client-side: record Called mapping (non-test API)
    pub fn turn_client_handle_called(&self, source: SocketAddr, dest: SocketAddr, channel: u16) {
        crate::turn::turn_handler::TurnClientHandler::handle_called(&*self.turn_handler_client, &source, &dest, channel);
    }
}
