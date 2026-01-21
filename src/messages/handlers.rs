use crate::api::bingle_api::BingleApi;
use crate::ddb::DdbBackend;
use crate::messages::types::*;
use log::warn;
use crate::turn::turn_handler::TurnHandler;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct FromStruct {
    pub id: String,
    pub network_source_key: crate::api::bingle_api::NetworkEndpoint,
}

pub trait MessageHandler {
    // Plain text
    fn on_plain_text(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, msg: &PlainTextMessage) {
        // Build JSON for callback
        let json = serde_json::to_value(msg).unwrap_or_else(|_| serde_json::json!({"text": msg.text}));
        // Delegate to API on_message via the per-API Router if installed
        if let Some(router) = crate::messages::router::Router::current() {
            if let Some(cb) = router.get_on_message() {
                // Normalize sender id: issuer without suffix
                let sender_id = _from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
                // Use direct socket address as sender_handle when available
                let sender_handle = _from
                    .network_source_key
                    .inet_socket_address()
                    .map(|a: std::net::SocketAddr| a.to_string())
                    .unwrap_or_else(|| "".to_string());
                cb(sender_id, sender_handle, json);
                return;
            }
        }
        // Fallback to logging if no on_message callback is installed
        log::info!("[MessageHandler::on_plain_text][default] {}", serde_json::to_string(&json).unwrap_or_else(|_| "<unprintable>".into()));
    }

    // Ping messages
    fn on_ping_ping(&self, api: Arc<dyn BingleApi>, from: &FromStruct, msg: &PingPing) {
        log::info!("[on_ping_ping] handling ping {:?} from {:?}", msg, from);
        // Reply with PingResponse: app="ping", type="response", verifiedId from API, text="ACK: {text}"
        if let Some(router) = crate::messages::router::Router::current() {
            let sender_opt = router.get_sender();
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
            if let Some(tag) = router.get_last_response_tag() {
                json_obj.insert("responseTag".to_string(), serde_json::Value::String(tag));
            }
            let ack_text = format!("ACK: {}", msg.text.clone().unwrap_or_default());
            json_obj.insert("text".to_string(), serde_json::Value::String(ack_text));
            let json_val = serde_json::Value::Object(json_obj);

            // Prepare destination (use from.id (issuer) as base32 algorand address without conversion)
            let nsk = from.network_source_key.clone();
            let user_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
            log::info!("[handlers::on_ping_ping] sending response {:?}", json_val);
            let ok = sender(&nsk, &user_id, json_val);
            if !ok {
                log::warn!("[handlers::on_ping_ping] sender returned false");
            }
        }
    }
    fn on_ping_response(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &PingResponse) { self.on_unimplemented(&Message::Ping(PingMessage::Response(_msg.clone()))); }

