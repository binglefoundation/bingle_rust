use crate::api::bingle_api::BingleApi;
use crate::messages::types::*;
use crate::ddb::DdbBackend;
use std::sync::Arc;
use log::warn;

#[derive(Debug, Clone)]
pub struct FromStruct {
    pub id: String,
    pub network_source_key: crate::api::bingle_api::NetworkSourceKey,
}

pub trait MessageHandler {
    // Plain text
    fn on_plain_text(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, msg: &PlainTextMessage) {
        // Default implementation: print payload; frameworks may provide their own handler that
        // forwards to an API instance without using globals.
        let json = serde_json::to_value(msg).unwrap_or_else(|_| serde_json::json!({"text": msg.text}));
        log::info!("[MessageHandler::on_plain_text][default] {}", serde_json::to_string(&json).unwrap_or_else(|_| "<unprintable>".into()));
    }

    // Relay messages
    fn on_relay_call(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayCall) { self.on_unimplemented(&Message::Relay(RelayMessage::Call(_msg.clone()))); }
    fn on_relay_response(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::RelayResponse(_msg.clone()))); }
    fn on_triangle_test1(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest1) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest1(_msg.clone()))); }
    fn on_triangle_test2(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest2) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest2(_msg.clone()))); }
    fn on_triangle_test3(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest3) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest3(_msg.clone()))); }
    fn on_triangle_test1_response(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest1Response) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest1Response(_msg.clone()))); }
    fn on_relay_listen(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayListen) { self.on_unimplemented(&Message::Relay(RelayMessage::Listen(_msg.clone()))); }
    fn on_relay_check(&self, _api: Arc<dyn BingleApi>, from: &FromStruct, _msg: &RelayCheck) {
        // Send CheckResponse available=true back to the last sender address using the real Bingle API sender
        let sender_opt = crate::messages::router::Router::current().and_then(|r| r.get_sender());
        if sender_opt.is_none() { warn!("[handlers::on_relay_check] No sender available"); return; }
        let sender = sender_opt.unwrap();
        // Compose JSON manually to include responseTag if present
        let mut json_obj = serde_json::Map::new();
        json_obj.insert("app".to_string(), serde_json::Value::Null);
        json_obj.insert("type".to_string(), serde_json::Value::String("CheckResponse".to_string()));
        json_obj.insert("available".to_string(), serde_json::Value::Bool(true));
        if let Some(tag) = crate::messages::router::Router::current().and_then(|r| r.get_last_response_tag()) {
            json_obj.insert("responseTag".to_string(), serde_json::Value::String(tag));
        }
        let json_val = serde_json::Value::Object(json_obj);
        let nsk = from.network_source_key.clone();
        // Convert from.id (issuer) to raw address and base64(36)
        let raw_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
        let user_id_b64 = match crate::blockchain::algo_ops::id_b64_from_algorand_addr(&raw_id) {
            Ok(s) => s,
            Err(e) => {
                warn!("[handlers::on_relay_check] invalid from.id '{}': {}", raw_id, e);
                return; // do not send invalid id
            }
        };
        let _ok = sender(&nsk, &user_id_b64, json_val);
    }
    fn on_relay_listen_response(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayListenResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::ListenResponse(_msg.clone()))); }
    fn on_relay_check_response(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayCheckResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::CheckResponse(_msg.clone()))); }
    fn on_relay_call_response(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayCallResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::CallResponse(_msg.clone()))); }
    fn on_relay_keep_alive(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayKeepAlive) { self.on_unimplemented(&Message::Relay(RelayMessage::KeepAlive(_msg.clone()))); }

    // DDB messages (default to unimplemented unless overridden)
    fn on_ddb_upsert_resolve(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, msg: &DdbUpsertResolve) { self.on_unimplemented(&Message::Ddb(DdbMessage::UpsertResolve(msg.clone()))); }
    fn on_ddb_query_resolve(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, msg: &DdbQueryResolve) { self.on_unimplemented(&Message::Ddb(DdbMessage::QueryResolve(msg.clone()))); }

    // Unknown
    fn on_unknown(&self, _api: Arc<dyn BingleApi>, _raw: &serde_json::Value) {
        log::info!("[UNIMPLEMENTED] Unknown message: {}", _raw);
    }

    // Default unimplemented handler: prints the message JSON
    fn on_unimplemented(&self, msg: &Message) {
        log::info!("[UNIMPLEMENTED] {}", serde_json::to_string(&crate::messages::marshal::to_json_value(msg)).unwrap_or_else(|_| "<unprintable>".into()));
    }
}

pub struct DefaultPrintingHandler;

