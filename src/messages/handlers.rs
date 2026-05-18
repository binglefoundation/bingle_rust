use crate::api::bingle_api::{BingleApi, BingleApiInternal, BingleApiBoth, BingleError};
use crate::relay::relay_finder::RelayFinderTrait;
use crate::engine::BingleAccessUnsafeForTests;
use crate::ddb::DdbBackend;
use crate::messages::types::*;
use tracing::{error, warn};
use std::sync::Arc;

#[derive(Clone)]
pub struct FromStruct {
    pub id: String,
    pub network_source_key: crate::api::bingle_api::NetworkEndpoint,
    pub router: std::sync::Arc<crate::messages::router::Router>,
}

impl std::fmt::Debug for FromStruct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FromStruct")
            .field("id", &self.id)
            .field("network_source_key", &self.network_source_key)
            .finish()
    }
}

// Adapter to allow passing the composite API where a plain BingleApi is required (e.g., RelayFinder)
struct BothAsApi { inner: Arc<dyn BingleApiBoth> }
impl BingleApiInternal for BothAsApi {
    fn mutex_handle_request(&self, from_id: String, req: crate::messages::types::MutexRequest) { self.inner.mutex_handle_request(from_id, req) }
    fn mutex_handle_response(&self, from_id: String, resp: crate::messages::types::MutexResponse) { self.inner.mutex_handle_response(from_id, resp) }
    fn mutex_handle_release(&self, from_id: String, rel: crate::messages::types::MutexRelease) { self.inner.mutex_handle_release(from_id, rel) }
    fn get_relay_state(&self) -> String { self.inner.get_relay_state() }
    fn set_state(&self, state: crate::engine::EngineState) { self.inner.set_state(state) }
    fn get_state(&self) -> crate::engine::EngineState { self.inner.get_state() }
    fn set_nat_type(&self, nat: crate::engine::NatType) { self.inner.set_nat_type(nat) }
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { self.inner.get_last_public_addr() }
    fn ddb_register_ip(&self, endpoint: std::net::SocketAddr, am_relay: bool) -> Result<(), BingleError> { self.inner.ddb_register_ip(endpoint, am_relay) }
    fn ddb_register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), BingleError> { self.inner.ddb_register_relay(relay_id, relay_sig) }
    fn update_turn_listener_relay(&self, relay_id: String, relay_addr: std::net::SocketAddr) -> Result<(), BingleError> { self.inner.update_turn_listener_relay(relay_id, relay_addr) }
    fn turn_client_handle_listen_response(&self, relay_addr: std::net::SocketAddr, relay_id: String) { self.inner.turn_client_handle_listen_response(relay_addr, relay_id) }
    fn turn_lookup_addr_by_id(&self, id: String) -> Option<std::net::SocketAddr> { self.inner.turn_lookup_addr_by_id(id) }
    fn turn_handle_call(&self, source_id: String, dest_id: String, source: std::net::SocketAddr, dest: std::net::SocketAddr) -> i32 { self.inner.turn_handle_call(source_id, dest_id, source, dest) }
    fn turn_handle_listen(&self, id: String, source: std::net::SocketAddr) -> bool { self.inner.turn_handle_listen(id, source) }
    fn turn_handle_called(&self, source: std::net::SocketAddr, dest: std::net::SocketAddr, channel: u16) { self.inner.turn_handle_called(source, dest, channel) }
    fn notify_listening(&self, listening: bool, nat_type: crate::engine::NatType) { self.inner.notify_listening(listening, nat_type) }
    fn set_relay_state(&self, state: crate::engine::RelayState) { self.inner.set_relay_state(state) }
    fn get_peer_ddb_target(&self) -> Option<usize> { self.inner.get_peer_ddb_target() }
    fn ddb_upsert_record(&self, record: crate::ddb::AdvertRecord) { self.inner.ddb_upsert_record(record) }
    fn ddb_backend_size(&self) -> usize { self.inner.ddb_backend_size() }
    fn initialize_relay(&self) { self.inner.initialize_relay() }
    fn is_relay(&self) -> bool { self.inner.is_relay() }
    fn signal_signon_complete(&self) { self.inner.signal_signon_complete() }
    fn reset_signon_complete(&self) { self.inner.reset_signon_complete() }
    fn ripple_message(&self, message: serde_json::Value, originator_id: String, ddb_backend: &dyn crate::ddb::DdbBackend) { self.inner.ripple_message(message, originator_id, ddb_backend) }
}
impl BingleApi for BothAsApi {
    fn debug_print_options(&self) { self.inner.debug_print_options() }
    fn list_all_relays(&self, include_self: bool) -> Vec<crate::relay::relay_finder::RelayInfo> { self.inner.list_all_relays(include_self) }
    fn get_my_id(&self) -> Option<String> { self.inner.get_my_id() }
    fn get_user_id(&self) -> Option<String> { self.inner.get_user_id() }
    fn get_handle(&self) -> Option<String> { self.inner.get_handle() }
    fn get_app_id(&self) -> Option<u64> { self.inner.get_app_id() }
    fn get_algo_provider_config(&self) -> Option<crate::blockchain::algo_ops::AlgoChainConfig> { self.inner.get_algo_provider_config() }
    fn start(&mut self, options: &crate::api::bingle_api::StartOptions) -> Result<(), BingleError> { self.inner.access_unsafe_for_tests(|a| a.start(options)) }
    fn stop(&mut self) { self.inner.access_unsafe_for_tests(|a| a.stop()) }
    fn network_change(&mut self) { self.inner.access_unsafe_for_tests(|a| a.network_change()) }
    fn handle_lookup(&self, _handle: &crate::api::bingle_api::Handle) -> Result<Option<crate::api::bingle_api::UserId>, BingleError> { self.inner.handle_lookup(_handle) }
    fn handle_lookup_by_id(&self, user_id: &crate::api::bingle_api::UserId) -> Option<crate::api::bingle_api::Handle> { self.inner.handle_lookup_by_id(user_id) }
    fn send_message_to_id(&self, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<bool, BingleError> { self.inner.send_message_to_id(user_id, message, progress) }
    fn send_message_to_handle(&self, handle: &crate::api::bingle_api::Handle, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<bool, BingleError> { self.inner.send_message_to_handle(handle, message, progress) }
    fn send_message_to_network(&self, nsk: &crate::api::bingle_api::NetworkEndpoint, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<bool, BingleError> { self.inner.send_message_to_network(nsk, user_id, message, progress) }
    fn send_message_to_id_with_response(&self, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, BingleError> { self.inner.send_message_to_id_with_response(user_id, message, progress) }
    fn send_message_to_handle_with_response(&self, handle: &crate::api::bingle_api::Handle, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, BingleError> { self.inner.send_message_to_handle_with_response(handle, message, progress) }
    fn send_message_to_network_with_response(&self, nsk: &crate::api::bingle_api::NetworkEndpoint, user_id: &crate::api::bingle_api::UserId, message: serde_json::Value, progress: Option<Arc<crate::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, BingleError> { self.inner.send_message_to_network_with_response(nsk, user_id, message, progress) }
    fn set_on_message(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnMessageHandler>>) { }
    fn set_on_connect(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnConnectHandler>>) { }
    fn set_on_listening(&mut self, _handler: Option<Arc<crate::api::bingle_api::OnListeningHandler>>) { }
}

pub trait MessageHandler {
    // Plain text
    fn on_plain_text(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &PlainTextMessage) {
        // Build JSON for callback
        let json = serde_json::to_value(msg).unwrap_or_else(|_| serde_json::json!({"text": msg.text}));
        // Delegate to API on_message via the per-API Router if installed
        if let Some(cb) = from.router.get_on_message() {
            // Normalize sender id: issuer without suffix
            let sender_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
            // Look up the sender's handle by id via the API. If not found, log an error and abort.
            match api.handle_lookup_by_id(&sender_id) {
                Some(sender_handle) => cb(sender_id, sender_handle, json),
                None => {
                    tracing::error!(
                        "[MessageHandler::on_plain_text] failed to resolve handle for sender id {}",
                        sender_id
                    );
                }
            }
            return;
        }
        // Fallback to logging if no on_message callback is installed
        tracing::warn!("[MessageHandler::on_plain_text] No callback set up {}", serde_json::to_string(&json).unwrap_or_else(|_| "<unprintable>".into()));
    }

    // Ping messages
    fn on_ping_ping(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &PingPing) {
        tracing::info!("[on_ping_ping] handling ping {:?} from {:?}", msg, from);
        // Reply with PingResponse: app="ping", type="response", verifiedId from API, text="ACK: {text}"
        let sender_opt = from.router.get_sender();
        if sender_opt.is_none() {
            warn!("[handlers::on_ping_ping] No sender available");
            return;
        }
        let sender = sender_opt.unwrap();

            // Obtain our id (verifiedId)
            let my_id = match api.get_my_id() {
                Some(id) => id,
                None => {
                    warn!("[handlers::on_ping_ping] get_my_id returned None");
                    return;
                }
            };

            // Build response JSON following OpenAPI schema
            let mut json_obj = serde_json::Map::new();
            json_obj.insert("app".to_string(), serde_json::Value::String("ping".to_string()));
            json_obj.insert("type".to_string(), serde_json::Value::String("response".to_string()));
            json_obj.insert("verifiedId".to_string(), serde_json::Value::String(my_id));
            // If responseTag was provided on request context, echo it
            if let Some(tag) = from.router.get_last_response_tag() {
                json_obj.insert("responseTag".to_string(), serde_json::Value::String(tag));
            }
            let ack_text = format!("ACK: {}", msg.text.clone().unwrap_or_default());
            json_obj.insert("text".to_string(), serde_json::Value::String(ack_text));
            let json_val = serde_json::Value::Object(json_obj);

            // Prepare destination (use from.id (issuer) as base32 algorand address without conversion)
            let nsk = from.network_source_key.clone();
            let user_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
            tracing::info!("[handlers::on_ping_ping] sending response {:?} to {:?}", json_val, nsk);
            let ok = sender(&nsk, &user_id, json_val);
            if !ok {
                tracing::warn!("[handlers::on_ping_ping] sender returned false");
            }
    }
    fn on_ping_response(&self, _api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &PingResponse) {
        if msg.response_tag.is_none() {
            // Build JSON for callback
            let json = serde_json::to_value(msg).unwrap_or_else(|_| serde_json::json!({"verifiedId": msg.verified_id}));
            // Delegate to API on_message via the per-API Router if installed
            if let Some(cb) = from.router.get_on_message() {
                // Normalize sender id: issuer without suffix
                let sender_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
                // Use direct socket address as sender_handle when available
                let sender_handle = from
                    .network_source_key
                    .inet_socket_address()
                    .map(|a: std::net::SocketAddr| a.to_string())
                    .unwrap_or_else(|| "".to_string());
                cb(sender_id, sender_handle, json);
                return;
            }
        }
        self.on_unimplemented(&Message::Ping(PingMessage::Response(msg.clone())));
    }

    // Relay messages
    fn on_relay_call(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &RelayCall) {
        let router = &from.router;
        if !router.get_am_relay() {
            warn!("[handlers::on_relay_call] Not a relay: ignoring Call");
            return;
        }
        let src = match router.get_last_from() {
            Some(a) => a,
            None => { warn!("[handlers::on_relay_call] No source address available"); return; }
        };
        // Resolve destination address by id recorded via Listen using internal API
        let called_id_raw = msg.called_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
        let dest = match api.turn_lookup_addr_by_id(called_id_raw.clone()) {
            Some(a) => a,
            None => { warn!("[handlers::on_relay_call] called id not registered: {}", called_id_raw); return; }
        };
        let source_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
        let ch = api.turn_handle_call(source_id, called_id_raw.clone(), src, dest);
        if ch < 0 { warn!("[handlers::on_relay_call] turn_handle_call failed"); return; }

        // Before setting the response, notify the called node with a RelayCalled message
        if let Some(sender) = router.get_sender() {
            let msg = Message::Relay(RelayMessage::RelayCalled(RelayCalled { app: None, channel: ch as u16 }));
            let json_val = crate::messages::marshal::to_json_value(&msg);
            let nsk = crate::api::bingle_api::NetworkEndpoint::new_direct(dest);
            let ok = sender(&nsk, &called_id_raw, json_val);
            if !ok { warn!("[handlers::on_relay_call] sender returned false when sending RelayCalled"); }
        } else {
            error!("[handlers::on_relay_call] No sender available to notify called node");
        }

        // Build RelayResponse { app: null, channel }
        let mut obj = serde_json::Map::new();
        obj.insert("app".to_string(), serde_json::Value::Null);
        obj.insert("type".to_string(), serde_json::Value::String("RelayResponse".to_string()));
        obj.insert("channel".to_string(), serde_json::Value::Number(serde_json::Number::from(ch as u64)));
        if let Some(tag) = router.get_last_response_tag() { obj.insert("responseTag".to_string(), serde_json::Value::String(tag)); }
        router.set_outbound_response(Some(serde_json::Value::Object(obj)));
    }
    fn on_relay_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::RelayResponse(_msg.clone()))); }
    fn on_triangle_test1(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayTriangleTest1) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest1(_msg.clone()))); }
    fn on_triangle_test2(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayTriangleTest2) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest2(_msg.clone()))); }
    fn on_triangle_test3(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayTriangleTest3) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest3(_msg.clone()))); }
    fn on_triangle_test1_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayTriangleTest1Response) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest1Response(_msg.clone()))); }
    fn on_relay_listen(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, _msg: &RelayListen) {
        // Only process on relay nodes
        let router = &from.router;
        if !router.get_am_relay() {
            warn!("[handlers::on_relay_listen] Not a relay: ignoring Listen request");
            return;
        }
        // Source address must be known from DTLS/mux layer
        let src = match router.get_last_from() {
            Some(a) => a,
            None => {
                warn!("[handlers::on_relay_listen] No source address available");
                return;
            }
        };
        let source_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
        let _ok = api.turn_handle_listen(source_id, src);
        // Build and stash a ListenResponse; include responseTag if present
        let mut obj = serde_json::Map::new();
        obj.insert("app".to_string(), serde_json::Value::Null);
        obj.insert("type".to_string(), serde_json::Value::String("ListenResponse".to_string()));
        if let Some(tag) = router.get_last_response_tag() {
            obj.insert("responseTag".to_string(), serde_json::Value::String(tag));
        }
        router.set_outbound_response(Some(serde_json::Value::Object(obj)));
    }
    fn on_relay_check(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &RelayCheck) {
        // Send CheckResponse with current relay state back to the last sender address using the real Bingle API sender
        let router_opt = Some(from.router.clone());
        let sender_opt = router_opt.as_ref().and_then(|r| r.get_sender());
        if sender_opt.is_none() {
            warn!("[handlers::on_relay_check] No sender available");
            if let Some(router) = router_opt {
                // Use typed Fail message per OpenAPI instead of building a raw map
                let fail = crate::messages::types::Fail { app: None, typ: "fail".to_string(), response_tag: router.get_last_response_tag(), reason: "no sender available".to_string() };
                let json = serde_json::to_value(fail).unwrap_or(serde_json::Value::Null);
                router.set_outbound_response(Some(json));
            }
            return;
        }
        let sender = sender_opt.unwrap();
        // Compose JSON manually to include responseTag if present
        let mut json_obj = serde_json::Map::new();
        json_obj.insert("app".to_string(), serde_json::Value::Null);
        json_obj.insert("type".to_string(), serde_json::Value::String("CheckResponse".to_string()));
        let state = api.get_relay_state();
        json_obj.insert("state".to_string(), serde_json::Value::String(state));
        if let Some(tag) = msg.tag.clone() {
            json_obj.insert("responseTag".to_string(), serde_json::Value::String(tag));
        }
        let json_val = serde_json::Value::Object(json_obj);
        let nsk = from.network_source_key.clone();
        // Convert from.id (issuer) to raw Algorand address (base32)
        let user_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
        tracing::info!("[handlers::on_relay_check] Sending CheckResponse to {}: {}", user_id, json_val);
        let _ok = sender(&nsk, &user_id, json_val);
    }
    fn on_relay_listen_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayListenResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::ListenResponse(_msg.clone()))); }
    fn on_relay_check_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayCheckResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::CheckResponse(_msg.clone()))); }
    fn on_relay_call_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayCallResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::CallResponse(_msg.clone()))); }
    fn on_relay_keep_alive(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayKeepAlive) { self.on_unimplemented(&Message::Relay(RelayMessage::KeepAlive(_msg.clone()))); }

    // New: RelayCalled handler (client-side) – register TURN mapping via internal API
    fn on_relay_called(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &RelayCalled) {
        // The UDP sender of this message should be the relay address
        let relay_addr = match from.router.get_last_from() {
            Some(a) => a,
            None => { warn!("[handlers::on_relay_called] No relay address (last_from) available"); return; }
        };
        // Our public address must be known (from STUN/static); use API to obtain
        let my_pub = match api.get_last_public_addr() {
            Some(a) => a,
            None => { warn!("[handlers::on_relay_called] No public address available to register TURN mapping"); return; }
        };
        api.turn_handle_called(relay_addr, my_pub, msg.channel);
    }

    // DDB messages (default to unimplemented unless overridden)
    fn on_ddb_upsert_resolve(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, msg: &DdbUpsertResolve) { self.on_unimplemented(&Message::Ddb(DdbMessage::UpsertResolve(msg.clone()))); }
    fn on_ddb_delete_resolve(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, msg: &DdbDeleteResolve) { self.on_unimplemented(&Message::Ddb(DdbMessage::DeleteResolve(msg.clone()))); }
    fn on_ddb_query_resolve(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, msg: &DdbQueryResolve) { self.on_unimplemented(&Message::Ddb(DdbMessage::QueryResolve(msg.clone()))); }
    fn on_ddb_init_resolve(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, msg: &DdbInitResolve) { self.on_unimplemented(&Message::Ddb(DdbMessage::InitResolve(msg.clone()))); }
    fn on_ddb_dump_resolve(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, msg: &DdbDumpResolve) { self.on_unimplemented(&Message::Ddb(DdbMessage::DumpResolve(msg.clone()))); }
    fn on_ddb_signon(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, msg: &DdbSignon) { self.on_unimplemented(&Message::Ddb(DdbMessage::Signon(msg.clone()))); }
    fn on_ddb_signon_response(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, msg: &DdbSignonResponse) { self.on_unimplemented(&Message::Ddb(DdbMessage::SignonResponse(msg.clone()))); }
    fn on_ddb_get_epoch(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, msg: &DdbGetEpoch) { self.on_unimplemented(&Message::Ddb(DdbMessage::GetEpoch(msg.clone()))); }
    fn on_ddb_epoch_info(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, msg: &DdbEpochInfo) { self.on_unimplemented(&Message::Ddb(DdbMessage::EpochInfo(msg.clone()))); }

    // Unknown
    fn on_unknown(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, raw: &serde_json::Value) {
        // Delegate to API on_message via the per-API Router if installed
        if let Some(cb) = from.router.get_on_message() {
            // Normalize sender id: issuer without suffix
            let sender_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
            // Look up the sender's handle by id via the API.
            match api.handle_lookup_by_id(&sender_id) {
                Some(sender_handle) => cb(sender_id, sender_handle, raw.clone()),
                None => {
                    tracing::error!(
                        "[MessageHandler::on_unknown] failed to resolve handle for sender id {}",
                        sender_id
                    );
                }
            }
            return;
        }
        tracing::info!("[UNIMPLEMENTED] Unknown message: {}", raw);
    }

    // Default unimplemented handler: prints the message JSON
    fn on_unimplemented(&self, msg: &Message) {
        tracing::warn!("[on_unimplemented] {}", serde_json::to_string(&crate::messages::marshal::to_json_value(msg)).unwrap_or_else(|_| "<unprintable>".into()));
    }
}