    // Relay messages
    fn on_relay_call(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayCall) {
        use crate::turn::turn_handler::TurnRelayHandler;
        if let Some(router) = crate::messages::router::Router::current() {
            if !router.get_am_relay() {
                warn!("[handlers::on_relay_call] Not a relay: ignoring Call");
                return;
            }
            let src = match router.get_last_from() {
                Some(a) => a,
                None => { warn!("[handlers::on_relay_call] No source address available"); return; }
            };
            let turn = match router.get_turn_handler() {
                Some(h) => h,
                None => { warn!("[handlers::on_relay_call] No TURN handler available"); return; }
            };
            // Resolve destination address by id recorded via Listen
            let called_id_raw = _msg.called_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
            let dest = match turn.lookup_addr_by_id(&called_id_raw) {
                Some(a) => a,
                None => { warn!("[handlers::on_relay_call] called id not registered: {}", called_id_raw); return; }
            };
            let ch = TurnRelayHandler::handle_call(&*turn, &src, &dest);
            if ch < 0 { warn!("[handlers::on_relay_call] handle_call failed"); return; }
            // Build RelayResponse { app: null, channel }
            let mut obj = serde_json::Map::new();
            obj.insert("app".to_string(), serde_json::Value::Null);
            obj.insert("type".to_string(), serde_json::Value::String("RelayResponse".to_string()));
            obj.insert("channel".to_string(), serde_json::Value::Number(serde_json::Number::from(ch as u64)));
            if let Some(tag) = router.get_last_response_tag() { obj.insert("responseTag".to_string(), serde_json::Value::String(tag)); }
            router.set_outbound_response(Some(serde_json::Value::Object(obj)));
        } else {
            warn!("[handlers::on_relay_call] No router context available");
        }
    }
    fn on_relay_response(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::RelayResponse(_msg.clone()))); }
    fn on_triangle_test1(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest1) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest1(_msg.clone()))); }
    fn on_triangle_test2(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest2) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest2(_msg.clone()))); }
    fn on_triangle_test3(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest3) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest3(_msg.clone()))); }
    fn on_triangle_test1_response(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest1Response) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest1Response(_msg.clone()))); }
    fn on_relay_listen(&self, _api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayListen) {
        // Only process on relay nodes
        if let Some(router) = crate::messages::router::Router::current() {
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
            // Turn handler must be provided via Router by the Engine
            let turn = match router.get_turn_handler() {
                Some(h) => h,
                None => {
                    warn!("[handlers::on_relay_listen] No TURN handler available");
                    return;
                }
            };
            let source_id = _from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
            let _ok = turn.handle_listen(&source_id, &src);
            // Build and stash a ListenResponse; include responseTag if present
            let mut obj = serde_json::Map::new();
            obj.insert("app".to_string(), serde_json::Value::Null);
            obj.insert("type".to_string(), serde_json::Value::String("ListenResponse".to_string()));
            if let Some(tag) = router.get_last_response_tag() {
                obj.insert("responseTag".to_string(), serde_json::Value::String(tag));
            }
            router.set_outbound_response(Some(serde_json::Value::Object(obj)));
        } else {
            warn!("[handlers::on_relay_listen] No router context available");
        }
    }
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
        // Convert from.id (issuer) to raw Algorand address (base32)
        let user_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
        let _ok = sender(&nsk, &user_id, json_val);
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
        // Convert issuer-form id to raw Algorand address (base32) for network send
        let from_user_id = from.id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
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
            use crate::api::bingle_api::NetworkEndpoint;
            let nsk = NetworkEndpoint::new_direct(associated_relay.address);
            let user_id = associated_relay.id.clone();
            // Use the provided API for sending
            let ok = api_for_thread.send_message_to_network(&nsk, &user_id, json_val, None);
            log::info!("[handlers::on_triangle_test1] TriangleTest2 -> {} ok={}", associated_relay.address, ok);

            // After sending TriangleTest2 to the peer relay, send TriangleTest1Response back to the sender of TriangleTest1

            let resp = Message::Relay(RelayMessage::TriangleTest1Response(RelayTriangleTest1Response { app: None }));
            let resp_json = crate::messages::marshal::to_json_value(&resp);

            if from_user_id.is_empty() {
                warn!("[handlers::on_triangle_test1] Skipping TriangleTest1Response: invalid sender id");
            } else {
                let ok2 = api_for_thread.send_message_to_network(&from_nsk, &from_user_id, resp_json, None);
                log::info!("[handlers::on_triangle_test1] TriangleTest1Response sent ok={}", ok2);
            }
        });
    }

    fn on_triangle_test2(&self, api: Arc<dyn BingleApi>, _from: &FromStruct, msg: &RelayTriangleTest2) {
        // On T2: send T3 to checking_endpoint (acts as peer relay behavior).
        use crate::api::bingle_api::NetworkEndpoint;
        let endpoint = msg.checking_endpoint;
        let t3 = RelayTriangleTest3 { app: None };
        let out = Message::Relay(RelayMessage::TriangleTest3(t3));
        let json_val = crate::messages::marshal::to_json_value(&out);
        let nsk = NetworkEndpoint::new_direct(endpoint);
        // Convert checking_id (issuer) to raw address by trimming issuer suffix (base32 Algorand address)
        let user_id = msg.checking_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
        let ok = api.send_message_to_network(&nsk, &user_id, json_val, None);
        log::info!("[handlers::on_triangle_test2] TriangleTest3 -> {} ok={}", endpoint, ok);
    }

    fn on_triangle_test3(&self, api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest3) {
        // Use internal API to set engine state to EndpointAvailable and NAT type to FullCone.
        if let Some(internal) = crate::messages::router::Router::current().and_then(|r| r.get_bingle_api_internal()) {
            internal.set_state(crate::engine::EngineState::EndpointAvailable);
            internal.set_nat_type(crate::engine::NatType::FullCone);
            // After setting NAT type, start a background thread to register our discovered public endpoint in DDB.
            let internal_clone = internal.clone();
            let api_for_log = api.clone();
            std::thread::spawn(move || {
                // Obtain the discovered public address (including port)
                if let Some(addr) = internal_clone.get_last_public_addr() {
                    match internal_clone.ddb_register_ip(addr) {
                        Ok(()) => {
                            // On success, mark engine state as Registered and print id/handle for debugging
                            let uid = api_for_log.get_user_id().unwrap_or_else(|| "<unknown>".to_string());
                            let handle = api_for_log.get_handle().unwrap_or_else(|| "<unknown>".to_string());
                            log::info!("[handlers::on_triangle_test3] ddb_register_ip succeeded (user_id={}, handle={})", uid, handle);
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

    fn on_triangle_test1_response(&self, api: Arc<dyn BingleApi>, _from: &FromStruct, _msg: &RelayTriangleTest1Response) {
        log::info!("[DefaultPrintingHandler] TriangleTest1Response received");

        // Move all logic to a spawned thread with delay
        let api_for_thread = api.clone();
        std::thread::spawn(move || {
            // Delay for 10 seconds before making the state check
            std::thread::sleep(std::time::Duration::from_secs(10));

            let internal_opt = crate::messages::router::Router::current().and_then(|r| r.get_bingle_api_internal());
            if let Some(internal) = internal_opt.clone() {
                // Only set state/nat_type if current state is neither EndpointAvailable nor Registered
                let cur = internal.get_state();
                if cur != crate::engine::EngineState::EndpointAvailable && cur != crate::engine::EngineState::Registered {
                    let _ = internal.set_state(crate::engine::EngineState::NATRestricted);
                    internal.set_nat_type(crate::engine::NatType::Restricted);

                    // After setting NAT type Restricted, contact our associated relay to start TURN Listen and register relay in DDB.
                    use std::time::Duration;
                    use crate::relay::relay_finder::{RelayFinder, RootRelayInfo};

                    // Build discovery closure similar to on_triangle_test1
                    let discover: std::sync::Arc<dyn Fn() -> Vec<RootRelayInfo> + Send + Sync> = {
                        #[cfg(not(target_os = "ios"))]
                        {
                            let app_id_opt = api_for_thread
                                .get_app_id()
                                .or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
                            let app_id = match app_id_opt {
                                Some(v) => v,
                                None => { warn!("[on_triangle_test1_response] app_id missing; cannot discover relay"); return; }
                            };
                            let cfg = api_for_thread.get_algo_provider_config();
                            crate::relay::discovery::indexer_discover_closure(app_id, cfg)
                        }
                        #[cfg(target_os = "ios")]
                        {
                            let _ = api_for_thread
                                .get_app_id()
                                .or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()))
                                .expect("on_triangle_test1_response (iOS): app_id is required");
                            std::sync::Arc::new(|| panic!("on_triangle_test1_response (iOS): discovery not supported without indexer"))
                        }
                    };

                    let finder = RelayFinder::new(api.clone(), Duration::from_secs(60), discover);
                    let my_id = match api.get_my_id() {
                        Some(id) => id,
                        None => { warn!("[on_triangle_test1_response] get_my_id returned None"); return; }
                    };
                    let relay_info = match finder.find_relay(&my_id) {
                        Ok(info) => info,
                        Err(e) => { warn!("[on_triangle_test1_response] find_relay failed: {}", e); return; }
                    };

                    // Send Relay::Listen and expect Relay::ListenResponse
                    let listen = crate::messages::types::RelayListen { app: None };
                    let msg = crate::messages::types::Message::Relay(crate::messages::types::RelayMessage::Listen(listen));
                    let json = crate::messages::marshal::to_json_value(&msg);
                    let nsk = crate::api::bingle_api::NetworkEndpoint::new_direct(relay_info.address);
                    let uid = relay_info.id.clone();
                    match api.send_message_to_network_with_response(&nsk, &uid, json, None) {
                        Ok(resp) => {
                            let ty_ok = resp.get("type").and_then(|v| v.as_str()) == Some("ListenResponse");
                            if !ty_ok { warn!("[on_triangle_test1_response] unexpected response to Listen: {}", resp); return; }
                        }
                        Err(e) => { warn!("[on_triangle_test1_response] Listen request failed: {}", e); return; }
                    }

                    // Register relay association in DDB
                    if let Some(internal) = internal_opt {
                        if let Err(e) = internal.ddb_register_relay(relay_info.id.clone(), None) {
                            warn!("[on_triangle_test1_response] ddb_register_relay failed: {}", e);
                        } else {
                            log::info!("[on_triangle_test1_response] ddb_register_relay succeeded for relay_id={}", relay_info.id);
                            internal.set_state(crate::engine::EngineState::Registered)
                        }
                    }

                } else {
                    log::info!("[on_triangle_test1_response] ignoring due to state={:?}", cur);
                }
            } else {
                warn!("[handlers::on_triangle_test1_response] No internal API available; cannot set state");
            }
        });
    }
}
