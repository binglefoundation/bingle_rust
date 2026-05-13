use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::api::bingle_api::{BingleError, NetworkEndpoint, StartOptions, UserId};
use crate::themes;
use crate::{info_theme, warn_theme, debug_theme};
use crate::blockchain::algo_ops::AlgoChainConfig;
use crate::ddb::{AdvertRecord, DdbBackend, InetSocketAddress};
use crate::distributed_mutex::DistributedMutex;
use crate::dtls::{Dtls, DtlsOpenSsl, NetworkMux, UdpNetworkMux};
use crate::messages::handlers::MessageHandler;
use crate::messages::types::{Message, RelayMessage, RelayTriangleTest1};
use crate::messages::{from_json_str, DefaultPrintingHandler};
use crate::relay::relay_finder::{RelayFinder, RelayInfo, RelayFinderTrait};
use crate::stun::endpoint_finder::StunEndpointFinder;
use crate::stun::endpoint_finder_impl::StunEndpointFinderImpl;
use crate::turn::turn_handler::TurnHandler;
use uuid::Uuid;

// Helper: count peer states (excluding self) from finder caches
fn count_peer_states(
    finder: &dyn RelayFinderTrait,
    my_id: &str,
) -> (usize, usize) {
    let peers = finder.list_root_relays(my_id, false);
    let mut available = 0usize;
    let mut starting = 0usize;
    for r in peers {
        if let Some(st) = r.state {
            match st {
                RelayState::Available => available += 1,
                RelayState::Starting => starting += 1,
                RelayState::Loading => {}
                RelayState::Loaded => {}
                RelayState::Off => {}
                RelayState::Own => {}
            }
        }
    }
    (available, starting)
}

#[derive(Debug, Default)]
pub struct ResponseWait {
    pub responded: bool,
    pub response: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    StunIdentify,
    TrianglePing,
    EndpointAvailable,
    Registered,
    NATRestricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatType {
    Unknown = 0,
    NoConnection = 1,
    Symmetric = 2,
    Restricted = 3,
    FullCone = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelayState {
    Off,
    Starting,
    Loading,
    Loaded,
    Available,
    Own,
}

pub type EngineType = std::sync::Arc<Engine>;

pub trait BingleAccess<T: ?Sized> {
    fn access<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R;
}

impl<T: ?Sized> BingleAccess<T> for std::sync::Arc<T> {
    fn access<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        f(self.as_ref())
    }
}

impl<T: ?Sized> BingleAccess<T> for std::sync::Weak<T> {
    fn access<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let arc = self
            .upgrade()
            .expect("Bingle item dropped (Weak upgrade failed)");
        f(arc.as_ref())
    }
}

pub trait BingleAccessUnsafeForTests<T: ?Sized> {
    /// Test-only escape hatch: get a mutable reference out of an `Arc`/`Weak` without locking.
    ///
    /// # Safety
    /// This is intentionally unsafe-ish: it can create mutable aliasing if other strong/weak
    /// references are used concurrently. Use only in single-threaded tests where you control
    /// all references.
    fn access_unsafe_for_tests<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R;
}

impl<T: ?Sized> BingleAccessUnsafeForTests<T> for std::sync::Arc<T> {
    fn access_unsafe_for_tests<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        // We can't use Arc::get_mut as it requires &mut self.
        // But this is access_unsafe_for_tests, so we just use the pointer.
        unsafe {
            let ptr = std::sync::Arc::as_ptr(self) as *mut T;
            f(&mut *ptr)
        }
    }
}

impl<T: ?Sized> BingleAccessUnsafeForTests<T> for std::sync::Weak<T> {
    fn access_unsafe_for_tests<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let arc = self
            .upgrade()
            .expect("Bingle item dropped (Weak upgrade failed)");

        unsafe {
            let ptr = std::sync::Arc::as_ptr(&arc) as *mut T;
            f(&mut *ptr)
        }
    }
}

/// Minimal Engine implementation that wires UDP mux + DTLS and routes inbound JSON messages.
pub struct Engine {
    // Distributed mutex used to coordinate relay initialization across peer relays
    relay_init_mutex:
        Option<std::sync::Arc<crate::distributed_mutex::ModifiedLamportDistributedMutex>>,
    options: StartOptions,
    mux: Option<Arc<UdpNetworkMux>>, // concrete to access start/stop helpers
    // Underlying DTLS listener; per-connection adapters delegate to this
    dtls: Box<dyn Dtls + Send + Sync>,
    state: EngineState,
    relay_state: RelayState,
    last_public_addr_shared: Arc<Mutex<Option<SocketAddr>>>,
    stun: Option<Arc<Mutex<Box<dyn StunEndpointFinder + Send + Sync>>>>, // background STUN
    relay_finder: Option<Arc<RelayFinder>>, // used to locate peer relay
    triangle_wait: Option<(Arc<(Mutex<bool>, Condvar)>, Instant)>, // wait for TriangleTest3
    // Callback to send messages via the Bingle protocol (API surface) instead of direct DTLS
    send_via_bingle:
        Option<Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync>>,
    // Unified BingleApi handle bound to this engine instance (non-optional)
    bingle_api: crate::api::bingle_api::BingleApiBothType,
    // Async readiness flag: once set, engine_state_for_tests should report EndpointAvailable
    endpoint_ready: std::sync::atomic::AtomicBool,
    // Flag indicating NAT restricted state when endpoint is not yet available
    nat_restricted: std::sync::atomic::AtomicBool,
    // Flag indicating we have registered our endpoint in the DDB
    registered: std::sync::atomic::AtomicBool,
    // Current NAT type classification
    nat_type: std::sync::atomic::AtomicU8,
    // Per-connection state tracked at the Engine level (keyed by NetworkEndpointKey)
    connections: std::sync::Arc<
        std::sync::Mutex<
            std::collections::HashMap<crate::api::bingle_api::NetworkEndpointKey, ConnectionEntry>,
        >,
    >,
    // Pending responses map and issuer state moved from BingleApiImpl
    pending_responses: Arc<Mutex<HashMap<Uuid, Arc<(Mutex<ResponseWait>, Condvar)>>>>,
    issuer: Option<Arc<String>>,
    // In-memory DDB backend used by relay nodes (and for tests)
    ddb_backend: std::sync::Arc<std::sync::Mutex<crate::ddb::InMemoryDdbBackend>>,
    // Per-API router instance used to avoid global mutable state
    router: Option<std::sync::Arc<crate::messages::router::Router>>,
    // DDB client bound to the API instance (always present; may be a NullDdbClient)
    ddb_client: std::sync::Arc<dyn crate::ddb::DdbClient>,
    // TURN handlers (split): client and relay variants
    turn_handler_client:
        Option<std::sync::Arc<crate::turn::turn_client_handler_impl::TurnClientHandlerImpl>>,
    turn_handler_relay: Option<std::sync::Arc<crate::turn::turn_relay_handler_impl::TurnRelayHandlerImpl>>,
    // Application-level callback for listening state changes (set by API)
    on_listening_cb: std::sync::Arc<
        std::sync::Mutex<Option<std::sync::Arc<crate::api::bingle_api::OnListeningHandler>>>,
    >,
    // When loading DDB from a peer, store the reported record count
    peer_ddb_records: Option<usize>,
    // Signon completion signal
    signon_complete: Arc<(Mutex<bool>, Condvar)>,
    // Set of endpoints we have seen (sent to)
    seen_endpoints: Arc<Mutex<std::collections::HashSet<InetSocketAddress>>>,
    pub(crate) span: tracing::Span,
}

impl Engine {
    pub fn relay_state_str(&self) -> String {
        match self.relay_state {
            RelayState::Off => "off".to_string(),
            RelayState::Starting => "starting".to_string(),
            RelayState::Loading => "loading".to_string(),
            RelayState::Loaded => "loaded".to_string(),
            RelayState::Available => "available".to_string(),
            RelayState::Own => "own".to_string(),
        }
    }

