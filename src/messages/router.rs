use crate::messages::handlers::{MessageHandler, FromStruct};
use crate::messages::types::*;

use std::cell::RefCell;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use crate::api::bingle_api::{BingleApi, BingleApiInternal, BingleApiBoth, NetworkEndpoint, UserId, Handle};
use crate::engine::BingleAccess;

// Thread-local current router used to avoid globals and isolate per-API state.
thread_local! {
    static CURRENT_ROUTER: RefCell<Option<Arc<Router>>> = RefCell::new(None);
}

#[derive(Default)]
pub struct Router {
    sender: Mutex<Option<Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>>>,
    api: Mutex<Option<crate::api::bingle_api::BingleApiBothType>>,
    last_from: Mutex<Option<SocketAddr>>,
    last_response_tag: Mutex<Option<String>>,
    on_message: Mutex<Option<Arc<crate::api::bingle_api::OnMessageHandler>>>,
    // DDB/relay context
    am_relay: std::sync::atomic::AtomicBool,
    ddb_backend: Mutex<Option<std::sync::Arc<std::sync::Mutex<crate::ddb::InMemoryDdbBackend>>>>,
    // Outbound response produced by handlers during routing (consumed by Engine/DTLS layer)
    outbound_response: Mutex<Option<serde_json::Value>>,
}

struct LockingApiWrapper {
    api: crate::api::bingle_api::BingleApiBothType,
}

