use crate::messages::handlers::{MessageHandler, FromStruct};
use crate::messages::types::*;

use std::cell::RefCell;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use crate::api::bingle_api::{BingleApi, BingleApiInternal, NetworkEndpoint, UserId};

// Thread-local current router used to avoid globals and isolate per-API state.
thread_local! {
    static CURRENT_ROUTER: RefCell<Option<Arc<Router>>> = RefCell::new(None);
}

// No global fallbacks. Each API instance owns its own Router.
// A thread-local CURRENT_ROUTER is used to select the active Router context during routing and callbacks.

#[derive(Default)]
pub struct Router {
    sender: Mutex<Option<Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>>>,
    api: Mutex<Option<Arc<dyn BingleApi>>>,
    api_internal: Mutex<Option<Arc<dyn BingleApiInternal>>>,
    last_from: Mutex<Option<SocketAddr>>,
    last_response_tag: Mutex<Option<String>>,
    on_message: Mutex<Option<Arc<crate::api::bingle_api::OnMessageHandler>>>,
    // DDB/relay context
    am_relay: std::sync::atomic::AtomicBool,
    ddb_backend: Mutex<Option<std::sync::Arc<std::sync::Mutex<crate::ddb::InMemoryDdbBackend>>>>,
    // Outbound response produced by handlers during routing (consumed by Engine/DTLS layer)
    outbound_response: Mutex<Option<serde_json::Value>>,
}

struct CombinedApi {
    api: Arc<dyn BingleApi>,
    internal: Option<Arc<dyn BingleApiInternal>>,
}