pub struct DefaultPrintingHandler;

impl MessageHandler for DefaultPrintingHandler {
    fn on_ddb_get_epoch(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &DdbGetEpoch) {
        let msg_tag = msg.tag.clone();
        let router = &from.router;
        // Only relays in Available state may serve getEpoch
        if !router.get_am_relay() {
            let mut obj = serde_json::Map::new();
            obj.insert("app".to_string(), serde_json::Value::String("ddb".into()));
            obj.insert("type".to_string(), serde_json::Value::String("fail".into()));
            if let Some(tag) = router.get_last_response_tag() { obj.insert("responseTag".to_string(), serde_json::Value::String(tag)); }
            obj.insert("text".to_string(), serde_json::Value::String("not a relay".into()));
            router.set_outbound_response(Some(serde_json::Value::Object(obj)));
            return;
        }
        // Consult API for relay state
        let state_ok = api.get_relay_state() == "available";
        if !state_ok {
            let mut obj = serde_json::Map::new();
            obj.insert("app".to_string(), serde_json::Value::String("ddb".into()));
            obj.insert("type".to_string(), serde_json::Value::String("fail".into()));
            if let Some(tag) = msg_tag { obj.insert("responseTag".to_string(), serde_json::Value::String(tag)); }
            obj.insert("text".to_string(), serde_json::Value::String("relay not available".into()));
            router.set_outbound_response(Some(serde_json::Value::Object(obj)));
            return;
        }
        // Build DdbEpochInfo from backend snapshot
        if let Some(backend_arc) = router.get_ddb_backend() {
            if let Ok(backend) = backend_arc.lock() {
                let (relay_ids, relay_endpoints) = backend.make_epoch_info();
                let info = crate::messages::types::DdbEpochInfo {
                    app: "ddb".into(),
                    epoch_id: msg.epoch_id,
                    tree_order: 2,
                    relay_ids,
                    relay_endpoints,
                    response_tag: msg_tag,
                    text: None,
                    data: None,
                };
                let resp = crate::messages::types::Message::Ddb(
                    crate::messages::types::DdbMessage::EpochInfo(info)
                );
                let json = crate::messages::marshal::to_json_value(&resp);
                router.set_outbound_response(Some(json));
            } else {
                let mut obj = serde_json::Map::new();
                obj.insert("app".to_string(), serde_json::Value::String("ddb".into()));
                obj.insert("type".to_string(), serde_json::Value::String("fail".into()));
                if let Some(tag) = router.get_last_response_tag() { obj.insert("responseTag".to_string(), serde_json::Value::String(tag)); }
                obj.insert("text".to_string(), serde_json::Value::String("ddb backend unavailable".into()));
                router.set_outbound_response(Some(serde_json::Value::Object(obj)));
            }
        } else {
            let mut obj = serde_json::Map::new();
            obj.insert("app".to_string(), serde_json::Value::String("ddb".into()));
            obj.insert("type".to_string(), serde_json::Value::String("fail".into()));
            if let Some(tag) = router.get_last_response_tag() { obj.insert("responseTag".to_string(), serde_json::Value::String(tag)); }
            obj.insert("text".to_string(), serde_json::Value::String("no ddb backend".into()));
            router.set_outbound_response(Some(serde_json::Value::Object(obj)));
        }
    }