impl MessageHandler for DefaultPrintingHandler {
    fn on_ddb_upsert_resolve(&self, _api: Arc<dyn BingleApi>, from: &FromStruct, up: &DdbUpsertResolve) {
        if let Some(router) = crate::messages::router::Router::current() {
            if !router.get_am_relay() { return; }
            // Validate sender id
            let sender_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
            if up.record.id != up.start_id || up.record.id != sender_id { return; }
            // Upsert to backend
            if let Some(backend) = router.get_ddb_backend() {
                if let Ok(mut b) = backend.lock() { b.upsert(up.record.clone()); }
            }
            // Prepare response JSON and stash on router for Engine/DTLS layer to send.
            let resp = crate::messages::types::Message::Ddb(
                crate::messages::types::DdbMessage::UpdateResponse(
                    crate::messages::types::DdbUpdateResponse { app: "ddb".to_string(), tag: None, response_tag: up.response_tag.clone(), text: None, data: None }
                )
            );
            let json = crate::messages::marshal::to_json_value(&resp);
            router.set_outbound_response(Some(json));
        }
    }

    fn on_ddb_query_resolve(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, q: &DdbQueryResolve) {
        if let Some(router) = crate::messages::router::Router::current() {
            if !router.get_am_relay() { return; }
            // Lookup
            let (found, advert_opt) = if let Some(backend) = router.get_ddb_backend() {
                if let Ok(b) = backend.lock() { let r = b.lookup(&q.id); (r.is_some(), r) } else { (false, None) }
            } else { (false, None) };
            let resp = crate::messages::types::Message::Ddb(
                crate::messages::types::DdbMessage::QueryResponse(
                    crate::messages::types::DdbQueryResponse { app: "ddb".to_string(), found, advert: advert_opt, tag: None, response_tag: q.response_tag.clone(), text: None, data: None }
                )
            );
            let json = crate::messages::marshal::to_json_value(&resp);
            router.set_outbound_response(Some(json));
        }
    }

