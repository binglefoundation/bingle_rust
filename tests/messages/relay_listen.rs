use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};

use rust_comms::messages::{Message, RelayMessage};
use rust_comms::turn::turn_handler::TurnHandler;
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::types::{RelayListen};
use rust_comms::api::bingle_api::{BingleApi, StartOptions, NetworkEndpoint, UserId, Handle, ProgressCallback, OnMessageHandler, OnConnectHandler};
use crate::util::mock_api::MockApiBoth;

// Minimal API stub for router context
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
fn relay_listen_registers_and_responds() {
    // Arrange: a Router configured as a relay with a TURN handler
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(crate::util::mock_api::to_weak(MockApiBoth::new())));
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());
    // Provide internal API that exposes the shared TurnHandlerImpl
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
        fn turn_handle_listen(&self, id: std::string::String, source: std::net::SocketAddr) -> bool { self.turn.handle_listen(&id, &source) }
        fn turn_handle_called(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr, _channel: u16) { }
        fn notify_listening(&self, _listening: bool) { }
    }
    // router.set_bingle_api_internal(Some(std::sync::Arc::new(MockInternal { turn: turn.clone() }) as std::sync::Arc<dyn rust_comms::api::bingle_api::BingleApiInternal>));
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
    let t = obj.get("type").and_then(|v: &serde_json::Value| v.as_str());
    assert_eq!(t, Some("ListenResponse"));

    // id->addr map should contain the from.id (issuer trimmed) -> source address
    assert_eq!(turn.lookup_addr_by_id("ALGOADDR123"), Some(source));
}