    fn on_ddb_init_resolve(&self, _api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &DdbInitResolve) {
        let router = &from.router;
        if !router.get_am_relay() { return; }
        let sender_opt = router.get_sender();
        if sender_opt.is_none() { warn!("[handlers::on_ddb_init_resolve] No sender available"); return; }
        let sender = sender_opt.unwrap();

        // Get backend
        let backend_opt = router.get_ddb_backend();
        if backend_opt.is_none() { warn!("[handlers::on_ddb_init_resolve] No DDB backend available"); return; }
        let backend_arc = backend_opt.unwrap();
        let guard = backend_arc.lock();
        if guard.is_err() { warn!("[handlers::on_ddb_init_resolve] Backend lock poisoned"); return; }
        let backend = guard.unwrap();

        // Prepare destination from FromStruct
        let nsk = from.network_source_key.clone();
        let user_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();

        // Invoke backend handle_init: snapshot and send InitResponse + DumpResolve per record.
        let response_tag = msg.tag.clone();
        backend.handle_init(&nsk, &user_id, response_tag, &|nsk2, uid2, val| sender(nsk2, &uid2.to_string(), val));
    }

    fn on_ddb_upsert_resolve(&self, _api: Arc<dyn BingleApiBoth>, from: &FromStruct, up: &DdbUpsertResolve) {
        let router = &from.router;
        if !router.get_am_relay() { return; }
        // Validate sender id
        let sender_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        if up.record.id != up.start_id { return; }
        if !up.rippled && up.record.id != sender_id { return; }
        if up.tag.is_none() {
            tracing::error!("[handlers::on_ddb_upsert_resolve] No responseTag in DdbUpsertResolve {:?}", up);
            return;
        }
        // Upsert to backend
        if let Some(backend) = router.get_ddb_backend() {
            if let Ok(mut b) = backend.lock()
            {
                tracing::info!("[handlers::on_ddb_upsert_resolve] Upserting record: {:?}", up.record);
                b.upsert(up.record.clone()); }
            else {
                tracing::error!("[handlers::on_ddb_upsert_resolve] Could not lock DDB backend for upsert");
            }
        }
        else {
            tracing::error!("[handlers::on_ddb_upsert_resolve] No DDB backend available");
        }

        if !up.rippled {
            let mut rippled_up = up.clone();
            rippled_up.rippled = true;
            let ripple_msg = Message::Ddb(DdbMessage::UpsertResolve(rippled_up));
            let ripple_json = crate::messages::marshal::to_json_value(&ripple_msg);
            if let Some(backend) = router.get_ddb_backend() {
                if let Ok(b) = backend.lock() {
                    _api.ripple_message(ripple_json, up.start_id.clone(), &*b);
                }
            }

            // Prepare response JSON and stash on router for Engine/DTLS layer to send.
            let response_tag = up.tag.clone();

            let resp = crate::messages::types::Message::Ddb(
                crate::messages::types::DdbMessage::UpdateResponse(
                    crate::messages::types::DdbUpdateResponse { app: "ddb".to_string(), response_tag, text: None, data: None }
                )
            );
            let json = crate::messages::marshal::to_json_value(&resp);
            router.set_outbound_response(Some(json));
        }
    }

