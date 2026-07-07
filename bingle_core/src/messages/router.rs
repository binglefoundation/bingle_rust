use crate::messages::handlers::{FromStruct, MessageHandler};
use crate::messages::types::*;

use ed25519_dalek::SigningKey;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use crate::api::bingle_api::{
    BingleApi, BingleApiBoth, BingleApiInternal, BingleError, Handle, NetworkEndpoint, UserId,
};

/// Check if a message type may only originate from a relay.
pub fn only_from_relay(msg: &Message) -> bool {
    match msg {
        Message::Relay(rm) => match rm {
            RelayMessage::TriangleTest2(_)
            | RelayMessage::TriangleTest3(_)
            | RelayMessage::RelayCalled(_) => true,
            _ => false,
        },
        Message::Ddb(dm) => match dm {
            DdbMessage::Signon(_) => true,
            DdbMessage::UpsertResolve(m) => m.rippled,
            DdbMessage::DeleteResolve(m) => m.rippled,
            DdbMessage::Signoff(m) => m.rippled,
            DdbMessage::InitResolve(_) => true,
            DdbMessage::UpdateResponse(_)
            | DdbMessage::QueryResponse(_)
            | DdbMessage::SignonResponse(_)
            | DdbMessage::RelaysStatusResponse(_)
            | DdbMessage::InitResponse(_)
            | DdbMessage::DumpResolve(_) => true,
            _ => false,
        },
        Message::Mutex(_) => true,
        Message::ReportFail(rf) => match rf {
            ReportFailMessage::ReportFailedRipple(_)
            | ReportFailMessage::ReportFailedRippleResponse(_)
            | ReportFailMessage::ReportFailedComplete(_) => true,
            _ => false,
        },
        _ => false,
    }
}

#[derive(Default)]
pub struct Router {
    sender: Mutex<
        Option<
            Arc<
                dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool
                    + Send
                    + Sync
                    + 'static,
            >,
        >,
    >,
    api: Mutex<Option<crate::api::bingle_api::BingleApiBothType>>,
    last_from: Mutex<Option<SocketAddr>>,
    last_response_tag: Mutex<Option<String>>,
    on_message: Mutex<Option<Arc<crate::api::bingle_api::OnMessageHandler>>>,
    // DDB/relay context
    am_relay: std::sync::atomic::AtomicBool,
    ddb_backend: Mutex<Option<Arc<Mutex<crate::ddb::InMemoryDdbBackend>>>>,
}

struct LockingApiWrapper {
    api: crate::api::bingle_api::BingleApiBothType,
}

impl LockingApiWrapper {
    fn api(&self, method: &str) -> Option<Arc<dyn BingleApiBoth>> {
        self.api.upgrade().or_else(|| {
            tracing::error!("[LockingApiWrapper::{}] BingleApi dropped", method);
            None
        })
    }
}