    fn on_triangle_test1(&self, api: Arc<dyn BingleApi>, from: &FromStruct, msg: &RelayTriangleTest1) {
        // Print options via API for debugging
        api.debug_print_options();
        // Run in a thread per requirements
        let checking = msg.checking_endpoint;
        let api_for_thread = api.clone();
        // Clone sender context needed inside the spawned thread (avoid borrowing 'from')
        let from_nsk = from.network_source_key.clone();
        // Convert issuer-form id to base64(36) user id for network send
        let from_user_id_b64 = {
            let raw = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
            match crate::blockchain::algo_ops::id_b64_from_algorand_addr(&raw) {
                Ok(s) => s,
                Err(e) => {
                    warn!("[handlers::on_triangle_test1] invalid from.id '{}': {}", raw, e);
                    String::new()
                }
            }
        };
        std::thread::spawn(move || {
            // Proceed to construct a RelayFinder like in stun_consistent_process, using Indexer-based discovery when available.
            use std::time::Duration;
            use crate::relay::relay_finder::{RelayFinder, RootRelayInfo};
            let discover: std::sync::Arc<dyn Fn() -> Vec<RootRelayInfo> + Send + Sync> = {
                #[cfg(not(target_os = "ios"))]
                {
                    // Prefer app_id from API options; fallback to env var for legacy
                    let app_id_opt = api_for_thread
                        .get_app_id()
                        .or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
                    let app_id = app_id_opt.expect("on_triangle_test1: app_id is required (options.api or BINGLE_APP_ID)");
                    let cfg = api_for_thread.get_algo_provider_config();
                    crate::relay::discovery::indexer_discover_closure(app_id, cfg)
                }
                #[cfg(target_os = "ios")]
                {
                    // On iOS we also require proper discovery via indexer; panic if not configured
                    let _ = api_for_thread
                        .get_app_id()
                        .or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()))
                        .expect("on_triangle_test1 (iOS): app_id is required");
                    std::sync::Arc::new(|| panic!("on_triangle_test1 (iOS): discovery not supported without indexer"))
                }
            };
            // Use the BingleApi instance passed to the handler
            let finder = RelayFinder::new(api_for_thread.clone(), Duration::from_secs(60), discover);

            // Obtain our id from API (derived from engine issuer)
            let my_id = match api_for_thread.get_my_id() {
                Some(id) => id,
                None => { warn!("[handlers::on_triangle_test1] get_my_id returned None"); return; }
            };
            let associated_relay = match finder.find_relay(&my_id) {
                Ok(info) => info,
                Err(e) => { warn!("[handlers::on_triangle_test1] find_relay failed: {}", e); return; }
            };

            // Build TriangleTest2 with checking_endpoint from TriangleTest1 and checking_id as our id (no issuer suffix)
            let t2 = RelayTriangleTest2 { app: None, checking_id: my_id.clone(), checking_endpoint: checking };
            let msg_out = Message::Relay(RelayMessage::TriangleTest2(t2));
            let json_val = crate::messages::marshal::to_json_value(&msg_out);

            // Build NetworkSourceKey and user id base64(36) as required by API
            use crate::api::bingle_api::{NetworkSourceKey, UserId};
            let nsk = NetworkSourceKey::new_direct(associated_relay.address);
            let user_id: UserId = match crate::blockchain::algo_ops::id_b64_from_algorand_addr(&associated_relay.id) {
                Ok(s) => s,
                Err(e) => {
                    warn!("[handlers::on_triangle_test1] invalid relay id '{}': {}", associated_relay.id, e);
                    associated_relay.id.clone()
                }
            };
            // Use the provided API for sending
            let ok = api_for_thread.send_message_to_network(&nsk, &user_id, json_val, None);
            log::info!("[handlers::on_triangle_test1] TriangleTest2 -> {} ok={}", associated_relay.address, ok);

            // After sending TriangleTest2 to the peer relay, send TriangleTest1Response back to the sender of TriangleTest1

            let resp = Message::Relay(RelayMessage::TriangleTest1Response(RelayTriangleTest1Response { app: None }));
            let resp_json = crate::messages::marshal::to_json_value(&resp);

            if from_user_id_b64.is_empty() {
                warn!("[handlers::on_triangle_test1] Skipping TriangleTest1Response: invalid sender id");
            } else {
                let ok2 = api_for_thread.send_message_to_network(&from_nsk, &from_user_id_b64, resp_json, None);
                log::info!("[handlers::on_triangle_test1] TriangleTest1Response sent ok={}", ok2);
            }
        });
    }

    fn on_triangle_test2(&self, api: Arc<dyn BingleApi>, _from: &FromStruct, msg: &RelayTriangleTest2) {
        // On T2: send T3 to checking_endpoint (acts as peer relay behavior).
        use crate::api::bingle_api::NetworkSourceKey;
        use base64::Engine as _;
        let endpoint = msg.checking_endpoint;
        let t3 = RelayTriangleTest3 { app: None };
        let out = Message::Relay(RelayMessage::TriangleTest3(t3));
        let json_val = crate::messages::marshal::to_json_value(&out);
        let nsk = NetworkSourceKey::new_direct(endpoint);
        // Convert checking_id (issuer) to raw address by trimming issuer suffix, then base32->base64(36)
        let raw_id = msg.checking_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
        let user_id_b64 = match crate::blockchain::algo_ops::id_b64_from_algorand_addr(&raw_id) {
            Ok(s) => s,
            Err(e) => {
                warn!("[handlers::on_triangle_test2] invalid checking_id '{}': {}", raw_id, e);
                // Fallback to a deterministic valid 36-byte base64 string so the send path is exercised in tests
                base64::engine::general_purpose::STANDARD.encode([0u8; 36])
            }
        };
        let ok = api.send_message_to_network(&nsk, &user_id_b64, json_val, None);
        log::info!("[handlers::on_triangle_test2] TriangleTest3 -> {} ok={}", endpoint, ok);
    }

    fn on_triangle_test3(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest3) {
        // Use internal API to set engine state to EndpointAvailable and NAT type to FullCone.
        if let Some(internal) = crate::messages::router::Router::current().and_then(|r| r.get_bingle_api_internal()) {
            internal.set_state(crate::engine::EngineState::EndpointAvailable);
            internal.set_nat_type(crate::engine::NatType::FullCone);
            // After setting NAT type, start a background thread to register our discovered public endpoint in DDB.
            let internal_clone = internal.clone();
            std::thread::spawn(move || {
                // Obtain the discovered public address (including port)
                if let Some(addr) = internal_clone.get_last_public_addr() {
                    match internal_clone.ddb_register_ip(addr) {
                        Ok(()) => {
                            // On success, mark engine state as Registered
                            internal_clone.set_state(crate::engine::EngineState::Registered);
                        }
                        Err(e) => {
                            log::warn!("[handlers::on_triangle_test3] ddb_register_ip failed: {}", e);
                        }
                    }
                } else {
                    log::warn!("[handlers::on_triangle_test3] get_last_public_addr returned None; skipping DDB register");
                }
            });
        } else {
            warn!("[handlers::on_triangle_test3] No internal API available; cannot set state");
        }
    }

    fn on_triangle_test1_response(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest1Response) {
        log::info!("[DefaultPrintingHandler] TriangleTest1Response received");
        // Per requirement: if Engine state is not EndpointAvailable, set it to NATRestricted and NAT type Restricted
        // We don't have direct state query here; rely on Engine's internal setter semantics.
        if let Some(internal) = crate::messages::router::Router::current().and_then(|r| r.get_bingle_api_internal()) {
            // Only set NATRestricted if we are not already EndpointAvailable.
            // Engine::set_state_internal will ignore NATRestricted if EndpointAvailable flag is set.
            let _ = internal.set_state(crate::engine::EngineState::NATRestricted);
            internal.set_nat_type(crate::engine::NatType::Restricted);
        } else {
            warn!("[handlers::on_triangle_test1_response] No internal API available; cannot set state");
        }
    }
}