    fn on_ddb_delete_resolve(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &DdbDeleteResolve) {
        let router = &from.router;
        if !router.get_am_relay() { return; }
        // Validate sender id
        let sender_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        if !msg.rippled && msg.start_id != sender_id { return; }

        // Delete from backend
        if let Some(backend) = router.get_ddb_backend() {
            if let Ok(mut b) = backend.lock() { b.delete(&msg.start_id); }
        }

        // Prepare response JSON and stash on router for Engine/DTLS layer to send.
        let response_tag = msg.tag.clone();
        let resp = crate::messages::types::Message::Ddb(
            crate::messages::types::DdbMessage::UpdateResponse(
                crate::messages::types::DdbUpdateResponse { app: "ddb".to_string(), response_tag, text: None, data: None }
            )
        );
        let json = crate::messages::marshal::to_json_value(&resp);
        router.set_outbound_response(Some(json));

        if !msg.rippled {
            let mut rippled_msg = msg.clone();
            rippled_msg.rippled = true;
            let ripple_msg = Message::Ddb(DdbMessage::DeleteResolve(rippled_msg));
            let ripple_json = crate::messages::marshal::to_json_value(&ripple_msg);
            if let Some(backend) = router.get_ddb_backend() {
                if let Ok(b) = backend.lock() {
                    api.ripple_message(ripple_json, msg.start_id.clone(), &*b);
                }
            }
        }
    }

