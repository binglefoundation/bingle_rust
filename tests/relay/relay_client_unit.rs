use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{BingleApi, NetworkEndpoint, ProgressCallback, StartOptions, UserId, Handle, OnMessageHandler, OnConnectHandler};
use rust_comms::relay::relay_client::RelayClient;
use rust_comms::ddb::client::DdbClient;

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

// ---------------- Mocks ----------------

#[derive(Clone)]
struct ApiMock {
    sent_nsk: Arc<Mutex<Option<NetworkEndpoint>>>,
    sent_uid: Arc<Mutex<Option<String>>>,
    response: serde_json::Value,
}

impl ApiMock {
    fn new(response: serde_json::Value) -> Self {
        Self { sent_nsk: Arc::new(Mutex::new(None)), sent_uid: Arc::new(Mutex::new(None)), response }
    }
}

impl BingleApi for ApiMock { 
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
    fn send_message_to_network_with_response(&self, network_source_key: &NetworkEndpoint, user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> {
        *self.sent_nsk.lock().unwrap() = Some(network_source_key.clone());
        *self.sent_uid.lock().unwrap() = Some(user_id.clone());
        Ok(self.response.clone())
    }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

#[derive(Clone)]
struct DdbMock {
    lookup_result: Option<NetworkEndpoint>,
}

impl DdbMock { fn new(lookup_result: Option<NetworkEndpoint>) -> Self { Self { lookup_result } } }

impl DdbClient for DdbMock {
    fn register_ip(&self, _endpoint: SocketAddr) -> Result<(), String> { Err("not used".into()) }
    fn register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> { Err("not used".into()) }
    fn lookup(&self, _id: &str) -> Result<NetworkEndpoint, String> { self.lookup_result.clone().ok_or_else(|| "no lookup".into()) }
    fn start_load_from_peer(&self, _peer_id: &str) -> Result<usize, String> { Err("not used".into()) }
}

// ---------------- Tests ----------------

#[test]
fn call_with_address_present_returns_endpoint_with_channel() {
    let relay_id = "RELAYID123".to_string();
    let relay_addr = addr(9100);
    let nsk = NetworkEndpoint::new_relay(relay_id.clone(), Some(relay_addr), None);

    let api = ApiMock::new(serde_json::json!({
        "type": "RelayResponse",
        "channel": 3456
    }));
    let ddb = DdbMock::new(None);

    let client = RelayClient::new(crate::util::mock_bingle_api::to_weak(api.clone()), Arc::new(ddb));

    let out = client.call(&nsk, "TARGETID").expect("call ok");

    assert!(out.is_relay());
    assert_eq!(out.relay_id().unwrap(), relay_id);
    assert_eq!(out.relay_address().unwrap(), relay_addr);
    assert_eq!(out.relay_channel().unwrap(), 3456);

    // And we sent to the relay address directly with the relay id as user_id
    let sent = api.sent_nsk.lock().unwrap().clone().expect("sent nsk");
    assert_eq!(sent.inet_socket_address().unwrap(), relay_addr);
    let uid = api.sent_uid.lock().unwrap().clone().expect("uid");
    assert_eq!(uid, relay_id);
}

#[test]
fn call_resolves_relay_address_via_ddb_when_missing() {
    let relay_id = "RELAYID123".to_string();
    let relay_addr = addr(9200);
    let nsk = NetworkEndpoint::new_relay(relay_id.clone(), None, None);

    let api = ApiMock::new(serde_json::json!({
        "type": "RelayCallResponse",
        "calledId": "TARGETID",
        "channel": 777
    }));
    let ddb = DdbMock::new(Some(NetworkEndpoint::new_direct(relay_addr)));

    let client = RelayClient::new(crate::util::mock_bingle_api::to_weak(api.clone()), Arc::new(ddb));

    let out = client.call(&nsk, "TARGETID").expect("call ok");

    assert!(out.is_relay());
    assert_eq!(out.relay_id().unwrap(), relay_id);
    assert_eq!(out.relay_address().unwrap(), relay_addr);
    assert_eq!(out.relay_channel().unwrap(), 777);

    // ensure API was called with resolved address
    let sent = api.sent_nsk.lock().unwrap().clone().expect("sent nsk");
    assert_eq!(sent.inet_socket_address().unwrap(), relay_addr);
}
