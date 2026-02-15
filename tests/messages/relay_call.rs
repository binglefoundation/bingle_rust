use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use rust_comms::messages::{Message, RelayMessage};
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::types::RelayCall;
use rust_comms::turn::turn_handler::TurnHandler;
use rust_comms::api::bingle_api::{BingleApi, StartOptions, NetworkEndpoint, UserId, Handle, ProgressCallback, OnMessageHandler, OnConnectHandler};
use crate::util::mock_api::MockApiBoth;

// Minimal API stub
struct MockApi;
impl BingleApi for MockApi { 
    fn set_on_listening(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {} 
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None } 
    fn get_handle(&self) -> Option<String> { None } 
    fn get_user_id(&self) -> Option<String> { None }
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[test]
fn relay_call_allocates_channel_and_maps_pair() {
    // Arrange: router as relay with TURN handler and two registered peers
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(crate::util::mock_api::to_weak(MockApiBoth::new())));
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());
    struct MockInternal { pub turn: std::sync::Arc<rust_comms::turn::turn_handler::TurnHandlerImpl> }
    impl rust_comms::api::bingle_api::BingleApiInternal for MockInternal {
        fn get_relay_state(&self) -> String { "off".to_string() }
        fn set_state(&self, _state: rust_comms::engine::EngineState) {}
        fn get_state(&self) -> rust_comms::engine::EngineState { rust_comms::engine::EngineState::StunIdentify }
        fn set_nat_type(&self, _nat: rust_comms::engine::NatType) {}
        fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { None }
        fn ddb_register_ip(&self, _endpoint: std::net::SocketAddr) -> Result<(), String> { Ok(()) }
        fn ddb_register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> { Ok(()) }
        fn update_turn_listener_relay(&self, _relay_id: String, _relay_addr: std::net::SocketAddr) -> Result<(), String> { Ok(()) }
        fn turn_client_handle_listen_response(&self, _relay_addr: std::net::SocketAddr, _relay_id: String) { }
        fn turn_lookup_addr_by_id(&self, id: std::string::String) -> Option<std::net::SocketAddr> { self.turn.lookup_addr_by_id(&id) }
        fn turn_handle_call(&self, source: std::net::SocketAddr, dest: std::net::SocketAddr) -> i32 { rust_comms::turn::turn_handler::TurnRelayHandler::handle_call(&*self.turn, &source, &dest) }
        fn turn_handle_listen(&self, id: std::string::String, source: std::net::SocketAddr) -> bool { use rust_comms::turn::turn_handler::TurnHandler; self.turn.handle_listen(&id, &source) }
        fn turn_handle_called(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr, _channel: u16) { }
        fn notify_listening(&self, _listening: bool) { }
    }
    // router.set_bingle_api_internal(Some(std::sync::Arc::new(MockInternal { turn: turn.clone() }) as std::sync::Arc<dyn rust_comms::api::bingle_api::BingleApiInternal>));
    router.set_am_relay(true);

    let caller = addr(9101);
    let callee = addr(9102);
    // Register both via handle_listen
    assert!(turn.handle_listen("CALLERID", &caller));
    assert!(turn.handle_listen("CALLEEID", &callee));

    // Source of message is caller
    router.set_last_from(Some(caller));

    // Install a sender to capture RelayCalled sent to callee
    let captured: Arc<Mutex<Option<(NetworkEndpoint, String, serde_json::Value)>>> = Arc::new(Mutex::new(None));
    let captured_clone = captured.clone();
    router.set_sender(Some(Arc::new(move |nsk: &NetworkEndpoint, uid: &UserId, json: serde_json::Value| {
        *captured_clone.lock().unwrap() = Some((nsk.clone(), uid.to_string(), json.clone()));
        true
    })));

    // Act: send Relay::Call(calledId = CALLEEID)
    let handler = DefaultPrintingHandler;
    let call = Message::Relay(RelayMessage::Call(RelayCall { app: None, called_id: "CALLEEID".to_string() }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &call, "CALLERID");
    });

    // Assert: RelayResponse with channel
    let out = router.take_outbound_response();
    assert!(out.is_some(), "expected a RelayResponse");
    let obj = out.unwrap();
    let t = obj.get("type").and_then(|v: &serde_json::Value| v.as_str());
    assert_eq!(t, Some("RelayResponse"));
    let ch = obj.get("channel").and_then(|v: &serde_json::Value| v.as_u64()).expect("channel");
    let ch_u16 = ch as u16;

    // Verify RelayCalled was sent to callee with the same channel
    let sent = captured.lock().unwrap().clone().expect("RelayCalled should be sent to callee");
    let (nsk_sent, user_id_sent, json_sent) = sent;
    assert_eq!(nsk_sent, NetworkEndpoint::new_direct(callee));
    assert_eq!(user_id_sent, "CALLEEID");
    assert_eq!(json_sent.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("RelayCalled"));
    assert_eq!(json_sent.get("channel").and_then(|v: &serde_json::Value| v.as_u64()), Some(ch));

    // Verify internal mappings reflect (caller, callee) -> ch and ch -> caller
    let mapped_dest = turn.lookup_addr_by_channel_for_tests(ch_u16).expect("ch->addr");
    assert_eq!(mapped_dest, (caller, callee));
    let p2c = turn.lookup_channel_for_pair_for_tests(&caller, &callee).expect("pair->ch");
    assert_eq!(p2c, ch_u16);
}
