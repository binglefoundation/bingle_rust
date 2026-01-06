use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::sync::Arc;

use rust_comms::messages::{Message, RelayMessage};
use rust_comms::messages::handlers::{DefaultPrintingHandler, MessageHandler};
use rust_comms::messages::types::{RelayListen};
use rust_comms::api::bingle_api::{BingleApi, StartOptions, NetworkEndpoint, UserId, Handle, ProgressCallback, OnMessageHandler, OnConnectHandler};

// Minimal API stub for router context
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
fn relay_listen_registers_and_responds() {
    // Arrange: a Router configured as a relay with a TURN handler
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(Arc::new(MockApi)));
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());
    router.set_turn_handler(Some(turn.clone()));
    router.set_am_relay(true);
    let source = addr(9001);
    router.set_last_from(Some(source));

    // Act: route a Relay::Listen message via DefaultPrintingHandler
    let handler = DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::Listen(RelayListen { app: None }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        // from_id not used in this path
        router.route(&handler, &msg, "ALGOADDR123");
    });

    // Assert response was produced and source IP registered
    let out = router.take_outbound_response();
    assert!(out.is_some(), "expected an outbound response");
    let obj = out.unwrap();
    let t = obj.get("type").and_then(|v| v.as_str());
    assert_eq!(t, Some("ListenResponse"));

    // id->addr map should contain the from.id (issuer trimmed) -> source address
    assert_eq!(turn.lookup_addr_by_id("ALGOADDR123"), Some(source));
}