impl BingleApi for LockingApiWrapper {
    fn debug_print_options(&self) {
        if let Some(a) = self.api("debug_print_options") {
            a.debug_print_options()
        }
    }
    fn list_all_relays(&self, include_self: bool) -> Vec<crate::relay::relay_finder::RelayInfo> {
        self.api("list_all_relays")
            .map(|a| a.list_all_relays(include_self))
            .unwrap_or_default()
    }
    fn get_my_id(&self) -> Option<String> {
        self.api("get_my_id").and_then(|a| a.get_my_id())
    }
    fn get_user_id(&self) -> Option<String> {
        self.api("get_user_id").and_then(|a| a.get_user_id())
    }
    fn get_handle(&self) -> Option<String> {
        self.api("get_handle").and_then(|a| a.get_handle())
    }
    fn get_app_id(&self) -> Option<u64> {
        self.api("get_app_id").and_then(|a| a.get_app_id())
    }
    fn get_algo_provider_config(&self) -> Option<crate::blockchain::algo_ops::AlgoChainConfig> {
        self.api("get_algo_provider_config")
            .and_then(|a| a.get_algo_provider_config())
    }
    fn get_accounts_cache(
        &self,
    ) -> Option<Arc<Mutex<crate::blockchain::algo_bingle::AccountsCache>>> {
        self.api("get_accounts_cache")
            .and_then(|a| a.get_accounts_cache())
    }
    fn clear_accounts_cache(&self) {
        if let Some(a) = self.api("clear_accounts_cache") {
            a.clear_accounts_cache()
        }
    }
    fn start(
        &mut self,
        _options: &crate::api::bingle_api::StartOptions,
    ) -> Result<(), BingleError> {
        Err(BingleError::Other(
            "not supported in handler context".to_string(),
        ))
    }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn handle_lookup(&self, handle: &Handle) -> Result<Option<UserId>, BingleError> {
        self.api("handle_lookup")
            .ok_or_else(|| BingleError::Other("API dropped".to_string()))
            .and_then(|a| a.handle_lookup(handle))
    }
    fn handle_lookup_by_id(&self, user_id: &UserId) -> Option<Handle> {
        self.api("handle_lookup_by_id")
            .and_then(|a| a.handle_lookup_by_id(user_id))
    }
    fn send_message_to_id(
        &self,
        user_id: &UserId,
        message: serde_json::Value,
        progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        self.api("send_message_to_id")
            .ok_or_else(|| BingleError::Other("API dropped".to_string()))
            .and_then(|a| a.send_message_to_id(user_id, message, progress))
    }
    fn send_message_to_handle(
        &self,
        handle: &Handle,
        message: serde_json::Value,
        progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        self.api("send_message_to_handle")
            .ok_or_else(|| BingleError::Other("API dropped".to_string()))
            .and_then(|a| a.send_message_to_handle(handle, message, progress))
    }
    fn send_message_to_network(
        &self,
        nsk: &NetworkEndpoint,
        user_id: &UserId,
        message: serde_json::Value,
        progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        self.api("send_message_to_network")
            .ok_or_else(|| BingleError::Other("API dropped".to_string()))
            .and_then(|a| a.send_message_to_network(nsk, user_id, message, progress))
    }
    fn send_message_to_id_with_response(
        &self,
        user_id: &UserId,
        message: serde_json::Value,
        progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        self.api("send_message_to_id_with_response")
            .ok_or_else(|| BingleError::Other("API dropped".to_string()))
            .and_then(|a| a.send_message_to_id_with_response(user_id, message, progress))
    }
    fn send_message_to_handle_with_response(
        &self,
        handle: &Handle,
        message: serde_json::Value,
        progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        self.api("send_message_to_handle_with_response")
            .ok_or_else(|| BingleError::Other("API dropped".to_string()))
            .and_then(|a| a.send_message_to_handle_with_response(handle, message, progress))
    }
    fn send_message_to_network_with_response(
        &self,
        nsk: &NetworkEndpoint,
        user_id: &UserId,
        message: serde_json::Value,
        progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        self.api("send_message_to_network_with_response")
            .ok_or_else(|| BingleError::Other("API dropped".to_string()))
            .and_then(|a| a.send_message_to_network_with_response(nsk, user_id, message, progress))
    }
    fn set_on_message(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnConnectHandler>>) {}
    fn set_on_listening(
        &mut self,
        _handler: Option<Arc<crate::api::bingle_api::OnListeningHandler>>,
    ) {
    }
}