    fn relay_state_to_str_static(st: RelayState) -> &'static str {
        match st {
            RelayState::Off => "off",
            RelayState::Starting => "starting",
            RelayState::Loading => "loading",
            RelayState::Loaded => "loaded",
            RelayState::Available => "available",
            RelayState::Own => "own",
        }
    }

    pub(crate) fn set_relay_state(&mut self, new_state: RelayState, reason: &str) {
        let span = self.span.clone();
        let _guard = span.enter();
        let prev = self.relay_state;
        let prev_str = Self::relay_state_to_str_static(prev);
        let new_str = Self::relay_state_to_str_static(new_state);
        if prev != new_state {
            info_theme!(
                themes::ENGINE,
                "[Engine] relay_state change: {} -> {} reason={}",
                prev_str,
                new_str,
                reason
            );
        } else {
            info_theme!(
                themes::ENGINE,
                "[Engine] relay_state set to {} again reason={}",
                new_str,
                reason
            );
        }
        self.relay_state = new_state;
    }

    pub fn set_last_public_addr(&mut self, addr: Option<SocketAddr>) {
        info_theme!(themes::ENGINE, "[Engine] set_last_public_addr: {:?}", addr);
        if let Ok(mut g) = self.last_public_addr_shared.lock() {
            *g = addr;
        } else {
            tracing::error!("[Engine] last_public_addr_shared lock failed; replacing Arc");
            self.last_public_addr_shared = Arc::new(Mutex::new(addr));
        }
    }

    /// Return the appropriate TURN handler for current role (client vs relay)
    pub fn get_approp_turn_handler(&self) -> std::sync::Arc<dyn TurnHandler + Send + Sync> {
        if self.options.am_relay {
            self.turn_handler_relay.clone().expect("Relay must have turn_handler_relay")
        } else {
            self.turn_handler_client.clone().expect("Non-relay must have turn_handler_client")
        }
    }

    /// Upsert a list of root relays into the in-memory DDB backend (as am_relay=true records).
    fn upsert_roots_into_backend(&self, roots: &[RelayInfo]) {
        if roots.is_empty() {
            debug_theme!(themes::ENGINE, "[Engine::upsert_roots_into_backend] no roots to upsert");
            return;
        }
        info_theme!(
            themes::ENGINE,
            "[Engine::upsert_roots_into_backend] upserting {} root relay record(s)",
            roots.len()
        );
        if let Ok(mut b) = self.ddb_backend.lock() {
            for r in roots {
                let host = match r.address.ip() {
                    IpAddr::V4(v4) => v4.to_string(),
                    IpAddr::V6(v6) => v6.to_string(),
                };
                tracing::debug!(
                    "[Engine::upsert_roots_into_backend] upsert id={} addr={}:{}",
                    r.id,
                    host,
                    r.address.port()
                );
                let rec = AdvertRecord {
                    id: r.id.clone(),
                    endpoint: Some(InetSocketAddress {
                        host,
                        port: r.address.port(),
                    }),
                    am_relay: Some(true),
                    relay_id: None,
                    relay_sig: None,
                    date: "1970-01-01T00:00:00Z".to_string(),
                    sig: None,
                };
                b.upsert(rec);
            }
            tracing::info!("[Engine::upsert_roots_into_backend] upsert complete");
        } else {
            warn_theme!(themes::ENGINE, "[Engine::upsert_roots_into_backend] failed to lock ddb_backend for upsert");
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
    pub fn app_id(&self) -> Option<u64> {
        self.options.app_id
    }
    pub fn algo_provider_config(&self) -> Option<AlgoChainConfig> {
        self.options.algo_provider_config.clone()
    }

    /// Install or clear the application-level OnListening handler (set by API).
    pub fn set_on_listening_handler(
        &mut self,
        cb: Option<std::sync::Arc<crate::api::bingle_api::OnListeningHandler>>,
    ) {
        if let Ok(mut g) = self.on_listening_cb.lock() {
            *g = cb;
        }
    }

    /// Notify the application-level OnListening handler, if installed.
    pub fn notify_listening(&self, listening: bool) {
        let nat = self.nat_type();
        if let Ok(g) = self.on_listening_cb.lock() {
            if let Some(cb) = &*g {
                cb(listening, nat);
            }
        }
    }

    /// Create a common TURN handler for both relay and client modes
    fn create_turn_handler(
        &self,
    ) -> std::sync::Arc<dyn Fn(&dyn NetworkMux, &SocketAddr, &[u8]) + Send + Sync> {
        let am_relay = self.options.am_relay;
        let turn: std::sync::Arc<dyn TurnHandler + Send + Sync> = self.get_approp_turn_handler();
        let last_public_addr_shared = self.last_public_addr_shared.clone();

        Arc::new(
            move |source: &dyn NetworkMux, from: &SocketAddr, packet: &[u8]| {
                let local_public_addr = match last_public_addr_shared.lock() {
                    Ok(g) => match *g {
                        Some(addr) => addr,
                        None => {
                            tracing::error!("[Engine][TURN] no last_public_addr_shared; cannot handle TURN packet");
                            return;
                        }
                    },
                    Err(_) => {
                        tracing::error!("[Engine][TURN] failed to lock last_public_addr_shared; cannot handle TURN packet");
                        return;
                    }
                };


                // Parse/unwrap the TURN ChannelData using our handler
                if let Some(wrapped) =
                    turn.handle_turn_incoming(Some(from), Some(local_public_addr), packet)
                {
                    if am_relay {
                        if wrapped.is_relay_local {
                            // This is the special case where a relay (us) is sending via another relay
                            if let Some(udp) = source
                                .as_any()
                                .downcast_ref::<crate::dtls::network_mux_udp::UdpNetworkMux>(
                            ) {
                                tracing::info!(
                                    "[Engine][TURN relay] message for own relay node, re-injecting {} bytes from {}",
                                    wrapped.message.len(),
                                    wrapped.network_endpoint
                                );
                                udp.reprocess(&wrapped.network_endpoint, &wrapped.message);
                            } else {
                                tracing::warn!(
                                    "[Engine][TURN relay] message for own relay node but source is not UdpNetworkMux"
                                );
                            }
                        } else {
                            tracing::info!(
                                "[Engine][TURN] handle_turn_incoming (relay) {} bytes from {}:",
                                wrapped.message.len(),
                                wrapped.network_endpoint
                            );
                            // Relay role: forward stripped payload to resolved ip_address via concrete UDP mux
                            if let Some(udp) = source
                                .as_any()
                                .downcast_ref::<crate::dtls::network_mux_udp::UdpNetworkMux>(
                            ) {
                                let nsk = crate::api::bingle_api::NetworkEndpoint::new_direct(
                                    wrapped.ip_address,
                                );
                                // Here we forward the TURN packet including channel number to the resolved ip_address
                                if let Err(e) = udp.write(&nsk, &packet) {
                                    tracing::warn!(
                                        "[Engine][TURN relay] forward to {} failed: {}",
                                        wrapped.ip_address,
                                        e
                                    );
                                } else {
                                    tracing::info!(
                                        "[Engine][TURN relay] forwarded {} bytes to {}",
                                        wrapped.message.len(),
                                        wrapped.ip_address
                                    );
                                }
                            } else {
                                tracing::warn!(
                                    "[Engine][TURN relay] source is not UdpNetworkMux; cannot forward"
                                );
                            }
                        }
                    } else {
                        tracing::info!(
                            "[Engine][TURN] handle_turn_incoming (not relay) {} bytes from {}:",
                            wrapped.message.len(),
                            wrapped.network_endpoint
                        );
                        // Non-relay role: this packet is for us. Re-inject the stripped payload into the UDP mux
                        if let Some(udp) = source
                            .as_any()
                            .downcast_ref::<crate::dtls::network_mux_udp::UdpNetworkMux>(
                        ) {
                            udp.reprocess(&wrapped.network_endpoint, &wrapped.message);
                            tracing::info!(
                                "[Engine][TURN client] reprocessed {} bytes from {}",
                                wrapped.message.len(),
                                wrapped.network_endpoint
                            );
                        } else {
                            tracing::warn!(
                                "[Engine][TURN client] source is not UdpNetworkMux; cannot reprocess"
                            );
                        }
                    }
                } else {
                    tracing::warn!("[Engine][TURN] handle_turn_incoming returned None (ignored)");
                }
            },
        )
    }
}

// Adapter exposing minimal internal controls for handlers -> engine without referencing BingleApiImpl
// Removed: using BingleApiBoth directly via LockingApiWrapper in Router.

// Per-connection state holding a DTLS adapter bound to a specific peer
struct ConnectionEntry {
    last_seen: Instant,
}

impl Engine {
    pub fn new(options: &StartOptions, api: crate::api::bingle_api::BingleApiBothType) -> Self {
        let dtls: Box<dyn Dtls + Send + Sync> =
            Box::new(DtlsOpenSsl::new(options.handle.clone()).with_dangerous_debug(options.dangerous_debug));
        Self::new_with_dtls(options, api, dtls)
    }

    pub fn new_with_dtls(
        options: &StartOptions,
        api: crate::api::bingle_api::BingleApiBothType,
        dtls: Box<dyn Dtls + Send + Sync>,
    ) -> Self {
        tracing::info!("[Engine::new] options={:?}", options);
        #[allow(unused)]
        {}
        // Build a DDB client now (always present); choose real or null implementation
        let ddb: std::sync::Arc<dyn crate::ddb::DdbClient> = {
            let have_app = options.app_id.or_else(|| {
                std::env::var("BINGLE_APP_ID")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
            });
            if have_app.is_none() {
                tracing::error!("[Engine::new] no BINGLE_APP_ID set will use NullDdbClient");
            }
            if let Some(app_id) = have_app {
                std::sync::Arc::new(crate::ddb::DdbClientImpl::new(
                    api.clone(),
                    app_id,
                    options.algo_provider_config.clone(),
                ))
            } else {
                std::sync::Arc::new(crate::ddb::NullDdbClient::new())
            }
        };

        let mut eng = Self {
            relay_init_mutex: None,
            options: options.clone(),
            mux: None,
            dtls,
            state: EngineState::StunIdentify,
            relay_state: RelayState::Off,
            last_public_addr_shared: Arc::new(Mutex::new(None)),
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
            ddb_backend: std::sync::Arc::new(std::sync::Mutex::new(
                crate::ddb::InMemoryDdbBackend::new(),
            )),
            router: None,
            ddb_client: ddb,
            turn_handler_client: if options.am_relay { None } else { Some(std::sync::Arc::new(
                crate::turn::turn_client_handler_impl::TurnClientHandlerImpl::new(),
            )) },
            turn_handler_relay: if options.am_relay {Some(std::sync::Arc::new(
                crate::turn::turn_relay_handler_impl::TurnRelayHandlerImpl::new(),
            )) } else { None},
            on_listening_cb: std::sync::Arc::new(std::sync::Mutex::new(None)),
            peer_ddb_records: None,
            signon_complete: Arc::new((Mutex::new(false), Condvar::new())),
            seen_endpoints: Arc::new(Mutex::new(std::collections::HashSet::new())),
            span: tracing::Span::none(),
        };
        eng.set_last_public_addr(options.static_ip.clone());
        eng
    }

    /// Provide a per-API router instance to avoid global state collisions across APIs/tests.
    pub fn set_router(&mut self, router: std::sync::Arc<crate::messages::router::Router>) {
        self.router = Some(router);
    }

    /// Test helper: Inject a custom DTLS implementation.
    pub fn set_dtls(&mut self, dtls: Box<dyn Dtls + Send + Sync>) {
        self.dtls = dtls;
    }

    /// Access the configured DTLS instance, if any (read-only).
    pub fn dtls(&self) -> &(dyn Dtls + Send + Sync) {
        self.dtls.as_ref()
    }

    /// Test helper: Inject a custom DDB client.
    pub fn set_ddb_client_for_tests(&mut self, ddb: Arc<dyn crate::ddb::DdbClient>) {
        self.ddb_client = ddb;
    }

    /// Test helper: get the local UDP bind address of the mux, if started.
    pub fn local_bind_addr_for_tests(&self) -> Option<SocketAddr> {
        if let Some(m) = &self.mux {
            let mut addr = m.local_addr().ok()?;
            if addr.ip().is_unspecified() {
                addr.set_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
            }
            Some(addr)
        } else {
            None
        }
    }

    /// Test helper: simulate receiving a message from the network.
    pub fn receive_message_for_tests(&mut self, from_ep: &NetworkEndpoint, data: &[u8]) {
        let msg = match crate::messages::marshal::from_json_str(&String::from_utf8_lossy(data)) {
            Ok(m) => m,
            Err(_) => {
                tracing::warn!("[Engine::receive_message_for_tests] failed to parse message: {:?}", String::from_utf8_lossy(data));
                return;
            },
        };
        if let Some(r) = &self.router {
            let handler = DefaultPrintingHandler;
            let issuer = self.issuer.as_ref().map(|i| i.as_str()).unwrap_or("test");
            r.route_with_network(&handler, &msg, issuer, from_ep);
        } else {
            tracing::warn!("[Engine::receive_message_for_tests] no router available");
        }
    }

    /// Apply a closure to the DTLS instance.
    pub fn with_dtls_mut<F: FnOnce(&mut (dyn Dtls + Send + Sync))>(&mut self, f: F) {
        f(self.dtls.as_mut())
    }

    /// Set and get issuer moved from API layer.
    pub fn set_issuer(&mut self, issuer: String) {
        tracing::info!("[Engine::set_issuer] issuer={}", issuer);
        self.issuer = Some(Arc::from(issuer));
    }
    /// Return the issuer string (id + ISSUER_SUFFIX) or an error if not set. Logs a warning on None.
    pub fn issuer(&self) -> Result<&str, String> {
        if let Some(ref s) = self.issuer {
            Ok(s.as_str())
        } else {
            tracing::warn!("[Engine::issuer] issuer not set");
            Err("issuer not set".to_string())
        }
    }

    /// Pending response registration/fulfillment helpers
    pub fn register_pending(&self, tag: Uuid) {
        let pair = Arc::new((Mutex::new(ResponseWait::default()), Condvar::new()));
        if let Ok(mut m) = self.pending_responses.lock() {
            m.insert(tag, pair);
        }
    }
    pub fn fulfill_pending(&self, tag: &Uuid, response: serde_json::Value) -> bool {
        let pair_opt = {
            match self.pending_responses.lock() {
                Ok(m) => m.get(tag).cloned(),
                Err(_) => None,
            }
        };
        if let Some(pair) = pair_opt {
            let (lock, cvar) = (&pair.0, &pair.1);
            if let Ok(mut g) = lock.lock() {
                g.responded = true;
                g.response = Some(response);
                cvar.notify_all();
            }
            true
        } else {
            false
        }
    }

    /// Returns a clone of the pending responses map Arc to allow waiting without holding the Engine lock.
    pub fn pending_responses_arc(
        &self,
    ) -> Arc<Mutex<HashMap<Uuid, Arc<(Mutex<ResponseWait>, Condvar)>>>> {
        self.pending_responses.clone()
    }

    pub fn wait_for_response(&self, tag: &Uuid, timeout: Duration) -> Option<serde_json::Value> {
        let pending = self.pending_responses.clone();
        Self::wait_for_response_static(pending, tag, timeout)
    }

    /// Static version of wait_for_response that can be called without holding the Engine lock.
    pub fn wait_for_response_static(
        pending: Arc<Mutex<HashMap<Uuid, Arc<(Mutex<ResponseWait>, Condvar)>>>>,
        tag: &Uuid,
        timeout: Duration,
    ) -> Option<serde_json::Value> {
        let pair_opt = {
            match pending.lock() {
                Ok(m) => m.get(tag).cloned(),
                Err(_) => None,
            }
        };
        if let Some(pair) = pair_opt {
            let (lock, cvar) = (&pair.0, &pair.1);
            if let Ok(mut g) = lock.lock() {
                let start = Instant::now();
                loop {
                    if g.responded {
                        break;
                    }
                    let remaining = timeout.saturating_sub(start.elapsed());
                    if remaining.is_zero() {
                        break;
                    }
                    let (gg, res) = cvar
                        .wait_timeout(g, remaining)
                        .expect("condvar wait failed");
                    g = gg;
                    if res.timed_out() && !g.responded {
                        break;
                    }
                }
                let out = if g.responded { g.response.take() } else { None };
                drop(g);
                // cleanup
                if let Ok(mut m) = pending.lock() {
                    m.remove(tag);
                }
                out
            } else {
                None
            }
        } else {
            None
        }
    }
    pub fn remove_pending(&self, tag: &Uuid) -> bool {
        if let Ok(mut m) = self.pending_responses.lock() {
            m.remove(tag).is_some()
        } else {
            false
        }
    }

    pub fn signal_signon_complete(&self) {
        let (lock, cvar) = &*self.signon_complete;
        if let Ok(mut complete) = lock.lock() {
            tracing::info!("[Engine::signal_signon_complete] signaling complete=true");
            *complete = true;
            cvar.notify_all();
        }
    }

    pub fn reset_signon_complete(&self) {
        let (lock, _) = &*self.signon_complete;
        if let Ok(mut complete) = lock.lock() {
            tracing::info!("[Engine::reset_signon_complete] resetting complete=false");
            *complete = false;
        }
    }

    pub fn await_signon_complete(&self, timeout: Duration) -> bool {
        let (lock, cvar) = &*self.signon_complete;
        let mut complete = lock.lock().unwrap();
        let start = Instant::now();
        tracing::info!("[Engine::await_signon_complete] awaiting signon completion with timeout {:?}", timeout);
        while !*complete {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                tracing::warn!("[Engine::await_signon_complete] timed out");
                return false;
            }
            let (new_complete, wait_res) = cvar.wait_timeout(complete, remaining).unwrap();
            complete = new_complete;
            if wait_res.timed_out() && !*complete {
                tracing::warn!("[Engine::await_signon_complete] wait_timeout returned timed out");
                return false;
            }
        }
        tracing::info!("[Engine::await_signon_complete] signon complete observed");
        true
    }

    /// Install a Bingle protocol sender callback for Engine-initiated messages.
    pub fn set_send_via_bingle(
        &mut self,
        cb: Option<Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync>>,
    ) {
        self.send_via_bingle = cb;
    }

    /// Set or replace the BingleApi handle bound to this Engine instance.
    pub fn set_bingle_api(&mut self, api: crate::api::bingle_api::BingleApiBothType) {
        self.bingle_api = api.clone();
        // Initialize a DDB client bound to this API instance (always set; may be Null)
        {
            let have_app = self.options.app_id.or_else(|| {
                std::env::var("BINGLE_APP_ID")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
            });
            self.ddb_client = if let Some(app_id) = have_app {
                std::sync::Arc::new(crate::ddb::DdbClientImpl::new(
                    api.clone(),
                    app_id,
                    self.options.algo_provider_config.clone(),
                ))
            } else {
                std::sync::Arc::new(crate::ddb::NullDdbClient::new())
            };
        }
    }

    /// Clear bindings to API instance and global router callbacks to avoid dangling pointers between tests.
    pub fn clear_api_bindings(&mut self) {
        // Clear per-API router instance only (no global fallbacks)
        if let Some(r) = &self.router {
            r.clear_for_tests();
        }
        // Also drop local references
        self.send_via_bingle = None;
    }

    /// Check whether the engine believes a connection to endpoint exists.
    pub fn has_connection(&self, endpoint: &crate::api::bingle_api::NetworkEndpoint) -> bool {
        if let Some(key) = endpoint.get_key() {
            self.connections
                .lock()
                .map(|m| m.contains_key(&key))
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Testing helper: number of tracked connections.
    pub fn connections_len_for_tests(&self) -> usize {
        self.connections.lock().map(|m| m.len()).unwrap_or(0)
    }

    /// Testing helper: get copy of seen endpoints.
    pub fn seen_endpoints_for_tests(&self) -> Vec<InetSocketAddress> {
        self.seen_endpoints.lock().unwrap().iter().cloned().collect()
    }

    /// Send bytes to a peer and track the connection's last_seen.
    /// If this is the first interaction with the peer, create a connection entry on successful send.
    pub fn send_to_peer(
        &self,
        to: &crate::api::bingle_api::NetworkEndpoint,
        data: &[u8],
    ) -> Result<(), String> {
        tracing::info!("[Engine::send_to_peer] {}, {:?}", to, data);
        // Guard: reject incomplete relay endpoints (missing channel); fully-configured
        // relay endpoints (with channel+address) are handled by the TURN layer in DTLS.
        if to.is_relay() && to.relay_channel().is_none() {
            return Err(format!("[Engine::send_to_peer] rejecting incomplete relay endpoint (no channel): {}", to));
        }
        // Guard: do not send to ourselves
        if let Some(target_addr) = to.inet_socket_address() {
            if self.last_public_addr() == Some(target_addr) {
                return Err(format!("[Engine::send_to_peer] rejecting send to self: {}", target_addr));
            }
        }
        // Perform the DTLS send using the configured DTLS instance (avoid pre-locking connections to
        // prevent rare OS mutex EINVAL during early send paths). We update the connection map only
        // after a successful send.
        let res = self.dtls.send(to, data);
        if res.is_ok() {
            // Track seen endpoints
            if let Some(addr) = to.inet_socket_address() {
                if let Ok(mut seen) = self.seen_endpoints.lock() {
                    seen.insert(addr.into());
                }
            }
            // Track connection using NetworkEndpointKey derived from `to`
            if let Some(key) = to.get_key() {
                if let Ok(mut m) = self.connections.lock() {
                    use std::collections::hash_map::Entry;
                    match m.entry(key) {
                        Entry::Occupied(mut e) => {
                            e.get_mut().last_seen = Instant::now();
                        }
                        Entry::Vacant(v) => {
                            v.insert(ConnectionEntry {
                                last_seen: Instant::now(),
                            });
                        }
                    }
                }
                else {
                    tracing::warn!("[Engine::send_to_peer] could not lock connections");
                }
            }
            else {
                tracing::warn!("[Engine::send_to_peer] could not get key from NetworkEndpoint");
            }
        }
        res
    }

    /// Send a message to all known relays (except ourselves and the message originator).
    pub fn ripple_message(&self, message: serde_json::Value, originator_id: String, ddb: &dyn DdbBackend) {
        tracing::info!("[Engine::ripple_message] originator={}", originator_id);
        let my_id = match self.issuer() {
            Ok(iss) => iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string(),
            Err(_) => {
                tracing::warn!("[Engine::ripple_message] issuer not set, cannot ripple");
                return;
            }
        };
        let (relay_ids, relay_endpoints_opt) = ddb.make_epoch_info();

        // Use endpoints from make_epoch_info if available, otherwise try to look them up individually
        if let Some(endpoints) = relay_endpoints_opt {
            for (id, endpoint) in relay_ids.into_iter().zip(endpoints.into_iter()) {
                if id == my_id || id == originator_id {
                    tracing::debug!("[Engine::ripple_message] skipping relay {}", id);
                    continue;
                }
                if let Ok(addr) = std::net::SocketAddr::try_from(endpoint) {
                    tracing::info!("[Engine::ripple_message] sending to relay={} at {:?}", id, addr);
                    let nsk = crate::api::bingle_api::NetworkEndpoint::new_direct(addr);
                    if let Some(api) = self.bingle_api.upgrade() {
                        let _ = api.send_message_to_network(&nsk, &id, message.clone(), None);
                    }
                }
            }
        } else {
            for id in relay_ids {
                if id == my_id || id == originator_id {
                    tracing::debug!("[Engine::ripple_message] skipping relay {}", id);
                    continue;
                }
                if let Some(rec) = ddb.lookup(&id) {
                    if let Some(endpoint) = rec.endpoint {
                        if let Ok(addr) = std::net::SocketAddr::try_from(endpoint) {
                            tracing::info!("[Engine::ripple_message] sending to relay={} at {:?}", id, addr);
                            let nsk = crate::api::bingle_api::NetworkEndpoint::new_direct(addr);
                            if let Some(api) = self.bingle_api.upgrade() {
                                let _ = api.send_message_to_network(&nsk, &id, message.clone(), None);
                            }
                        }
                    }
                }
            }
        }
    }

    /// List all known relays using the current RelayFinder instance, if available.
    /// Returns an empty vector if the engine has not initialized discovery yet
    /// or if our issuer/id is not set.
    pub fn list_all_relays(&self, include_self: bool) -> Vec<RelayInfo> {
        let span = self.span.clone();
        let _guard = span.enter();
        tracing::info!("[Engine::list_all_relays] include_self={}", include_self);
        let my_id = match self.issuer() {
            Ok(iss) => iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string(),
            Err(e) => {
                tracing::warn!("[Engine::list_all_relays] issuer unavailable: {}", e);
                return Vec::new();
            }
        };
        match &self.relay_finder {
            Some(finder) => {
                let res = finder.list_all_relays(&my_id, include_self);
                tracing::info!("[Engine::list_all_relays] returning {} relays", res.len());
                res
            }
            None => {
                tracing::warn!("[Engine::list_all_relays] relay_finder not initialized");
                Vec::new()
            }
        }
    }

    /// Install or wrap the DTLS handle_message callback to delegate into the Engine routing logic.
    /// This avoids duplicating the same closure in different Engine start paths.
    fn install_dtls_handler(&mut self) -> Result<(), BingleError> {
        // Capture any existing handler without taking a mutable borrow to self.dtls
        let existing = self.dtls.get_handle_message();

        // Capture safe, shareable state for the handler closure (avoid raw self pointers)
        let connections = self.connections.clone();
        let pending_responses = self.pending_responses.clone();
        let _ = self.send_via_bingle.clone();
        let bingle_api = self.bingle_api.clone();
        let am_relay = self.options.am_relay;
        let ddb_backend = self.ddb_backend.clone();
        let span = self.span.clone();

        // Now obtain a mutable reference to dtls only for installing the new handler
        let d = self.dtls.as_mut();
        let router_arc = self.router.clone();
        d.set_handle_message(Some(std::sync::Arc::new(move |server, from, issuer, data| {
                let _guard = span.enter();
                tracing::info!("[Engine::install_dtls_handler][cb] invoked from={} issuer={} bytes={}", from, issuer, data.len());
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
                            tracing::info!("[Engine::install_dtls_handler][cb] checking for responseTag in {}", v);
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
                                                tracing::info!("[Engine::install_dtls_handler][cb] got response, returning");
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
                                tracing::info!("[Engine::install_dtls_handler][cb] routing message {:?}", msg);
                                if let Some(r) = &router_arc {
                                    r.route_with_network(&handler, &msg, issuer, &from);
                                    if let Some(out) = r.take_outbound_response() {
                                        tracing::info!("[Engine::install_dtls_handler][cb] sending response {:?}", out);
                                        let bytes = serde_json::to_vec(&out).unwrap_or_else(|_| b"{}".to_vec());
                                        {
                                            if let Err(e) = server.send(&from, &bytes) { tracing::warn!("[Engine::install_dtls_handler][send outbound_response] failed: {}", e); }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                // Not valid JSON per our schema; treat as plaintext with raw bytes
                                tracing::warn!("[Engine::install_dtls_handler][cb] not valid json {} {:?}", s, e);
                                handler.on_unimplemented(&crate::messages::Message::Unknown(serde_json::Value::String(s.to_string())));
                            }
                        },
                        Err(e) => {
                            tracing::warn!("[Engine::install_dtls_handler][cb] not UTF-8 {:?}", e);
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
    }

    /// Start the engine using the provided StartOptions.
    /// Implements static endpoint path or STUN-based discovery when not provided.
    pub fn start(&mut self, options: &StartOptions) -> Result<(), BingleError> {
        let span = self.span.clone();
        let _guard = span.enter();
        // Keep a copy of options
        self.options = options.clone();

        if let Some(static_addr) = options.static_ip {
            return self.start_with_addr(options, static_addr);
        }

        // STUN path
        self.state = EngineState::StunIdentify;

        // Bind UDP on 0.0.0.0:0 and create mux (OS assigns an ephemeral port)
        let mut mux0 = UdpNetworkMux::bind("0.0.0.0:0")
            .map_err(|e| BingleError::Other(format!("Failed to bind UDP mux: {}", e)))?;
        let _local_addr: SocketAddr = mux0
            .local_addr()
            .map_err(|e| format!("Failed to get local addr: {}", e))?;

        // Use the pre-configured DTLS instance provided by the API and install message handler
        // We'll detect RelayTriangleTest3 to unblock waiters while still routing to default
        let triangle_signal: Arc<(Mutex<bool>, Condvar)> =
            Arc::new((Mutex::new(false), Condvar::new()));
        let _triangle_signal_clone = triangle_signal.clone();
        // Install the common DTLS handler wrapper
        self.install_dtls_handler()?;

        // Install STUN endpoint finder and hook into mux STUN handler
        let finder: Arc<Mutex<Box<dyn StunEndpointFinder + Send + Sync>>> =
            Arc::new(Mutex::new(Box::new(StunEndpointFinderImpl::new())));
        // Hook STUN packets directly to the finder via a capturing closure
        let finder_for_stun = finder.clone();
        mux0.set_handle_stun(Some(Arc::new(
            move |source: &dyn NetworkMux, from: &SocketAddr, data: &[u8]| {
                let _ = source.as_any(); // silence unused param warning
                if let Ok(mut guard) = finder_for_stun.lock() {
                    guard.process_packet(*from, data);
                }
            },
        )));

        // Configure TURN ChannelData handler based on role (relay vs client)
        tracing::info!("[Engine] set_handle_turn from start");
        let th = self.create_turn_handler();
        mux0.set_handle_turn(Some(&th));

        // Now wrap mux in Arc
        let mut mux0 = mux0;
        mux0.span = self.span.clone();
        let mux = Arc::new(mux0);

        // Start mux thread first so DTLS accept loop can receive
        mux.start()
            .map_err(|e| BingleError::Other(format!("Failed to start UDP mux: {}", e)))?;

        // Start DTLS with mux so that we can send/receive triangle messages over DTLS if needed later
        self.dtls.start(mux.clone())
            .map_err(|e| BingleError::Other(format!("Failed to start DTLS: {}", e)))?;

        // Persist mux, STUN finder, and triangle wait handle before initializing STUN
        self.mux = Some(mux.clone());
        self.stun = Some(finder.clone());
        // Store triangle wait handle for later awaits
        self.triangle_wait = Some((triangle_signal, Instant::now()));

        // After DTLS and mux are running, configure and start STUN finder logic
        self.start_stun_find(&options, &finder, &mux)?;
        Ok(())
    }

    pub(crate) fn initialize_relay(&mut self) {
        let span = self.span.clone();
        let _guard = span.enter();
        tracing::info!("[Engine::initialize_relay] starts for {:?}", self.issuer);
        self.set_relay_state(
            RelayState::Off,
            "initialize_relay: starting sequence (set Off before delay)",
        );

        // Build discovery closure using indexer when app_id is configured; else skip
        {
            let app_id_opt = self.options.app_id.or_else(|| {
                std::env::var("BINGLE_APP_ID")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
            });
            if let Some(app_id) = app_id_opt {
                tracing::info!("[Engine::initialize_relay] app_id configured: {}", app_id);

                let cfg = self.options.algo_provider_config.clone();
                let discover = crate::relay::discovery::indexer_discover_closure(app_id, cfg);
                let finder = crate::relay::relay_finder::RelayFinder::new(
                    self.bingle_api.clone(),
                    Duration::from_secs(60),
                    discover,
                );
                tracing::info!("[Engine::initialize_relay] RelayFinder constructed");

                // Determine our id for exclusion
                let my_id = if let Some(iss) = self.issuer.as_deref() {
                    iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX)
                        .to_string()
                } else {
                    self.options.handle.clone()
                };
                tracing::info!("[Engine::initialize_relay] resolved my_id={}", my_id);

                // Clear any stale state so that we always reload fresh on startup
                finder.clear_state_cache();
                tracing::info!("[Engine::initialize_relay] cleared finder state cache");

                // 1) Seed caches and load current states across the network
                // Load states via RelayCheck for all known relays (must include self)
                finder.load_relay_states(&my_id);
                tracing::info!("[Engine::initialize_relay] loaded peer relay states");
                let mut all_relays = finder.list_all_relays(&my_id, true);
                if !all_relays.iter().any(|r| r.id == my_id) {
                    let addr = self
                        .last_public_addr()
                        .unwrap_or_else(|| "0.0.0.0:0".parse().expect("valid fallback addr"));
                    all_relays.push(RelayInfo {
                        id: my_id.clone(),
                        address: addr,
                        state: Some(RelayState::Starting),
                    });
                }

                // Count peer states (excluding self)
                let (avail_cnt, starting_cnt) = count_peer_states(&finder, &my_id);
                tracing::info!(
                    "[Engine::initialize_relay] peer states after load: available={} starting={}",
                    avail_cnt,
                    starting_cnt
                );

                // Build peer id list including self for the mutex
                tracing::info!(
                    "[Engine::initialize_relay] discovered {:?} relays (including self)",
                    all_relays
                );
                let mut ids: Vec<String> = all_relays.iter().filter(|r| r.state != Some(RelayState::Off)).map(|r| r.id.clone()).collect();
                ids.sort();
                ids.dedup();
                tracing::info!(
                    "[Engine::initialize_relay] mutex participants: {}",
                    ids.len()
                );

                // Prepare sender closures to transmit mutex messages to peers by id (API will resolve addresses)
                let api_weak = self.bingle_api.clone();
                let finder_arc = Arc::new(finder);
                let my_id_for_send = my_id.clone();
                let send_common = move |dest_id: &str, json_val: serde_json::Value| {
                    let uid = dest_id.to_string();
                    // Progress logger for sending
                    // let progress: Arc<ProgressCallback> = Arc::new({
                    //     let uid = uid.clone();
                    //     move |pct: u8, msg: String| {
                    //         tracing::info!(
                    //             "[Engine::initialize_relay][mutex] progress={} dest_id={} msg={}",
                    //             pct,
                    //             uid,
                    //             msg
                    //         );
                    //     }
                    // });
                    let ok =
                        api_weak.access(|a| a.send_message_to_id(&uid, json_val.clone(), None).unwrap_or(false));
                    if !ok {
                        tracing::warn!(
                            "[Engine::initialize_relay][mutex] send_message_to_id failed for {} my_id={} json_val={}",
                            dest_id,
                            my_id_for_send,
                            json_val
                        );
                    }
                };
                let send_request = {
                    let send_common = send_common.clone();
                    move |dest_id: &str, req: &crate::messages::types::MutexRequest| {
                        let msg = crate::messages::types::Message::Mutex(
                            crate::messages::types::MutexMessage::Request(req.clone()),
                        );
                        let json_val = crate::messages::marshal::to_json_value(&msg);
                        send_common(dest_id, json_val);
                    }
                };
                let send_reply = {
                    let send_common = send_common.clone();
                    move |dest_id: &str, resp: &crate::messages::types::MutexResponse| {
                        let msg = crate::messages::types::Message::Mutex(
                            crate::messages::types::MutexMessage::Response(resp.clone()),
                        );
                        let json_val = crate::messages::marshal::to_json_value(&msg);
                        send_common(dest_id, json_val);
                    }
                };
                let send_release = {
                    let send_common = send_common.clone();
                    move |dest_id: &str, rel: &crate::messages::types::MutexRelease| {
                        let msg = crate::messages::types::Message::Mutex(
                            crate::messages::types::MutexMessage::Release(rel.clone()),
                        );
                        let json_val = crate::messages::marshal::to_json_value(&msg);
                        send_common(dest_id, json_val);
                    }
                };
                tracing::info!(
                    "[Engine::initialize_relay] prepared distributed mutex messaging closures"
                );

                // Create and store the distributed mutex
                let mtx = crate::distributed_mutex::ModifiedLamportDistributedMutex::new(
                    my_id.clone(),
                    ids,
                    send_request,
                    send_reply,
                    send_release,
                );
                self.relay_init_mutex = Some(std::sync::Arc::new(mtx));
                tracing::info!("[Engine::initialize_relay] created distributed mutex");

                // Use the mutex to serialize initialization of the DDB one node at a time
                let ddb_backend_arc = self.ddb_backend.clone();
                let roots_copy = all_relays.clone();
                if let Some(m) = self.relay_init_mutex.as_ref().cloned() {
                    let finder_arc_for_mtx = finder_arc.clone();
                    let my_id_for_mtx = my_id.clone();
                    m.acquire(|| {
                        self.set_relay_state(RelayState::Starting, "initialize_relay: mark self Starting before peer discovery and coordination");
                        tracing::info!("[Engine::initialize_relay] entering CS: relay state set to Starting: {}", my_id_for_mtx);

                        // Re-count peer states under the mutex to decide initialization strategy
                        finder_arc_for_mtx.clear_state_cache();
                        tracing::info!("[Engine::initialize_relay] cleared finder state cache");
                        finder_arc_for_mtx.load_relay_states(&my_id);
                        tracing::info!("[Engine::initialize_relay] loaded peer relay states");
                        let (avail_cnt, starting_cnt) = count_peer_states(&*finder_arc_for_mtx, &my_id_for_mtx);
                        tracing::info!("[Engine::initialize_relay] Peer state count: available={}, starting={}", avail_cnt, starting_cnt);
                        if avail_cnt == 0 {
                            tracing::info!("[Engine::initialize_relay] No peers available; initializing DDB directly");
                            // No available peers: upsert roots into backend as bootstrap
                            if let Ok(mut b) = ddb_backend_arc.lock() {
                                for r in &roots_copy {
                                    let host = match r.address.ip() { IpAddr::V4(v4) => v4.to_string(), IpAddr::V6(v6) => v6.to_string() };
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
                                tracing::info!("[Engine::initialize_relay] upserted {} root relay record(s) into backend", roots_copy.len());
                            } else {
                                tracing::warn!("[Engine::initialize_relay] failed to lock ddb_backend during upsert");
                            }
                        } else {
                            // Peers available: start DDB load from a peer
                            // Choose a preferred root peer (not self)
                            let relays = finder_arc_for_mtx.list_root_relays(&my_id_for_mtx, false);
                            if let Some(first) = relays.first() {
                                let peer_id: String = first.id.clone();
                                // Reset signon signal before starting load
                                self.reset_signon_complete();
                                let count_res = self.ddb_client().start_load_from_peer(&peer_id);
                                match count_res {
                                    Ok(n) => {
                                        self.peer_ddb_records = Some(n);
                                        self.set_relay_state(RelayState::Loading, "initialize_relay: started DDB load from peer");
                                        tracing::info!("[Engine::initialize_relay] started DDB load from peer {} with {} records", peer_id, n);

                                        // Wait for signon to complete (signaled from MessageHandler::on_ddb_signon_response)
                                        if !self.await_signon_complete(Duration::from_secs(60)) {
                                            tracing::warn!("[Engine::initialize_relay] Timed out waiting for signon completion");
                                        }

                                        // After loading the peer's DDB state, upsert an AdvertRecord
                                        // for ourselves so the DDB contains both the old state and our
                                        // new relay entry.
                                        let self_addr = self
                                            .last_public_addr()
                                            .unwrap_or_else(|| "0.0.0.0:0".parse().expect("valid fallback addr"));
                                        let self_host = match self_addr.ip() {
                                            IpAddr::V4(v4) => v4.to_string(),
                                            IpAddr::V6(v6) => v6.to_string(),
                                        };
                                        let self_rec = AdvertRecord {
                                            id: my_id_for_mtx.clone(),
                                            endpoint: Some(InetSocketAddress { host: self_host, port: self_addr.port() }),
                                            am_relay: Some(true),
                                            relay_id: None,
                                            relay_sig: None,
                                            date: "1970-01-01T00:00:00Z".to_string(),
                                            sig: None,
                                        };
                                        if let Ok(mut b) = ddb_backend_arc.lock() {
                                            b.upsert(self_rec);
                                            tracing::info!(
                                                "[Engine::initialize_relay] upserted self AdvertRecord into DDB after peer load (id={})",
                                                my_id_for_mtx
                                            );
                                        } else {
                                            tracing::warn!("[Engine::initialize_relay] failed to lock ddb_backend for self upsert after peer load");
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("[Engine::initialize_relay] start_load_from_peer failed for {}: {}", peer_id, e);
                                    }
                                }
                            } else {
                                tracing::warn!("[Engine::initialize_relay] no peer selected for DDB load");
                            }
                        }
                        // Need to await DDB load complete here and signon response
                        tracing::info!("[Engine::initialize_relay] relay initialization CS complete {}", my_id_for_mtx);
                    });
                    // After critical section completes, mark as Available
                    self.set_relay_state(
                        RelayState::Available,
                        "initialize_relay: DDB initialized under mutex",
                    );
                    tracing::info!(
                        "[Engine::initialize_relay] stage complete: DDB initialized and relay marked Available"
                    );
                }
                // Record the finder
                self.relay_finder = Some(finder_arc);
                tracing::info!("[Engine::initialize_relay] stored RelayFinder reference");
            } else {
                tracing::warn!(
                    "[Engine::initialize_relay] am_relay set but app_id not configured; skipping root relay discovery"
                );
                // Even if discovery is skipped, the relay is operational for local tests; mark available.
                self.set_relay_state(RelayState::Available, "initialize_relay: app_id not configured; skipping discovery; mark Available for local operation");
                tracing::warn!(
                    "[Engine::initialize_relay] stage complete: discovery skipped, relay marked Available"
                );
            }
        }
        tracing::info!("[Engine::initialize_relay] complete for {:?}", self.issuer);
    }

    pub(crate) fn initialize_relay_async(&mut self) {
        tracing::info!("[Engine] spawning initialize_relay thread");
        let self_ptr = std::sync::atomic::AtomicPtr::new(self as *mut Engine);
        let span = self.span.clone();
        std::thread::spawn(move || unsafe {
            let _guard = span.enter();
            let eng = &mut *self_ptr.load(std::sync::atomic::Ordering::SeqCst);
            eng.initialize_relay();
        });
    }

    fn start_with_addr(
        &mut self,
        _options: &StartOptions,
        bind_addr: SocketAddr,
    ) -> Result<(), BingleError> {
        tracing::info!("[Engine] start_with_addr: bind_addr={:?}", bind_addr);

        self.set_last_public_addr(Some(
            self.options
                .static_ip
                .clone()
                .expect("start_with_address when no static address"),
        ));

        // Always bind UDP to 0.0.0.0:<port> so that we listen on all interfaces, even when a static external IP is configured.
        // The static address is used for signaling and routing outside any firewall, not for local bind.
        let port = bind_addr.port();
        let bind_all = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
        tracing::info!(
            "[Engine] start_with_addr: requested={:?} binding={:?}",
            bind_addr,
            bind_all
        );

        let mut mux0 =
            UdpNetworkMux::bind(bind_all).map_err(|e| BingleError::Other(format!("Failed to bind UDP mux: {}", e)))?;
        // Determine the concrete local address after bind (handles port 0)
        let _local_addr: SocketAddr = mux0
            .local_addr()
            .map_err(|e| BingleError::Other(format!("Failed to get local addr: {}", e)))?;

        // Install the common DTLS handler wrapper
        self.install_dtls_handler()?;

        // Configure TURN ChannelData handler based on role (relay vs client)
        tracing::info!("[Engine] set_handle_turn from start_with_addr");
        let th = self.create_turn_handler();
        mux0.set_handle_turn(Some(&th));

        // Now wrap mux in Arc
        let mut mux0 = mux0;
        mux0.span = self.span.clone();
        let mux = Arc::new(mux0);

        // Start the UDP mux background loop first
        mux.start().map_err(|e| BingleError::Other(format!("Failed to start UDP mux: {}", e)))?;

        // Start DTLS accept loop with the mux
        self.dtls.start(mux.clone())
            .map_err(|e| BingleError::Other(format!("Failed to start DTLS: {}", e)))?;

        // If we are configured as a relay, pre-populate the in-memory DDB with known root relays.
        if self.options.am_relay {
            self.initialize_relay_async();
        }
        // Static address path: once DTLS accept loop is running and any relay is available, notify that we are listening.
        self.notify_listening(true);

        self.mux = Some(mux);
        tracing::info!("[Engine] start_with_addr: done");
        self.set_state_internal(EngineState::Registered);

        Ok(())
    }

    fn on_stun_consistent(&mut self, public_addr: Option<SocketAddr>) {
        // Spawn a worker thread to process STUN-consistent follow-up to avoid blocking inbound packet path
        let self_ptr = std::sync::atomic::AtomicPtr::new(self as *mut Engine);
        let span = self.span.clone();
        std::thread::spawn(move || unsafe {
            let _guard = span.enter();
            let eng = &mut *self_ptr.load(std::sync::atomic::Ordering::SeqCst);
            eng.stun_consistent_process(public_addr);
        });
    }

    fn stun_consistent_process(&mut self, public_addr: Option<SocketAddr>) {
        tracing::info!("[Engine] on_stun_consistent: public_addr={:?}", public_addr);
        // Save last known public address (for validation/tests)
        self.set_last_public_addr(public_addr);

        // Transition to TrianglePing and perform relay triangle test
        let prev = self.state;
        self.state = EngineState::TrianglePing;
        tracing::info!("[Engine] state change: {:?} -> TrianglePing", prev);
        #[allow(unused)]
        {}

        // Do NOT mark EndpointAvailable here; proceed with the triangle process and only
        // transition to EndpointAvailable once TriangleTest3 is observed.

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
                // Capture app_id and provider config from options
                let opt_app_id = self.options.app_id;
                let opt_cfg = self.options.algo_provider_config.clone();
                let app_id_opt = opt_app_id.or_else(|| {
                    std::env::var("BINGLE_APP_ID")
                        .ok()
                        .and_then(|s| s.parse::<u64>().ok())
                });
                if let Some(app_id) = app_id_opt {
                    crate::relay::discovery::indexer_discover_closure(app_id, opt_cfg)
                } else {
                    // No app id set
                    panic!("[Engine] indexer discovery has no app id");
                }
            };

            let finder = crate::relay::relay_finder::RelayFinder::new(
                api.clone(),
                Duration::from_secs(60),
                discover,
            );
            // Use our id (Algorand address) for relay selection, not the user-visible handle.
            // Prefer the issuer set earlier by BingleApiImpl::start (issuer = id + ISSUER_SUFFIX).
            let my_id: String = if let Some(iss) = self.issuer.as_deref() {
                iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX)
                    .to_string()
            } else {
                // Fallback: if issuer is not set, use the handle (best-effort; may yield suboptimal selection).
                self.options.handle.clone()
            };
            // If configured as a relay, update the in-memory DDB with all root relays discovered
            if self.options.am_relay {
                let roots = finder.list_root_relays(&my_id, true);
                tracing::info!(
                    "[Engine::stun_consistent_process] discovered {} root relays (excluding self)",
                    roots.len()
                );
                self.upsert_roots_into_backend(&roots);
                tracing::info!("[Engine::stun_consistent_process] upserted root relays into backend");
            }

            let relay = finder.find_relay(&my_id);
            if let Ok(r) = relay {
                relay_target = Some(r.clone());
                tracing::info!("[Engine] chosen relay {} (id={})", r.address, r.id);
            } else {
                panic!("[Engine] no relay found");
            }
            self.relay_finder = Some(Arc::new(finder));
        }

        // Send TriangleTest1 to the discovered relay using the Bingle API callback if installed
        if let Some(target) = relay_target {
            let to_addr = target.address;
            let checking_ep = public_addr.unwrap_or(to_addr);
            let seen = self.seen_endpoints.lock().unwrap().iter().cloned().collect();
            let msg = Message::Relay(RelayMessage::TriangleTest1(RelayTriangleTest1 {
                app: None,
                checking_endpoint: checking_ep.into(),
                do_not_use_endpoints: seen,
            }));
            let nsk = NetworkEndpoint::new_direct(to_addr);
            // Build JSON value for the message
            let json_val = crate::messages::marshal::to_json_value(&msg);
            if let Some(cb) = &self.send_via_bingle {
                // Use the relay's actual Algorand address (base32) as the user id.
                let uid = target.id.clone();
                let ok = cb(&nsk, &uid, json_val);
                tracing::info!(
                    "[Engine] TriangleTest1 send_via_bingle to {} (uid=base32 relay id) -> {}",
                    to_addr,
                    ok
                );
                #[allow(unused)]
                {}
            } else {
                tracing::info!(
                    "[Engine][WARN] send_via_bingle not installed; cannot send TriangleTest1 to {}",
                    to_addr
                );
                #[allow(unused)]
                {}
            }
        } else {
            tracing::info!(
                "[Engine][WARN] TrianglePing path active but no destination to send TriangleTest1"
            );
            panic!(
                "[Engine][WARN] TrianglePing path active but no destination to send TriangleTest1"
            );
        }
    }

    pub(crate) fn on_stun_inconsistent(&mut self) {
        self.set_nat_type(NatType::Symmetric);
        self.set_state_internal(EngineState::NATRestricted);
        self.notify_listening(true);
    }

    pub(crate) fn on_stun_blocked(&mut self) {
        self.set_nat_type(NatType::NoConnection);
    }

    /// Configure STUN send/state handlers and start the finder after DTLS and mux are running.
    fn start_stun_find(
        &mut self,
        options: &StartOptions,
        finder: &Arc<Mutex<Box<dyn StunEndpointFinder + Send + Sync>>>,
        mux: &Arc<UdpNetworkMux>,
    ) -> Result<(), BingleError> {
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
                        tracing::warn!(
                            "[Engine::start] STUN send_packet_handler: invalid host '{}': {}",
                            host,
                            e
                        );
                    }
                }
            })));

            // Wire STUN state changes into Engine handlers.
            f.set_state_change_handler(Some(Arc::new(move |st, ep| {
                let p = self_ptr.load(std::sync::atomic::Ordering::SeqCst);
                if p.is_null() {
                    return;
                }
                unsafe {
                    if st == crate::stun::endpoint_finder::StunState::Consistent {
                        (&mut *p).on_stun_consistent(ep);
                    } else if st == crate::stun::endpoint_finder::StunState::Inconsistent {
                        (&mut *p).on_stun_inconsistent();
                    } else if st == crate::stun::endpoint_finder::StunState::Blocked {
                        (&mut *p).on_stun_blocked();
                    }
                }
            })));

            // Kick off STUN polling using provided servers
            let servers = options.stun_servers.clone().unwrap_or_default();
            if servers.is_empty() {
                return Err(BingleError::Other("No STUN servers provided".to_string()));
            }
            f.start(servers, 2_000, 60_000);
        }
        Ok(())
    }

    /// Stop the engine and background tasks if started.
    pub fn stop(&mut self) {
        let last_addr = self.last_public_addr();
        tracing::info!("[Engine::stop] starting {:?}:{:?}", self.issuer, last_addr);
        // First, clear any API pointers and global router callbacks to avoid dangling references across tests
        self.clear_api_bindings();
        self.dtls.stop().expect(&format!(
            "DTLS stop failed in Engine::stop {}:{}",
            self.issuer.as_ref().map(|s| s.as_str()).unwrap_or("None"),
            last_addr
                .map(|addr| addr.to_string())
                .unwrap_or_else(|| "None".to_string())
        ));

        if let Some(mux) = &self.mux {
            mux.stop();
        }
        else {
            tracing::warn!("[Engine::stop] mux is not running at stop time {:?}:{:?}", self.issuer, last_addr);
        }
        if let Some(stun_arc) = &self.stun {
            tracing::info!("[Engine::stop] locking STUN finder");
            if let Ok(mut finder) = stun_arc.lock() {
                tracing::info!("[Engine::stop] stopping STUN finder");
                finder.stop();
            }
            else {
                tracing::error!("[Engine::stop] STUN finder lock failed {:?}:{:?}", self.issuer, last_addr);
            }
        }
        else {
            tracing::warn!("[Engine::stop] STUN finder is not running at stop time {:?}:{:?}", self.issuer, last_addr);
        }
        self.mux = None;
        self.stun = None;
        tracing::info!("[Engine::stop] done {:?}:{:?}", self.issuer, last_addr);
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
    pub fn last_public_addr(&self) -> Option<SocketAddr> {
        self.last_public_addr_shared.lock().ok().and_then(|g| *g)
    }

    pub fn last_public_addr_shared_for_tests(&self) -> Option<SocketAddr> {
        self.last_public_addr_shared.lock().ok().and_then(|g| *g)
    }

    pub fn peer_ddb_records(&self) -> Option<usize> {
        self.peer_ddb_records
    }

    pub fn ddb_upsert_record(&self, record: AdvertRecord) {
        if let Ok(mut b) = self.ddb_backend.lock() {
            b.upsert(record);
        }
    }

    pub fn ddb_backend_size(&self) -> usize {
        if let Ok(b) = self.ddb_backend.lock() {
            b.len()
        } else {
            0
        }
    }

    pub fn test_force_stun_consistent(&mut self, addr: SocketAddr) {
        self.on_stun_consistent(Some(addr));
    }

    pub fn test_force_stun_inconsistent(&mut self) {
        self.on_stun_inconsistent();
    }

    pub fn test_force_stun_blocked(&mut self) {
        self.on_stun_blocked();
    }

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
    pub fn turn_client_handler_for_tests(
        &self,
    ) -> std::sync::Arc<crate::turn::turn_client_handler_impl::TurnClientHandlerImpl> {
        self.turn_handler_client.clone().expect("In turn_client_handler_for_tests must have turn_handler_client")
    }
    pub fn turn_relay_handler_for_tests(
        &self,
    ) -> std::sync::Arc<crate::turn::turn_relay_handler_impl::TurnRelayHandlerImpl> {
        self.turn_handler_relay.clone().expect("In turn_relay_handler_for_tests must have turn_handler_relay")
    }
}