impl BingleApi for LockingApiWrapper {
    fn debug_print_options(&self) { self.api.access(|a| a.debug_print_options()) }
    fn list_all_relays(&self, include_self: bool) -> Vec<crate::relay::relay_finder::RelayInfo> { self.api.access(|a| a.list_all_relays(include_self)) }
    fn get_my_id(&self) -> Option<String> { self.api.access(|a| a.get_my_id()) }
    fn get_user_id(&self) -> Option<String> { self.api.access(|a| a.get_user_id()) }
    fn get_handle(&self) -> Option<String> { self.api.access(|a| a.get_handle()) }
    fn get_app_id(&self) -> Option<u64> { self.api.access(|a| a.get_app_id()) }
    fn get_algo_provider_config(&self) -> Option<crate::blockchain::algo_ops::AlgoChainConfig> { self.api.access(|a| a.get_algo_provider_config()) }
    fn start(&mut self, _options: &crate::api::bingle_api::StartOptions) -> Result<(), String> { Err("not supported in handler context".into()) }
    fn stop(&mut self) { }
    fn network_change(&mut self) { }
    fn handle_lookup(&self, handle: &Handle) -> Result<Option<UserId>, String> { self.api.access(|a| a.handle_lookup(handle)) }
    fn send_message_to_id(&self, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { self.api.access(|a| a.send_message_to_id(user_id, message, progress)) }
    fn send_message_to_handle(&self, handle: &crate::api::bingle_api::Handle, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { self.api.access(|a| a.send_message_to_handle(handle, message, progress)) }
    fn send_message_to_network(&self, nsk: &NetworkEndpoint, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> bool { self.api.access(|a| a.send_message_to_network(nsk, user_id, message, progress)) }
    fn send_message_to_id_with_response(&self, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { self.api.access(|a| a.send_message_to_id_with_response(user_id, message, progress)) }
    fn send_message_to_handle_with_response(&self, handle: &Handle, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { self.api.access(|a| a.send_message_to_handle_with_response(handle, message, progress)) }
    fn send_message_to_network_with_response(&self, nsk: &NetworkEndpoint, user_id: &UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { self.api.access(|a| a.send_message_to_network_with_response(nsk, user_id, message, progress)) }
    fn set_on_message(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnMessageHandler>>) { }
    fn set_on_connect(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnConnectHandler>>) { }
    fn set_on_listening(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnListeningHandler>>) { }
}

impl BingleApiInternal for LockingApiWrapper {
    fn mutex_handle_request(&self, from_id: String, req: crate::messages::types::MutexRequest) { self.api.access(|a| a.mutex_handle_request(from_id, req)) }
    fn mutex_handle_response(&self, from_id: String, resp: crate::messages::types::MutexResponse) { self.api.access(|a| a.mutex_handle_response(from_id, resp)) }
    fn mutex_handle_release(&self, from_id: String, rel: crate::messages::types::MutexRelease) { self.api.access(|a| a.mutex_handle_release(from_id, rel)) }
    fn get_relay_state(&self) -> String { self.api.access(|a| a.get_relay_state()) }
    fn set_state(&self, state: crate::engine::EngineState) { self.api.access(|a| a.set_state(state)) }
    fn get_state(&self) -> crate::engine::EngineState { self.api.access(|a| a.get_state()) }
    fn set_nat_type(&self, nat: crate::engine::NatType) { self.api.access(|a| a.set_nat_type(nat)) }
    fn get_last_public_addr(&self) -> Option<SocketAddr> { self.api.access(|a| a.get_last_public_addr()) }
    fn ddb_register_ip(&self, endpoint: SocketAddr, am_relay: bool) -> Result<(), String> { self.api.access(|a| a.ddb_register_ip(endpoint, am_relay)) }
    fn ddb_register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), String> { self.api.access(|a| a.ddb_register_relay(relay_id, relay_sig)) }
    fn update_turn_listener_relay(&self, relay_id: String, relay_addr: SocketAddr) -> Result<(), String> { self.api.access(|a| a.update_turn_listener_relay(relay_id, relay_addr)) }
    fn turn_client_handle_listen_response(&self, relay_addr: SocketAddr, relay_id: String) { self.api.access(|a| a.turn_client_handle_listen_response(relay_addr, relay_id)) }
    fn turn_lookup_addr_by_id(&self, id: String) -> Option<SocketAddr> { self.api.access(|a| a.turn_lookup_addr_by_id(id)) }
    fn turn_handle_call(&self, source: SocketAddr, dest: SocketAddr) -> i32 { self.api.access(|a| a.turn_handle_call(source, dest)) }
    fn turn_handle_listen(&self, id: String, source: SocketAddr) -> bool { self.api.access(|a| a.turn_handle_listen(id, source)) }
    fn turn_handle_called(&self, source: SocketAddr, dest: SocketAddr, channel: u16) { self.api.access(|a| a.turn_handle_called(source, dest, channel)) }
    fn notify_listening(&self, listening: bool) { self.api.access(|a| a.notify_listening(listening)) }
    fn set_relay_state(&self, state: crate::engine::RelayState) { self.api.access(|a| a.set_relay_state(state)) }
    fn get_peer_ddb_target(&self) -> Option<usize> { self.api.access(|a| a.get_peer_ddb_target()) }
    fn ddb_upsert_record(&self, record: crate::ddb::AdvertRecord) { self.api.access(|a| a.ddb_upsert_record(record)) }
    fn ddb_backend_size(&self) -> usize { self.api.access(|a| a.ddb_backend_size()) }
    fn initialize_relay(&self) { self.api.access(|a| a.initialize_relay()) }
    fn is_relay(&self) -> bool { self.api.access(|a| a.is_relay()) }
    fn signal_signon_complete(&self) { self.api.access(|a| a.signal_signon_complete()) }
    fn reset_signon_complete(&self) { self.api.access(|a| a.reset_signon_complete()) }
    fn ripple_message(&self, message: serde_json::Value, originator_id: String) { self.api.access(|a| a.ripple_message(message, originator_id)) }
}


impl Router {
    pub fn new(api: crate::api::bingle_api::BingleApiBothType) -> Self {
        Self {
            sender: Mutex::new(None),
            api: Mutex::new(Some(api)),
            last_from: Mutex::new(None),
            last_response_tag: Mutex::new(None),
            on_message: Mutex::new(None),
            am_relay: std::sync::atomic::AtomicBool::new(false),
            ddb_backend: Mutex::new(None),
            outbound_response: Mutex::new(None),
        }
    }

    pub fn with_current_router<R>(router: Arc<Router>, f: impl FnOnce() -> R) -> R {
        let prev = CURRENT_ROUTER.with(|cell| cell.replace(Some(router)));
        let out = f();
        CURRENT_ROUTER.with(|cell| { let _ = cell.replace(prev); });
        out
    }

    pub fn set_sender(&self, cb: Option<Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>>) {
        if let Ok(mut g) = self.sender.lock() { *g = cb; }
    }
    pub fn get_sender(&self) -> Option<Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>> {
        match self.sender.lock() { Ok(g) => g.clone(), Err(_) => None }
    }

    pub fn set_bingle_api(&self, api: Option<crate::api::bingle_api::BingleApiBothType>) { if let Ok(mut g) = self.api.lock() { *g = api; } }
    pub fn get_bingle_api(&self) -> Option<Arc<dyn crate::api::bingle_api::BingleApiBoth>> {
        match self.api.lock() {
            Ok(g) => g.as_ref().and_then(|w| w.upgrade()),
            Err(_) => None,
        }
    }

    pub fn set_last_from(&self, addr: Option<SocketAddr>) { if let Ok(mut g) = self.last_from.lock() { *g = addr; } }
    pub fn get_last_from(&self) -> Option<SocketAddr> { match self.last_from.lock() { Ok(g) => *g, Err(_) => None } }

    pub fn set_last_response_tag(&self, tag: Option<String>) { if let Ok(mut g) = self.last_response_tag.lock() { *g = tag; } }
    pub fn get_last_response_tag(&self) -> Option<String> { match self.last_response_tag.lock() { Ok(g) => g.clone(), Err(_) => None } }

    pub fn set_on_message(&self, cb: Option<Arc<crate::api::bingle_api::OnMessageHandler>>) { if let Ok(mut g) = self.on_message.lock() { *g = cb; } }
    pub fn get_on_message(&self) -> Option<Arc<crate::api::bingle_api::OnMessageHandler>> { match self.on_message.lock() { Ok(g) => g.clone(), Err(_) => None } }

    pub fn set_am_relay(&self, b: bool) { self.am_relay.store(b, std::sync::atomic::Ordering::SeqCst); }
    pub fn get_am_relay(&self) -> bool { self.am_relay.load(std::sync::atomic::Ordering::SeqCst) }
    pub fn set_ddb_backend(&self, backend: Option<std::sync::Arc<std::sync::Mutex<crate::ddb::InMemoryDdbBackend>>>) { if let Ok(mut g) = self.ddb_backend.lock() { *g = backend; } }
    pub fn get_ddb_backend(&self) -> Option<std::sync::Arc<std::sync::Mutex<crate::ddb::InMemoryDdbBackend>>> { match self.ddb_backend.lock() { Ok(g) => g.clone(), Err(_) => None } }

    pub fn set_outbound_response(&self, resp: Option<serde_json::Value>) { if let Ok(mut g) = self.outbound_response.lock() { *g = resp; } }
    pub fn take_outbound_response(&self) -> Option<serde_json::Value> { match self.outbound_response.lock() { Ok(mut g) => g.take(), Err(_) => None } }

    pub fn route_with_network<H: MessageHandler + ?Sized>(&self, handler: &H, msg: &Message, from_id: &str, from_ep: &NetworkEndpoint) {
        let api_opt = self.get_bingle_api();
        if api_opt.is_none() {
            log::warn!("[router::route_with_network] No BingleApi available to pass to handler");
            return;
        }
        let api_base = api_opt.unwrap();
        let api: Arc<dyn BingleApiBoth> = Arc::new(LockingApiWrapper {
            api: Arc::downgrade(&api_base),
        });
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
                DdbMessage::DumpResolve(m) => handler.on_ddb_dump_resolve(api.clone(), &from, m),
                DdbMessage::GetEpoch(m) => handler.on_ddb_get_epoch(api.clone(), &from, m),
                DdbMessage::EpochInfo(m) => handler.on_ddb_epoch_info(api.clone(), &from, m),
                DdbMessage::Signon(m) => handler.on_ddb_signon(api.clone(), &from, m),
                DdbMessage::SignonResponse(m) => handler.on_ddb_signon_response(api.clone(), &from, m),
                _ => handler.on_unimplemented(msg),
            },
            Message::Ping(p) => match p {
                PingMessage::Ping(m) => handler.on_ping_ping(api.clone(), &from, m),
                PingMessage::Response(m) => handler.on_ping_response(api.clone(), &from, m),
            },
            Message::Mutex(m) => {
                            match m {
                                MutexMessage::Request(req) => api.mutex_handle_request(from.id.clone(), req.clone()),
                                MutexMessage::Response(resp) => api.mutex_handle_response(from.id.clone(), resp.clone()),
                                MutexMessage::Release(rel) => api.mutex_handle_release(from.id.clone(), rel.clone()),
                            }
                        },
            Message::Unknown(v) => handler.on_unknown(api.clone(), v),
        }
    }

    pub fn route<H: MessageHandler + ?Sized>(&self, handler: &H, msg: &Message, from_id: &str) {
        let nsk = if let Some(addr) = self.get_last_from() {
            NetworkEndpoint::new_direct(addr)
        } else {
            NetworkEndpoint::new_direct("0.0.0.0:0".parse().unwrap())
        };
        self.route_with_network(handler, msg, from_id, &nsk);
    }

    pub fn clear_for_tests(&self) {
        self.set_sender(None);
        self.set_bingle_api(None);
        self.set_last_from(None);
        self.set_last_response_tag(None);
        self.set_on_message(None);
        self.set_am_relay(false);
        self.set_ddb_backend(None);
    }
}

impl Router {
    pub fn current() -> Option<Arc<Router>> {
        CURRENT_ROUTER.with(|cell| cell.borrow().clone())
    }
}