impl BingleApiInternal for LockingApiWrapper {
    fn mutex_handle_request(&self, from_id: String, req: MutexRequest) {
        if let Some(a) = self.api("mutex_handle_request") {
            a.mutex_handle_request(from_id, req)
        }
    }
    fn mutex_handle_response(&self, from_id: String, resp: MutexResponse) {
        if let Some(a) = self.api("mutex_handle_response") {
            a.mutex_handle_response(from_id, resp)
        }
    }
    fn mutex_handle_release(&self, from_id: String, rel: MutexRelease) {
        if let Some(a) = self.api("mutex_handle_release") {
            a.mutex_handle_release(from_id, rel)
        }
    }
    fn get_relay_state(&self) -> String {
        self.api("get_relay_state")
            .map(|a| a.get_relay_state())
            .unwrap_or_else(|| "off".to_string())
    }
    fn set_state(&self, state: crate::engine::EngineState) {
        if let Some(a) = self.api("set_state") {
            a.set_state(state)
        }
    }
    fn get_state(&self) -> crate::engine::EngineState {
        self.api("get_state")
            .map(|a| a.get_state())
            .unwrap_or(crate::engine::EngineState::StunIdentify)
    }
    fn set_nat_type(&self, nat: crate::engine::NatType) {
        if let Some(a) = self.api("set_nat_type") {
            a.set_nat_type(nat)
        }
    }
    fn get_last_public_addr(&self) -> Option<SocketAddr> {
        self.api("get_last_public_addr")
            .and_then(|a| a.get_last_public_addr())
    }
    fn ddb_register_ip(&self, endpoint: SocketAddr, am_relay: bool) -> Result<(), BingleError> {
        self.api("ddb_register_ip")
            .ok_or_else(|| BingleError::Other("API dropped".to_string()))
            .and_then(|a| a.ddb_register_ip(endpoint, am_relay))
    }
    fn ddb_register_relay(
        &self,
        relay_id: String,
        relay_sig: Option<String>,
    ) -> Result<(), BingleError> {
        self.api("ddb_register_relay")
            .ok_or_else(|| BingleError::Other("API dropped".to_string()))
            .and_then(|a| a.ddb_register_relay(relay_id, relay_sig))
    }
    fn ddb_signoff(&self) -> Result<(), BingleError> {
        self.api("ddb_signoff")
            .ok_or_else(|| BingleError::Other("API dropped".to_string()))
            .and_then(|a| a.ddb_signoff())
    }
    fn update_turn_listener_relay(
        &self,
        relay_id: String,
        relay_addr: SocketAddr,
    ) -> Result<(), BingleError> {
        self.api("update_turn_listener_relay")
            .ok_or_else(|| BingleError::Other("API dropped".to_string()))
            .and_then(|a| a.update_turn_listener_relay(relay_id, relay_addr))
    }
    fn start_relay_keep_alive(&self, relay_id: String, relay_addr: SocketAddr) {
        if let Some(a) = self.api("start_relay_keep_alive") {
            a.start_relay_keep_alive(relay_id, relay_addr)
        }
    }
    fn stop_relay_keep_alive(&self) {
        if let Some(a) = self.api("stop_relay_keep_alive") {
            a.stop_relay_keep_alive()
        }
    }
    fn turn_client_handle_listen_response(&self, relay_addr: SocketAddr, relay_id: String) {
        if let Some(a) = self.api("turn_client_handle_listen_response") {
            a.turn_client_handle_listen_response(relay_addr, relay_id)
        }
    }
    fn turn_lookup_addr_by_id(&self, id: String) -> Option<SocketAddr> {
        self.api("turn_lookup_addr_by_id")
            .and_then(|a| a.turn_lookup_addr_by_id(id))
    }
    fn turn_handle_call(
        &self,
        source_id: String,
        dest_id: String,
        source: SocketAddr,
        dest: SocketAddr,
    ) -> i32 {
        self.api("turn_handle_call")
            .map(|a| a.turn_handle_call(source_id, dest_id, source, dest))
            .unwrap_or(-1)
    }
    fn turn_handle_listen(&self, id: String, source: SocketAddr) -> bool {
        self.api("turn_handle_listen")
            .map(|a| a.turn_handle_listen(id, source))
            .unwrap_or(false)
    }
    fn turn_handle_called(&self, source: SocketAddr, dest: SocketAddr, channel: u16) {
        if let Some(a) = self.api("turn_handle_called") {
            a.turn_handle_called(source, dest, channel)
        }
    }
    fn notify_listening(&self, listening: bool, nat_type: crate::engine::NatType) {
        if let Some(a) = self.api("notify_listening") {
            a.notify_listening(listening, nat_type)
        }
    }
    fn set_relay_state(&self, state: crate::engine::RelayState) {
        if let Some(a) = self.api("set_relay_state") {
            a.set_relay_state(state)
        }
    }
    fn get_peer_ddb_target(&self) -> Option<usize> {
        self.api("get_peer_ddb_target")
            .and_then(|a| a.get_peer_ddb_target())
    }
    fn ddb_upsert_record(&self, record: AdvertRecord) {
        if let Some(a) = self.api("ddb_upsert_record") {
            a.ddb_upsert_record(record)
        }
    }
    fn ddb_delete_record(&self, id: &str) {
        if let Some(a) = self.api("ddb_delete_record") {
            a.ddb_delete_record(id)
        }
    }
    fn relay_finder_remove_relay(&self, relay_id: &str) {
        if let Some(a) = self.api("relay_finder_remove_relay") {
            a.relay_finder_remove_relay(relay_id)
        }
    }
    fn relay_finder_clear_state_cache(&self) {
        if let Some(a) = self.api("relay_finder_clear_state_cache") {
            a.relay_finder_clear_state_cache()
        }
    }
    fn ddb_backend_size(&self) -> usize {
        self.api("ddb_backend_size")
            .map(|a| a.ddb_backend_size())
            .unwrap_or(0)
    }
    fn initialize_relay(&self) {
        if let Some(a) = self.api("initialize_relay") {
            a.initialize_relay()
        }
    }
    fn is_relay(&self) -> bool {
        self.api("is_relay").map(|a| a.is_relay()).unwrap_or(false)
    }
    fn signal_signon_complete(&self) {
        if let Some(a) = self.api("signal_signon_complete") {
            a.signal_signon_complete()
        }
    }
    fn reset_signon_complete(&self) {
        if let Some(a) = self.api("reset_signon_complete") {
            a.reset_signon_complete()
        }
    }
    fn ripple_message(
        &self,
        message: serde_json::Value,
        originator_id: String,
        ddb_backend: &dyn crate::ddb::DdbBackend,
    ) {
        if let Some(a) = self.api("ripple_message") {
            a.ripple_message(message, originator_id, ddb_backend)
        }
    }
    fn get_signing_key(&self) -> Option<SigningKey> {
        self.api("get_signing_key")
            .and_then(|a| a.get_signing_key())
    }
}