    fn on_ddb_query_resolve(&self, _api: Arc<dyn BingleApiBoth>, from: &FromStruct, q: &DdbQueryResolve) {
        let router = &from.router;
        if !router.get_am_relay() { return; }
        // Lookup
        let (found, advert_opt) = if let Some(backend) = router.get_ddb_backend() {
            if let Ok(b) = backend.lock() { let r = b.lookup(&q.id); (r.is_some(), r) } else { (false, None) }
        } else { (false, None) };
        let response_tag = q.tag.clone();
        let resp = crate::messages::types::Message::Ddb(
            crate::messages::types::DdbMessage::QueryResponse(
                crate::messages::types::DdbQueryResponse { app: "ddb".to_string(), found, advert: advert_opt, response_tag, text: None, data: None }
            )
        );
        let json = crate::messages::marshal::to_json_value(&resp);
        router.set_outbound_response(Some(json));
    }

    fn on_ddb_dump_resolve(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &DdbDumpResolve) {
        tracing::info!("[handlers::on_ddb_dump_resolve] upserting from {:?}", msg);
        api.ddb_upsert_record(msg.record.clone());
        if let Some(target) = api.get_peer_ddb_target() {
            if target == api.ddb_backend_size() {
                tracing::info!("[handlers::on_ddb_dump_resolve] DDB sync complete (all {} records received). Sending DdbSignon.", target);
                let my_id = match api.get_my_id() {
                    Some(id) => id,
                    None => {
                        tracing::warn!("[handlers::on_ddb_dump_resolve] get_my_id returned None; cannot send Signon");
                        return;
                    }
                };

                let signon = DdbSignon {
                    app: "ddb".to_string(),
                    start_id: my_id,
                    original_signature: None,
                    rippled: Some(false),
                    tag: None,
                    text: None,
                    data: None,
                };
                let msg_out = Message::Ddb(DdbMessage::Signon(signon));
                let json = crate::messages::marshal::to_json_value(&msg_out);

                let nsk = from.network_source_key.clone();
                let peer_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();

                let api_for_thread = api.clone();
                std::thread::spawn(move || {
                    tracing::info!("[on_ddb_dump_resolve] sending DdbSignon to {} via {}", peer_id, nsk);
                    let ok = api_for_thread.send_message_to_network(&nsk, &peer_id, json, None);
                    tracing::info!("[on_ddb_dump_resolve] DdbSignon sent ok={:?}", ok);
                });
            }
        }
    }

