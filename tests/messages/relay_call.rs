use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rust_comms::messages::{Message, RelayMessage};
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::types::{RelayCall};
use rust_comms::turn::turn_handler::TurnHandler;
use rust_comms::api::bingle_api::{BingleApi, StartOptions, NetworkEndpoint, UserId, Handle, ProgressCallback, OnMessageHandler, OnConnectHandler};

// Minimal API stub
struct MockApi;
impl BingleApi for MockApi {
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
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(Arc::new(MockApi)));
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());
    router.set_turn_handler(Some(turn.clone()));
    router.set_am_relay(true);

    let caller = addr(9101);
    let callee = addr(9102);
    // Register both via handle_listen
    assert!(turn.handle_listen("CALLERID", &caller));
    assert!(turn.handle_listen("CALLEEID", &callee));

    // Source of message is caller
    router.set_last_from(Some(caller));

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
    let t = obj.get("type").and_then(|v| v.as_str());
    assert_eq!(t, Some("RelayResponse"));
    let ch = obj.get("channel").and_then(|v| v.as_u64()).expect("channel");
    let ch_u16 = ch as u16;

    // Verify internal mappings reflect (caller, callee) -> ch and ch -> caller
    let mapped_dest = turn.lookup_addr_by_channel_for_tests(ch_u16).expect("ch->addr");
    assert_eq!(mapped_dest, (caller, callee));
    let p2c = turn.lookup_channel_for_pair_for_tests(&caller, &callee).expect("pair->ch");
    assert_eq!(p2c, ch_u16);
}