impl Router {
    /// Backward-compatible helper retained for tests; no TLS state is used.
    pub fn with_current_router<R>(_router: Arc<Router>, f: impl FnOnce() -> R) -> R {
        f()
    }

    /// Backward-compatible helper retained for tests; no TLS state is kept.
    pub fn current() -> Option<Arc<Router>> {
        None
    }

    fn dispatch_message<H: MessageHandler + ?Sized>(
        handler: &H,
        api: Arc<dyn BingleApiBoth>,
        msg: &Message,
        from: &FromStruct,
    ) -> Vec<serde_json::Value> {
        tracing::info!("Router::dispatch_message: {:?}", msg);
        match msg {
            Message::PlainText(pt) => handler.on_plain_text(api.clone(), from, pt),
            Message::Relay(r) => match r {
                RelayMessage::Call(m) => handler.on_relay_call(api.clone(), from, m),
                RelayMessage::RelayResponse(m) => handler.on_relay_response(api.clone(), from, m),
                RelayMessage::TriangleTest1(m) => handler.on_triangle_test1(api.clone(), from, m),
                RelayMessage::TriangleTest2(m) => handler.on_triangle_test2(api.clone(), from, m),
                RelayMessage::TriangleTest3(m) => handler.on_triangle_test3(api.clone(), from, m),
                RelayMessage::TriangleTest1Response(m) => {
                    handler.on_triangle_test1_response(api.clone(), from, m)
                }
                RelayMessage::Listen(m) => handler.on_relay_listen(api.clone(), from, m),
                RelayMessage::Check(m) => handler.on_relay_check(api.clone(), from, m),
                RelayMessage::ListenResponse(m) => {
                    handler.on_relay_listen_response(api.clone(), from, m)
                }
                RelayMessage::CheckResponse(m) => {
                    handler.on_relay_check_response(api.clone(), from, m)
                }
                RelayMessage::CallResponse(m) => {
                    handler.on_relay_call_response(api.clone(), from, m)
                }
                RelayMessage::KeepAlive(m) => handler.on_relay_keep_alive(api.clone(), from, m),
                RelayMessage::RelayCalled(m) => handler.on_relay_called(api.clone(), from, m),
            },
            Message::Ddb(d) => match d {
                DdbMessage::UpsertResolve(m) => handler.on_ddb_upsert_resolve(api.clone(), from, m),
                DdbMessage::QueryResolve(m) => handler.on_ddb_query_resolve(api.clone(), from, m),
                DdbMessage::InitResolve(m) => handler.on_ddb_init_resolve(api.clone(), from, m),
                DdbMessage::DumpResolve(m) => handler.on_ddb_dump_resolve(api.clone(), from, m),
                DdbMessage::GetRelaysStatus(m) => {
                    handler.on_ddb_get_relays_status(api.clone(), from, m)
                }
                DdbMessage::RelaysStatusResponse(m) => {
                    handler.on_ddb_relays_status_response(api.clone(), from, m)
                }
                DdbMessage::Signon(m) => handler.on_ddb_signon(api.clone(), from, m),
                DdbMessage::SignonResponse(m) => {
                    handler.on_ddb_signon_response(api.clone(), from, m)
                }
                DdbMessage::Signoff(m) => handler.on_ddb_signoff(api.clone(), from, m),
                _ => handler.on_unimplemented(msg),
            },
            Message::Ping(p) => match p {
                PingMessage::Ping(m) => handler.on_ping_ping(api.clone(), from, m),
                PingMessage::Response(m) => handler.on_ping_response(api.clone(), from, m),
            },
            Message::Mutex(m) => match m {
                MutexMessage::Request(req) => handler.on_mutex_request(api.clone(), from, req),
                MutexMessage::Response(resp) => handler.on_mutex_response(api.clone(), from, resp),
                MutexMessage::Release(rel) => handler.on_mutex_release(api.clone(), from, rel),
            },
            Message::ReportFail(rf) => handler.on_report_fail(api.clone(), from, rf),
            Message::Unknown(v) => handler.on_unknown(api.clone(), from, v),
        }
        tracing::info!("Router::dispatch_message done: {:?}", msg);
        from.responses
            .lock()
            .map(|mut g| g.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn new(api: crate::api::bingle_api::BingleApiBothType) -> Self {
        Self {
            sender: Mutex::new(None),
            api: Mutex::new(Some(api)),
            last_from: Mutex::new(None),
            last_response_tag: Mutex::new(None),
            on_message: Mutex::new(None),
            am_relay: std::sync::atomic::AtomicBool::new(false),
            ddb_backend: Mutex::new(None),
        }
    }

    pub fn set_sender(
        &self,
        cb: Option<
            Arc<
                dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool
                    + Send
                    + Sync
                    + 'static,
            >,
        >,
    ) {
        if let Ok(mut g) = self.sender.lock() {
            *g = cb;
        }
    }
    pub fn get_sender(
        &self,
    ) -> Option<
        Arc<dyn Fn(&NetworkEndpoint, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>,
    > {
        match self.sender.lock() {
            Ok(g) => g.clone(),
            Err(_) => None,
        }
    }

    pub fn set_bingle_api(&self, api: Option<crate::api::bingle_api::BingleApiBothType>) {
        if let Ok(mut g) = self.api.lock() {
            *g = api;
        }
    }
    pub fn get_bingle_api(&self) -> Option<Arc<dyn BingleApiBoth>> {
        match self.api.lock() {
            Ok(g) => g.as_ref().and_then(|w| w.upgrade()),
            Err(_) => None,
        }
    }

    pub fn set_last_from(&self, addr: Option<SocketAddr>) {
        if let Ok(mut g) = self.last_from.lock() {
            *g = addr;
        }
    }
    pub fn get_last_from(&self) -> Option<SocketAddr> {
        match self.last_from.lock() {
            Ok(g) => *g,
            Err(_) => None,
        }
    }

    pub fn set_last_response_tag(&self, tag: Option<String>) {
        if let Ok(mut g) = self.last_response_tag.lock() {
            *g = tag;
        }
    }
    pub fn get_last_response_tag(&self) -> Option<String> {
        match self.last_response_tag.lock() {
            Ok(g) => g.clone(),
            Err(_) => None,
        }
    }

    pub fn set_on_message(&self, cb: Option<Arc<crate::api::bingle_api::OnMessageHandler>>) {
        if let Ok(mut g) = self.on_message.lock() {
            *g = cb;
        }
    }
    pub fn get_on_message(&self) -> Option<Arc<crate::api::bingle_api::OnMessageHandler>> {
        match self.on_message.lock() {
            Ok(g) => g.clone(),
            Err(_) => None,
        }
    }

    pub fn set_am_relay(&self, b: bool) {
        self.am_relay.store(b, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn get_am_relay(&self) -> bool {
        self.am_relay.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub fn set_ddb_backend(&self, backend: Option<Arc<Mutex<crate::ddb::InMemoryDdbBackend>>>) {
        if let Ok(mut g) = self.ddb_backend.lock() {
            *g = backend;
        }
    }
    pub fn get_ddb_backend(&self) -> Option<Arc<Mutex<crate::ddb::InMemoryDdbBackend>>> {
        match self.ddb_backend.lock() {
            Ok(g) => g.clone(),
            Err(_) => None,
        }
    }

    fn send_outbound_response(&self, from: &FromStruct, outbound: serde_json::Value) {
        if let Some(sender) = self.get_sender() {
            if !sender(&from.network_source_key, &from.id, outbound.clone()) {
                tracing::warn!(
                    "[router::send_outbound_response] sender callback returned false for {}",
                    from.id
                );
            }
            return;
        }

        if let Some(api) = self.get_bingle_api() {
            match api.send_message_to_network(&from.network_source_key, &from.id, outbound, None) {
                Ok(true) => {}
                Ok(false) => tracing::warn!(
                    "[router::send_outbound_response] api send returned false for {}",
                    from.id
                ),
                Err(e) => tracing::warn!(
                    "[router::send_outbound_response] api send failed for {}: {}",
                    from.id,
                    e
                ),
            }
            return;
        }

        tracing::warn!(
            "[router::send_outbound_response] no sender and no api available for {}",
            from.id
        );
    }

    pub fn route_with_network<H>(
        self: &Arc<Self>,
        handler: H,
        msg: &Message,
        from_id: &str,
        from_ep: &NetworkEndpoint,
    ) where
        H: MessageHandler + Send + Sync + 'static,
    {
        let Some(api_base) = self.get_bingle_api() else {
            tracing::warn!(
                "[router::route_with_network] No BingleApi available to pass to handler"
            );
            return;
        };
        let api: Arc<dyn BingleApiBoth> = Arc::new(LockingApiWrapper {
            api: Arc::downgrade(&api_base),
        });
        let from = FromStruct {
            id: from_id.to_string(),
            network_source_key: from_ep.clone(),
            router: self.clone(),
            responses: Arc::new(Mutex::new(Vec::new())),
        };
        let msg = msg.clone();
        let msg2 = msg.clone();
        tracing::info!(
            "[router::route_with_network] spawn handler thread to route message: {:?}",
            msg
        );
        if std::thread::Builder::new()
            .name("router-handler-thread".to_string())
            .spawn(move || {
                let outbound_responses = Self::dispatch_message(&handler, api, &msg, &from);
                for outbound in outbound_responses {
                    tracing::info!(
                        "[router::route_with_network] sending outbound response: {:?}",
                        outbound
                    );
                    let outbound2 = outbound.clone();
                    from.router.send_outbound_response(&from, outbound);
                    tracing::info!(
                        "[router::route_with_network] sent outbound response: {:?}",
                        outbound2
                    );
                }
            })
            .is_err()
        {
            tracing::warn!(
                "[router::route_with_network] failed to spawn background handler thread"
            );
        }
        tracing::info!(
            "[router::route_with_network] handler thread spawned for message: {:?}",
            msg2
        );
    }

    pub fn route<H: MessageHandler + ?Sized>(
        self: &Arc<Self>,
        handler: &H,
        msg: &Message,
        from_id: &str,
    ) -> Vec<serde_json::Value> {
        let nsk = if let Some(addr) = self.get_last_from() {
            NetworkEndpoint::new_direct(addr)
        } else {
            NetworkEndpoint::new_direct("0.0.0.0:0".parse().unwrap())
        };
        let Some(api_base) = self.get_bingle_api() else {
            tracing::warn!("[router::route] No BingleApi available to pass to handler");
            return Vec::new();
        };
        let api: Arc<dyn BingleApiBoth> = Arc::new(LockingApiWrapper {
            api: Arc::downgrade(&api_base),
        });
        let from = FromStruct {
            id: from_id.to_string(),
            network_source_key: nsk,
            router: self.clone(),
            responses: Arc::new(Mutex::new(Vec::new())),
        };
        Self::dispatch_message(handler, api, msg, &from)
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
