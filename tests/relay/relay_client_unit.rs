use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak_api_both};
use rust_comms::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId};
use rust_comms::ddb::client::DdbClient;
use rust_comms::relay::relay_client::RelayClient;

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

// ---------------- Mocks ----------------

#[derive(Clone)]
struct ApiMock {
    sent_nsk: Arc<Mutex<Option<NetworkEndpoint>>>,
    sent_uid: Arc<Mutex<Option<String>>>,
    response: serde_json::Value,
}

impl ApiMock {
    fn new(response: serde_json::Value) -> Self {
        Self {
            sent_nsk: Arc::new(Mutex::new(None)),
            sent_uid: Arc::new(Mutex::new(None)),
            response,
        }
    }
}

impl InnerBingleApi for ApiMock {
    fn send_message_to_network_with_response(
        &self,
        network_source_key: &NetworkEndpoint,
        user_id: &UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> {
        *self.sent_nsk.lock().unwrap() = Some(network_source_key.clone());
        *self.sent_uid.lock().unwrap() = Some(user_id.clone());
        Ok(self.response.clone())
    }
}

#[derive(Clone)]
struct DdbMock {
    lookup_result: Option<NetworkEndpoint>,
}

impl DdbMock {
    fn new(lookup_result: Option<NetworkEndpoint>) -> Self {
        Self { lookup_result }
    }
}

impl DdbClient for DdbMock {
    fn register_ip(
        &self,
        _endpoint: SocketAddr,
        _am_relay: bool,
    ) -> Result<(), rust_comms::api::bingle_api::BingleError> {
        Err(rust_comms::api::bingle_api::BingleError::Other(
            "not used".into(),
        ))
    }
    fn register_relay(
        &self,
        _relay_id: String,
        _relay_sig: Option<String>,
    ) -> Result<(), rust_comms::api::bingle_api::BingleError> {
        Err(rust_comms::api::bingle_api::BingleError::Other(
            "not used".into(),
        ))
    }
    fn lookup(
        &self,
        _id: &str,
    ) -> Result<NetworkEndpoint, rust_comms::api::bingle_api::BingleError> {
        self.lookup_result
            .clone()
            .ok_or_else(|| rust_comms::api::bingle_api::BingleError::Other("no lookup".into()))
    }
    fn start_load_from_peer(
        &self,
        _peer_id: &str,
    ) -> Result<usize, rust_comms::api::bingle_api::BingleError> {
        Err(rust_comms::api::bingle_api::BingleError::Other(
            "not used".into(),
        ))
    }
    fn signoff(&self) -> Result<(), rust_comms::api::bingle_api::BingleError> {
        Err(rust_comms::api::bingle_api::BingleError::Other(
            "not used".into(),
        ))
    }
}

// ---------------- Tests ----------------

#[test]
#[cfg(not(target_os = "ios"))]
pub fn call_with_address_present_returns_endpoint_with_channel() {
    let relay_id = "RELAYID123".to_string();
    let relay_addr = addr(9100);
    let nsk = NetworkEndpoint::new_relay(relay_id.clone(), Some(relay_addr), None);

    let ddb = DdbMock::new(None);
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(ApiMock::new(
        serde_json::json!({ "type": "RelayResponse", "channel": 3456 }),
    ));
    let client = RelayClient::new(
        to_weak_api_both(MockApiBoth::new_with_api_override(api)),
        Arc::new(ddb),
    );

    let out = client.call(&nsk, "TARGETID").expect("call ok");

    assert!(out.is_relay());
    assert_eq!(out.relay_id().unwrap(), relay_id);
    assert_eq!(out.relay_address().unwrap(), relay_addr);
    assert_eq!(out.relay_channel().unwrap(), 3456);

    // And we sent to the relay address directly with the relay id as user_id
    // Note: In unit test, we can check engine's tracked connections if needed
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn call_resolves_relay_address_via_ddb_when_missing() {
    let relay_id = "RELAYID123".to_string();
    let relay_addr = addr(9200);
    let nsk = NetworkEndpoint::new_relay(relay_id.clone(), None, None);

    let ddb = DdbMock::new(Some(NetworkEndpoint::new_direct(relay_addr)));
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(ApiMock::new(
        serde_json::json!({ "type": "RelayResponse", "channel": 777 }),
    ));
    let client = RelayClient::new(
        to_weak_api_both(MockApiBoth::new_with_api_override(api)),
        Arc::new(ddb),
    );

    let out = client.call(&nsk, "TARGETID").expect("call ok");

    assert!(out.is_relay());
    assert_eq!(out.relay_id().unwrap(), relay_id);
    assert_eq!(out.relay_address().unwrap(), relay_addr);
    assert_eq!(out.relay_channel().unwrap(), 777);

    // ensure engine would have sent
}