impl Engine {
    /// Relay-side: register a listener relay id -> address mapping (non-test API)
    pub fn turn_relay_handle_listen(&self, relay_id: &str, relay_addr: &SocketAddr) -> bool {
        self.turn_handler_relay.clone().expect("In turn_relay_handle_listen must have turn_handler_relay").handle_listen(relay_id, relay_addr)
    }

    /// Relay-side: lookup address by id (non-test API)
    pub fn turn_relay_lookup_addr_by_id(&self, relay_id: &str) -> Option<SocketAddr> {
        self.turn_handler_relay.clone().expect("In turn_relay_lookup_addr_by_id must have turn_handler_relay").lookup_addr_by_id(relay_id)
    }

    /// Relay-side: handle a Call by allocating channel (non-test API)
    pub fn turn_relay_handle_call(&self, source_id: &str, dest_id: &str, source: SocketAddr, dest: SocketAddr) -> i32 {
        crate::turn::turn_handler::TurnRelayHandler::handle_call(
            &*self.turn_handler_relay.clone().expect("In turn_relay_handle_call must have turn_handler_relay"),
            source_id,
            dest_id,
            &source,
            &dest,
        )
    }

    /// Client-side: record ListenResponse mapping (non-test API)
    pub fn turn_client_handle_listen_response(&self, relay_addr: SocketAddr, relay_id: &str) {
        crate::turn::turn_handler::TurnClientHandler::handle_listen_response(
            &*self.turn_handler_client.clone().expect("In turn_client_handle_listen_response must have turn_handler_client"),
            &relay_addr,
            relay_id,
        );
    }