    fn on_ddb_signon_response(&self, api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &DdbSignonResponse) {
        tracing::info!("[on_ddb_signon_response] received SignonResponse, signaling completion");
        api.signal_signon_complete();
    }

    fn on_ddb_signon(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &DdbSignon) {
        let router = &from.router;
        if !router.get_am_relay() { return; }
        tracing::info!("[on_ddb_signon] received Signon from {}", msg.start_id);

        let sender_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        if msg.rippled != Some(true) && msg.start_id != sender_id {
            tracing::warn!("[on_ddb_signon] start_id mismatch: msg={} sender={}", msg.start_id, sender_id);
            return;
        }

        // Create and insert relay record for the new peer
        let endpoint = from.network_source_key.inet_socket_address().map(|addr| {
            InetSocketAddress {
                host: match addr.ip() {
                    std::net::IpAddr::V4(v4) => v4.to_string(),
                    std::net::IpAddr::V6(v6) => v6.to_string(),
                },
                port: addr.port(),
            }
        });

        let record = AdvertRecord {
            id: msg.start_id.clone(),
            endpoint,
            am_relay: Some(true),
            relay_id: None,
            relay_sig: None,
            date: "1970-01-01T00:00:00Z".to_string(),
            sig: msg.original_signature.clone(),
        };
        api.ddb_upsert_record(record);
        tracing::info!("[on_ddb_signon] signed on relay, relay count = {}", api.ddb_backend_size());

        // Queue SignonResponse FIRST — the new relay needs to receive this and
        // transition to Available before other relays learn about it and try
        // getEpoch on it.
        let resp = Message::Ddb(DdbMessage::SignonResponse(DdbSignonResponse {
            app: "ddb".to_string(),
            response_tag: msg.tag.clone(),
            text: None,
            data: None,
        }));
        let json = crate::messages::marshal::to_json_value(&resp);
        router.set_outbound_response(Some(json));

        // Ripple asynchronously with a delay so the new relay has time to
        // process SignonResponse and transition to Available state before
        // other relays discover it and try getEpoch on it.
        if msg.rippled != Some(true) {
            let mut rippled_msg = msg.clone();
            rippled_msg.rippled = Some(true);
            let ripple_msg = Message::Ddb(DdbMessage::Signon(rippled_msg));
            let ripple_json = crate::messages::marshal::to_json_value(&ripple_msg);
            let start_id = msg.start_id.clone();
            let api_clone = api.clone();
            let router_clone = from.router.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                tracing::info!("[on_ddb_signon] rippling signon for {} after delay", start_id);
                if let Some(backend) = router_clone.get_ddb_backend() {
                    if let Ok(b) = backend.lock() {
                        api_clone.ripple_message(ripple_json, start_id, &*b);
                    }
                }
            });
        }
    }

    fn on_triangle_test1(&self, api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &RelayTriangleTest1) {
        // Print options via API for debugging
        api.debug_print_options();
        tracing::info!("[handlers::on_triangle_test1] received {:?} from {}", msg, from.id);

        let msg_tag = msg.tag.clone();
        // Run in a thread per requirements
        let checking = msg.checking_endpoint.clone();
        let exclusions: Vec<std::net::SocketAddr> = msg.do_not_use_endpoints.iter()
            .cloned()
            .map(|ie| ie.try_into().expect("valid doNotUseEndpoints"))
            .collect();
        let api_for_thread = api.clone();
        // Clone sender context needed inside the spawned thread (avoid borrowing 'from')
        let from_nsk = from.network_source_key.clone();
        // Convert issuer-form id to raw Algorand address (base32) for network send
        let from_user_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
        let msg2 = msg.clone();
        std::thread::spawn(move || {
            // Proceed to construct a RelayFinder like in stun_consistent_process, using Indexer-based discovery when available.
            use std::time::Duration;
            use crate::relay::relay_finder::{RelayFinder, RelayInfo};
            tracing::info!("[handlers::on_triangle_test1] in thread for triangle test1: {:?}", msg2);
            let discover: std::sync::Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync> = {
                // Prefer app_id from API options; fallback to env var for legacy
                let app_id_opt = api_for_thread
                    .get_app_id()
                    .or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
                let app_id = app_id_opt.expect("on_triangle_test1: app_id is required (options.api or BINGLE_APP_ID)");
                let cfg = api_for_thread.get_algo_provider_config();
                crate::relay::discovery::indexer_discover_closure(app_id, cfg)
            };
            // Use the BingleApi instance passed to the handler (wrap combined API as plain BingleApiBoth)
            let api_plain: std::sync::Arc<dyn crate::api::bingle_api::BingleApiBoth> = std::sync::Arc::new(BothAsApi { inner: api_for_thread.clone() });
            let finder = RelayFinder::new(Arc::downgrade(&api_plain), Duration::from_secs(60), discover);

            // Obtain our id from API (derived from engine issuer)
            let my_id = match api_for_thread.get_my_id() {
                Some(id) => id,
                None => { warn!("[handlers::on_triangle_test1] get_my_id returned None"); return; }
            };
            tracing::info!("[handlers::on_triangle_test1] call find_relay_excluding my_id = {}", my_id);
            let associated_relay = match finder.find_relay_excluding(&my_id, &exclusions) {
                Ok(info) => info,
                Err(e) => {
                    warn!("[handlers::on_triangle_test1] find_relay failed: {}", e);
                    // No corner node available — send TriangleTest1Response with no_corner_node=true
                    let resp = Message::Relay(RelayMessage::TriangleTest1Response(RelayTriangleTest1Response { app: None, no_corner_node: true, response_tag: msg_tag }));
                    let resp_json = crate::messages::marshal::to_json_value(&resp);
                    if !from_user_id.is_empty() {
                        let ok = api_for_thread.send_message_to_network(&from_nsk, &from_user_id, resp_json, None);
                        tracing::info!("[handlers::on_triangle_test1] TriangleTest1Response (no_corner_node) sent ok={:?}", ok);
                    }
                    return;
                }
            };
            tracing::info!("[handlers::on_triangle_test1] found relay: {:?}", associated_relay);

            // Build TriangleTest2 with checking_endpoint from TriangleTest1 and checking_id as our id (no issuer suffix)
            let t2 = RelayTriangleTest2 { app: None, checking_id: my_id.clone(), checking_endpoint: checking };
            let msg_out = Message::Relay(RelayMessage::TriangleTest2(t2));
            let json_val = crate::messages::marshal::to_json_value(&msg_out);

            // Build NetworkSourceKey and user id base64(36) as required by API
            use crate::api::bingle_api::NetworkEndpoint;
            let nsk = NetworkEndpoint::new_direct(associated_relay.address);
            let user_id = associated_relay.id.clone();
            // Use the provided API for sending
            let ok = api_for_thread.send_message_to_network(&nsk, &user_id, json_val, None);
            tracing::info!("[handlers::on_triangle_test1] TriangleTest2 -> {} ok={:?}", associated_relay.address, ok);

            // After sending TriangleTest2 to the peer relay, send TriangleTest1Response back to the sender of TriangleTest1

            let resp = Message::Relay(RelayMessage::TriangleTest1Response(RelayTriangleTest1Response { app: None, no_corner_node: false, response_tag: msg_tag }));
            let resp_json = crate::messages::marshal::to_json_value(&resp);

            if from_user_id.is_empty() {
                warn!("[handlers::on_triangle_test1] Skipping TriangleTest1Response: invalid sender id");
            } else {
                let ok2 = api_for_thread.send_message_to_network(&from_nsk, &from_user_id, resp_json, None);
                tracing::info!("[handlers::on_triangle_test1] TriangleTest1Response sent ok={:?}", ok2);
            }

            tracing::info!("[handlers::on_triangle_test1] done for {:?}", msg2);
        });
    }

    fn on_triangle_test2(&self, api: Arc<dyn BingleApiBoth>, _from: &FromStruct, msg: &RelayTriangleTest2) {
        // On T2: send T3 to checking_endpoint (acts as peer relay behavior).
        use crate::api::bingle_api::NetworkEndpoint;
        use std::convert::TryInto;
        let endpoint: std::net::SocketAddr = msg.checking_endpoint.clone().try_into().expect("valid checkingEndpoint");
        let t3 = RelayTriangleTest3 { app: None };
        let out = Message::Relay(RelayMessage::TriangleTest3(t3));
        let json_val = crate::messages::marshal::to_json_value(&out);
        let nsk = NetworkEndpoint::new_direct(endpoint);
        // Convert checking_id (issuer) to raw address by trimming issuer suffix (base32 Algorand address)
        let user_id = msg.checking_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
        let ok = api.send_message_to_network(&nsk, &user_id, json_val, None);
        tracing::info!("[handlers::on_triangle_test2] TriangleTest3 -> {} ok={:?}", endpoint, ok);
    }

    fn on_triangle_test3(&self, api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &RelayTriangleTest3) {
        // Use the combined API (BingleApi + BingleApiInternal) instead of Router::get_bingle_api_internal
        api.set_state(crate::engine::EngineState::EndpointAvailable);
        api.set_nat_type(crate::engine::NatType::FullCone);
        // After setting NAT type, start a background thread to register our discovered public endpoint in DDB.
        let api_for_thread = api.clone();
        std::thread::spawn(move || {
            // Obtain the discovered public address (including port)
            if let Some(addr) = api_for_thread.get_last_public_addr() {
                // Register the IP as not relay first
                match api_for_thread.ddb_register_ip(addr, false) {
                    Ok(()) => {
                        tracing::info!("[handlers::on_triangle_test3] initial DDB registration successful: {}", addr);
                        if api_for_thread.is_relay() {
                            api_for_thread.initialize_relay();
                            if let Err(e) = api_for_thread.ddb_register_ip(addr, true) {
                                tracing::warn!("[handlers::on_triangle_test3] second ddb_register_ip(true) failed: {}", e);
                            } else {
                                tracing::info!("[handlers::on_triangle_test3] relay DDB registration successful: {}", addr);
                            }
                        }
                        // Mark engine state as Registered and print id/handle for debugging
                        let uid = api_for_thread.get_user_id().unwrap_or_else(|| "<unknown>".to_string());
                        let handle = api_for_thread.get_handle().unwrap_or_else(|| "<unknown>".to_string());
                        tracing::info!("[handlers::on_triangle_test3] registration process completed (user_id={}, handle={})", uid, handle);
                        api_for_thread.set_state(crate::engine::EngineState::Registered);
                        // Notify that we are listening now
                        api_for_thread.notify_listening(true, crate::engine::NatType::FullCone);
                    }
                    Err(e) => {
                        tracing::warn!("[handlers::on_triangle_test3] initial ddb_register_ip failed: {}", e);
                    }
                }
            } else {
                tracing::warn!("[handlers::on_triangle_test3] get_last_public_addr returned None; skipping DDB register");
            }
        });
    }

    fn on_triangle_test1_response(&self, api: Arc<dyn BingleApiBoth>, _from: &FromStruct, msg: &RelayTriangleTest1Response) {
        tracing::info!("[DefaultPrintingHandler] TriangleTest1Response received (no_corner_node={})", msg.no_corner_node);

        if msg.no_corner_node {
            // No corner node available — immediately set NATRestricted if appropriate, then
            // run the same post-triangle logic that the delayed thread would execute.
            warn!("[DefaultPrintingHandler][on_triangle_test1_response] no corner node available for triangle test");

            // Execute the post-triangle relay registration logic immediately in a thread (no delay)
            let api_for_thread = api.clone();
            std::thread::spawn(move || {
                Self::post_triangle_relay_register(api_for_thread);
            });
        } else {
            // Normal path: spawn a delayed thread to check state after 10 seconds
            let api_for_thread = api.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(10));
                Self::post_triangle_relay_register(api_for_thread);
            });
        }
    }

}