impl BingleApi for CombinedApi {
    fn debug_print_options(&self) { self.api.debug_print_options() }
    fn get_my_id(&self) -> Option<String> { self.api.get_my_id() }
    fn get_handle(&self) -> Option<String> { self.api.get_handle() }
    fn get_app_id(&self) -> Option<u64> { self.api.get_app_id() }
    fn get_algo_provider_config(&self) -> Option<crate::blockchain::algo_ops::AlgoChainConfig> { self.api.get_algo_provider_config() }
    fn start(&mut self, _options: &crate::api::bingle_api::StartOptions) -> Result<(), String> { Err("not supported in handler context".into()) }
    fn stop(&mut self) { }
    fn network_change(&mut self) { }
    fn send_message_to_id(&self, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { self.api.send_message_to_id(user_id, message, progress) }
    fn send_message_to_handle(&self, handle: &crate::api::bingle_api::Handle, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { self.api.send_message_to_handle(handle, message, progress) }
    fn send_message_to_network(&self, nsk: &NetworkEndpoint, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { self.api.send_message_to_network(nsk, user_id, message, progress) }
    fn send_message_to_id_with_response(&self, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { self.api.send_message_to_id_with_response(user_id, message, progress) }
    fn send_message_to_handle_with_response(&self, handle: &crate::api::bingle_api::Handle, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { self.api.send_message_to_handle_with_response(handle, message, progress) }
    fn send_message_to_network_with_response(&self, nsk: &NetworkEndpoint, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { self.api.send_message_to_network_with_response(nsk, user_id, message, progress) }
    fn set_on_message(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnMessageHandler>>) { }
    fn set_on_connect(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnConnectHandler>>) { }
}
impl BingleApiInternal for CombinedApi {
    fn mutex_handle_request(&self, from_id: String, req: crate::messages::types::MutexRequest) {
        if let Some(i) = &self.internal { i.mutex_handle_request(from_id, req); }
    }
    fn mutex_handle_response(&self, from_id: String, resp: crate::messages::types::MutexResponse) {
        if let Some(i) = &self.internal { i.mutex_handle_response(from_id, resp); }
    }
    fn mutex_handle_release(&self, from_id: String, rel: crate::messages::types::MutexRelease) {
        if let Some(i) = &self.internal { i.mutex_handle_release(from_id, rel); }
    }
    fn get_relay_state(&self) -> String { if let Some(i) = &self.internal { i.get_relay_state() } else { "off".to_string() } }
    fn set_state(&self, state: crate::engine::EngineState) {
        if let Some(i) = &self.internal { i.set_state(state); }
    }
    fn get_state(&self) -> crate::engine::EngineState {
        if let Some(i) = &self.internal { i.get_state() } else { crate::engine::EngineState::StunIdentify }
    }
    fn set_nat_type(&self, nat: crate::engine::NatType) {
        if let Some(i) = &self.internal { i.set_nat_type(nat); }
    }
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> {
        if let Some(i) = &self.internal { i.get_last_public_addr() } else { None }
    }
    fn ddb_register_ip(&self, endpoint: std::net::SocketAddr) -> Result<(), String> {
        if let Some(i) = &self.internal { i.ddb_register_ip(endpoint) } else { Err("not implemented".into()) }
    }
    fn ddb_register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), String> {
        if let Some(i) = &self.internal { i.ddb_register_relay(relay_id, relay_sig) } else { Err("not implemented".into()) }
    }
    fn update_turn_listener_relay(&self, relay_id: String, relay_addr: std::net::SocketAddr) -> Result<(), String> {
        if let Some(i) = &self.internal { i.update_turn_listener_relay(relay_id, relay_addr) } else { Err("not implemented".into()) }
    }
    fn turn_client_handle_listen_response(&self, relay_addr: std::net::SocketAddr, relay_id: String) {
        if let Some(i) = &self.internal { i.turn_client_handle_listen_response(relay_addr, relay_id) }
    }
    fn turn_lookup_addr_by_id(&self, id: String) -> Option<std::net::SocketAddr> {
        if let Some(i) = &self.internal { i.turn_lookup_addr_by_id(id) } else { None }
    }
    fn turn_handle_call(&self, source: std::net::SocketAddr, dest: std::net::SocketAddr) -> i32 {
        if let Some(i) = &self.internal { i.turn_handle_call(source, dest) } else { -1 }
    }
    fn turn_handle_listen(&self, id: String, source: std::net::SocketAddr) -> bool {
        if let Some(i) = &self.internal { i.turn_handle_listen(id, source) } else { false }
    }
    fn turn_handle_called(&self, source: std::net::SocketAddr, dest: std::net::SocketAddr, channel: u16) {
        if let Some(i) = &self.internal { i.turn_handle_called(source, dest, channel) }
    }
    fn notify_listening(&self, listening: bool) {
        if let Some(i) = &self.internal { i.notify_listening(listening) }
    }
}

impl Router {
    pub fn new(api: Arc<dyn BingleApi>) -> Self {
        Self {
            sender: Mutex::new(None),
            api: Mutex::new(Some(api)),
            api_internal: Mutex::new(None),
            last_from: Mutex::new(None),
            last_response_tag: Mutex::new(None),
            on_message: Mutex::new(None),
            am_relay: std::sync::atomic::AtomicBool::new(false),
            ddb_backend: Mutex::new(None),
            outbound_response: Mutex::new(None),
        }
    }

    pub fn with_current_router<R>(router: Arc<Router>, f: impl FnOnce() -> R) -> R {
        // Temporarily set the thread-local router for duration of f
        let prev = CURRENT_ROUTER.with(|cell| cell.replace(Some(router)));
        let out = f();
        // Restore previous value
        CURRENT_ROUTER.with(|cell| {
            let _ = cell.replace(prev);
        });
        out
    }

    pub fn set_sender(&self, cb: Option<Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>>) {
        if let Ok(mut g) = self.sender.lock() { *g = cb; }
    }
    pub fn get_sender(&self) -> Option<Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>> {
        match self.sender.lock() { Ok(g) => g.clone(), Err(_) => None }
    }

    pub fn set_bingle_api(&self, api: Option<Arc<dyn BingleApi>>) { if let Ok(mut g) = self.api.lock() { *g = api; } }
    pub fn get_bingle_api(&self) -> Option<Arc<dyn BingleApi>> { match self.api.lock() { Ok(g) => g.clone(), Err(_) => None } }

    pub fn set_bingle_api_internal(&self, api: Option<Arc<dyn BingleApiInternal>>) { if let Ok(mut g) = self.api_internal.lock() { *g = api; } }
    pub fn get_bingle_api_internal(&self) -> Option<Arc<dyn BingleApiInternal>> { match self.api_internal.lock() { Ok(g) => g.clone(), Err(_) => None } }

    pub fn set_last_from(&self, addr: Option<SocketAddr>) { if let Ok(mut g) = self.last_from.lock() { *g = addr; } }
    pub fn get_last_from(&self) -> Option<SocketAddr> { match self.last_from.lock() { Ok(g) => *g, Err(_) => None } }

    pub fn set_last_response_tag(&self, tag: Option<String>) { if let Ok(mut g) = self.last_response_tag.lock() { *g = tag; } }
    pub fn get_last_response_tag(&self) -> Option<String> { match self.last_response_tag.lock() { Ok(g) => g.clone(), Err(_) => None } }

    pub fn set_on_message(&self, cb: Option<Arc<crate::api::bingle_api::OnMessageHandler>>) { if let Ok(mut g) = self.on_message.lock() { *g = cb; } }
    pub fn get_on_message(&self) -> Option<Arc<crate::api::bingle_api::OnMessageHandler>> { match self.on_message.lock() { Ok(g) => g.clone(), Err(_) => None } }

    // DDB/relay context setters/getters
    pub fn set_am_relay(&self, b: bool) { self.am_relay.store(b, std::sync::atomic::Ordering::SeqCst); }
    pub fn get_am_relay(&self) -> bool { self.am_relay.load(std::sync::atomic::Ordering::SeqCst) }
    pub fn set_ddb_backend(&self, backend: Option<std::sync::Arc<std::sync::Mutex<crate::ddb::InMemoryDdbBackend>>>) { if let Ok(mut g) = self.ddb_backend.lock() { *g = backend; } }
    pub fn get_ddb_backend(&self) -> Option<std::sync::Arc<std::sync::Mutex<crate::ddb::InMemoryDdbBackend>>> { match self.ddb_backend.lock() { Ok(g) => g.clone(), Err(_) => None } }


    // Outbound response helpers
    pub fn set_outbound_response(&self, resp: Option<serde_json::Value>) { if let Ok(mut g) = self.outbound_response.lock() { *g = resp; } }
    pub fn take_outbound_response(&self) -> Option<serde_json::Value> { match self.outbound_response.lock() { Ok(mut g) => g.take(), Err(_) => None } }

    /// Route a message providing the source network endpoint explicitly.
    pub fn route_with_network<H: MessageHandler + ?Sized>(&self, handler: &H, msg: &Message, from_id: &str, from_ep: &NetworkEndpoint) {
        let api_opt = self.get_bingle_api();
        if api_opt.is_none() {
            log::warn!("[router::route_with_network] No BingleApi available to pass to handler");
            return;
        }
        let api_base = api_opt.unwrap();
        let internal = self.get_bingle_api_internal();
        let combined_api: Arc<dyn crate::api::bingle_api::BingleApiBoth> = Arc::new(CombinedApi { api: api_base.clone(), internal });
        let api = combined_api.clone();
        // Build FromStruct with id and provided network endpoint
        let from = FromStruct { id: from_id.to_string(), network_source_key: from_ep.clone() };
        match msg {
            Message::PlainText(pt) => handler.on_plain_text(api.clone(), &from, pt),
            Message::Relay(r) => match r {
                RelayMessage::Call(m) => handler.on_relay_call(api.clone(), &from, m),
                RelayMessage::RelayResponse(m) => handler.on_relay_response(api.clone(), &from, m),
                RelayMessage::TriangleTest1(m) => handler.on_triangle_test1(api.clone(), &from, m),
                RelayMessage::TriangleTest2(m) => handler.on_triangle_test2(api.clone(), &from, m),
                RelayMessage::TriangleTest3(m) => handler.on_triangle_test3(api.clone(), &from, m),
                RelayMessage::TriangleTest1Response(m) => handler.on_triangle_test1_response(api.clone(), &from, m),
                RelayMessage::Listen(m) => handler.on_relay_listen(api.clone(), &from, m),
                RelayMessage::Check(m) => handler.on_relay_check(api.clone(), &from, m),
                RelayMessage::ListenResponse(m) => handler.on_relay_listen_response(api.clone(), &from, m),
                RelayMessage::CheckResponse(m) => handler.on_relay_check_response(api.clone(), &from, m),
                RelayMessage::CallResponse(m) => handler.on_relay_call_response(api.clone(), &from, m),
                RelayMessage::KeepAlive(m) => handler.on_relay_keep_alive(api.clone(), &from, m),
                RelayMessage::RelayCalled(m) => handler.on_relay_called(api.clone(), &from, m),
            },
            Message::Ddb(d) => match d {
                DdbMessage::UpsertResolve(m) => handler.on_ddb_upsert_resolve(api.clone(), &from, m),
                DdbMessage::QueryResolve(m) => handler.on_ddb_query_resolve(api.clone(), &from, m),
                DdbMessage::InitResolve(m) => handler.on_ddb_init_resolve(api.clone(), &from, m),
                DdbMessage::GetEpoch(m) => handler.on_ddb_get_epoch(api.clone(), &from, m),
                _ => handler.on_unimplemented(msg),
            },
            Message::Ping(p) => match p {
                PingMessage::Ping(m) => handler.on_ping_ping(api.clone(), &from, m),
                PingMessage::Response(m) => handler.on_ping_response(api.clone(), &from, m),
            },
            Message::Mutex(m) => {
                            // Forward Mutex messages into Engine via BingleApiInternal if available
                            if let Some(internal) = self.get_bingle_api_internal() {
                                match m {
                                    MutexMessage::Request(req) => internal.mutex_handle_request(from.id.clone(), req.clone()),
                                    MutexMessage::Response(resp) => internal.mutex_handle_response(from.id.clone(), resp.clone()),
                                    MutexMessage::Release(rel) => internal.mutex_handle_release(from.id.clone(), rel.clone()),
                                }
                            } else {
                                handler.on_unimplemented(msg)
                            }
                        },
            Message::Unknown(v) => handler.on_unknown(api.clone(), v),
        }
        // Handlers may set an outbound_response to be sent by DTLS layer; no-op here.
    }

    /// Backwards-compatible route that derives the endpoint from last_from.
    pub fn route<H: MessageHandler + ?Sized>(&self, handler: &H, msg: &Message, from_id: &str) {
        // Build FromStruct using last_from as a direct endpoint for legacy callers
        let nsk = if let Some(addr) = self.get_last_from() {
            NetworkEndpoint::new_direct(addr)
        } else {
            NetworkEndpoint::new_direct("0.0.0.0:0".parse().unwrap())
        };
        self.route_with_network(handler, msg, from_id, &nsk);
    }

    /// Test helper to clear all stored state (used by Engine::clear_api_bindings)
    pub fn clear_for_tests(&self) {
        self.set_sender(None);
        self.set_bingle_api(None);
        self.set_bingle_api_internal(None);
        self.set_last_from(None);
        self.set_last_response_tag(None);
        self.set_on_message(None);
        self.set_am_relay(false);
        self.set_ddb_backend(None);
    }
}

// Access the current thread-local Router if set.
impl Router {
    pub fn current() -> Option<Arc<Router>> {
        CURRENT_ROUTER.with(|cell| cell.borrow().clone())
    }
}