    /// Record CallResponse mapping (non-test API)
    pub fn turn_handle_call_response(
        &self,
        source: SocketAddr,
        dest: SocketAddr,
        channel: u16,
        relay_id: &str,
    ) {
        use crate::turn::turn_handler::TurnHandler;
        if let Some(h) = &self.turn_handler_client {
            h.handle_call_response(&source, &dest, channel, relay_id);
        }
        if let Some(h) = &self.turn_handler_relay {
            h.handle_call_response(&source, &dest, channel, relay_id);
        }
    }

    /// Client-side: record Called mapping (non-test API)
    pub fn turn_client_handle_called(&self, source: SocketAddr, dest: SocketAddr, channel: u16) {
        crate::turn::turn_handler::TurnClientHandler::handle_called(
            &*self.turn_handler_client.clone().expect("In turn_client_handle_called must have turn_handler_client"),
            &source,
            &dest,
            channel,
        );
    }

    // Mutex message handlers - delegate to the distributed mutex instance if it exists
    pub fn mutex_handle_request(&self, from_id: &str, req: &crate::messages::types::MutexRequest) {
        if let Some(m) = &self.relay_init_mutex {
            m.handle_request(from_id, req);
        }
    }
    pub fn mutex_handle_response(
        &self,
        from_id: &str,
        resp: &crate::messages::types::MutexResponse,
    ) {
        if let Some(m) = &self.relay_init_mutex {
            m.handle_reply(from_id, resp);
        }
    }
    pub fn mutex_handle_release(&self, from_id: &str, rel: &crate::messages::types::MutexRelease) {
        if let Some(m) = &self.relay_init_mutex {
            m.handle_release(from_id, rel);
        }
    }
}