impl DefaultPrintingHandler {
    /// Shared logic executed after a triangle test response (either delayed or immediate).
    ///
    /// Sets NATRestricted if current state is neither EndpointAvailable nor Registered,
    /// then contacts the associated relay to start TURN Listen and registers in the DDB.
    fn post_triangle_relay_register(api_for_thread: Arc<dyn BingleApiBoth>) {
        let cur = api_for_thread.get_state();
        if cur != crate::engine::EngineState::EndpointAvailable && cur != crate::engine::EngineState::Registered {
            let _ = api_for_thread.set_state(crate::engine::EngineState::NATRestricted);
            api_for_thread.set_nat_type(crate::engine::NatType::Restricted);

            // After setting NAT type Restricted, contact our associated relay to start TURN Listen and register relay in DDB.
            use std::time::Duration;
            use crate::relay::relay_finder::{RelayFinder, RelayInfo};

            // Build discovery closure similar to on_triangle_test1
            let discover: std::sync::Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync> = {
                let app_id_opt = api_for_thread
                    .get_app_id()
                    .or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
                let app_id = match app_id_opt {
                    Some(v) => v,
                    None => { warn!("[on_triangle_test1_response] app_id missing; cannot discover relay"); return; }
                };
                let cfg = api_for_thread.get_algo_provider_config();
                crate::relay::discovery::indexer_discover_closure(app_id, cfg)
            };

            // Wrap combined API as plain BingleApiBoth for RelayFinder
            let api_plain: std::sync::Arc<dyn crate::api::bingle_api::BingleApiBoth> = std::sync::Arc::new(BothAsApi { inner: api_for_thread.clone() });
            let finder = RelayFinder::new(Arc::downgrade(&api_plain), Duration::from_secs(60), discover);
            let my_id = match api_for_thread.get_my_id() {
                Some(id) => id,
                None => { warn!("[on_triangle_test1_response] get_my_id returned None"); return; }
            };
            let relay_info = match finder.find_relay(&my_id) {
                Ok(info) => info,
                Err(e) => { warn!("[on_triangle_test1_response] find_relay failed: {}", e); return; }
            };

            // Send Relay::Listen and expect Relay::ListenResponse
            let listen = crate::messages::types::RelayListen { app: None, tag: None };
            let msg = crate::messages::types::Message::Relay(crate::messages::types::RelayMessage::Listen(listen));
            let json = crate::messages::marshal::to_json_value(&msg);
            let nsk = crate::api::bingle_api::NetworkEndpoint::new_direct(relay_info.address);
            let uid = relay_info.id.clone();
            match api_for_thread.send_message_to_network_with_response(&nsk, &uid, json, None) {
                Ok(resp) => {
                    let ty_ok = resp.get("type").and_then(|v| v.as_str()) == Some("ListenResponse");
                    if !ty_ok { warn!("[on_triangle_test1_response] unexpected response to Listen: {}", resp); return; }

                    // Register the relay listener mapping via the internal API (engine turn_handler)
                    tracing::info!("[on_triangle_test1_response] Relay ListenResponse {:?} received; registering relay listener", resp);
                    api_for_thread.turn_client_handle_listen_response(relay_info.address, relay_info.id.clone());
                }
                Err(e) => { warn!("[on_triangle_test1_response] Listen request failed: {}", e); return; }
            }

            // Register relay association in DDB and mark registered
            if let Err(e) = api_for_thread.ddb_register_relay(relay_info.id.clone(), None) {
                warn!("[on_triangle_test1_response] ddb_register_relay failed: {}", e);
            } else {
                tracing::info!("[on_triangle_test1_response] ddb_register_relay succeeded for relay_id={}", relay_info.id);
                api_for_thread.set_state(crate::engine::EngineState::Registered);
                // Notify that we are listening now
                api_for_thread.notify_listening(true, crate::engine::NatType::Restricted)
            }
        } else {
            tracing::info!("[on_triangle_test1_response] ignoring due to state={:?}", cur);
        }
    }
}
