use rust_comms::engine::BingleAccessUnsafeForTests;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{NetworkEndpoint, BingleApi};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result};
#[path = "../test_util.rs"]
pub mod test_util;

#[derive(Clone)]
struct MockDtls {
    pub sends: Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
}

impl MockDtls {
    fn new() -> (Self, Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>) {
        let v = Arc::new(Mutex::new(vec![]));
        (Self { sends: v.clone() }, v)
    }
}

impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<rust_comms::dtls::UdpNetworkMux>) -> Result<()> { Ok(()) }
    fn stop(&mut self) -> Result<()> { Ok(()) }
    fn send(&self, to: &rust_comms::api::bingle_api::NetworkEndpoint, data: &[u8]) -> Result<()> {
        let addr = to.inet_socket_address().expect("MockDtls::send requires inet_socket_address");
        self.sends.lock().unwrap().push((addr, data.to_vec()));
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> { None }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
    fn with_handle_message(self, _handler: HandleMessage) -> Self where Self: Sized { self }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> { None }
    fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self where Self: Sized { self }
    fn get_ca_cert(&self) -> Option<&[u8]> { None }
    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_client_cert(&self) -> Option<&[u8]> { None }
    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_client_private_key(&self) -> Option<&[u8]> { None }
    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_server_signing_cert(&self) -> Option<&[u8]> { None }
    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> { None }
    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_dangerous_debug(&mut self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self where Self: Sized { self }
}

/// When a relay endpoint's relay_id matches our own id, send_message_to_network
/// should bypass the relay Call and convert to a direct endpoint using the relay_address.
#[cfg_attr(not(target_os = "ios"), test)]
pub fn self_relay_converts_to_direct_send() {
    let (mock, sent_vec) = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock));

    let my_id = test_util::ADDRESS_SPEND;
    let issuer = format!("{}.", my_id);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.set_issuer_for_tests(issuer);
    });

    let relay_addr: SocketAddr = "10.0.0.1:5000".parse().expect("valid addr");
    let relay_nsk = NetworkEndpoint::new_relay(
        my_id.to_string(),
        Some(relay_addr),
        None, // no channel — triggers the relay Call path
    );

    let target_uid = test_util::ADDRESS_RECEIVE.to_string();
    let ok = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.send_message_to_network(&relay_nsk, &target_uid, serde_json::json!({"test": true}), None)
    }).unwrap();

    assert!(ok, "send_message_to_network should succeed by converting self-relay to direct");
    let locked = sent_vec.lock().unwrap();
    assert_eq!(locked.len(), 1, "MockDtls::send should have been called once");
    assert_eq!(locked[0].0, relay_addr, "send should target the relay_address directly");
}

/// When a relay endpoint's relay_id matches our own id but relay_address is missing,
/// send_message_to_network should return false.
#[cfg_attr(not(target_os = "ios"), test)]
pub fn self_relay_no_relay_address_returns_false() {
    let (mock, sent_vec) = MockDtls::new();
    let mut options = rust_comms::api::bingle_api::StartOptions::default();
    options.am_relay = true;
    let api = BingleApiImpl::new_with_dtls_and_options(Box::new(mock), options);

    let my_id = test_util::ADDRESS_SPEND;
    let issuer = format!("{}.", my_id);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.set_issuer_for_tests(issuer);
    });

    let relay_nsk = NetworkEndpoint::new_relay(
        my_id.to_string(),
        None, // no relay_address
        None, // no channel
    );

    let target_uid = test_util::ADDRESS_RECEIVE.to_string();
    let ok = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.send_message_to_network(&relay_nsk, &target_uid, serde_json::json!({"test": true}), None)
    }).unwrap();

    assert!(!ok, "send_message_to_network should return false for self-relay with no relay_address");
    assert_eq!(sent_vec.lock().unwrap().len(), 0, "MockDtls::send should not have been called");
}

/// When a relay endpoint's relay_id does NOT match our own id, the self-relay
/// detection should not trigger (the normal relay Call path would be attempted).
/// Since there is no real relay to call in this test, the call will fail and return false.
#[cfg_attr(not(target_os = "ios"), test)]
pub fn non_self_relay_is_not_converted() {
    let (mock, _sent_vec) = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock));

    let my_id = test_util::ADDRESS_SPEND;
    let issuer = format!("{}.", my_id);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.set_issuer_for_tests(issuer);
    });

    let other_relay_id = test_util::ADDRESS_RECEIVE;
    let relay_addr: SocketAddr = "10.0.0.1:5000".parse().expect("valid addr");
    let relay_nsk = NetworkEndpoint::new_relay(
        other_relay_id.to_string(),
        Some(relay_addr),
        None, // no channel — triggers the relay Call path
    );

    let target_uid = test_util::ADDRESS_RECEIVE.to_string();
    let ok = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.send_message_to_network(&relay_nsk, &target_uid, serde_json::json!({"test": true}), None)
    }).unwrap();

    // The relay Call path is attempted (sends a Call message to the relay via MockDtls),
    // but the response times out since there is no actual relay, so result should be false.
    assert!(!ok, "send_message_to_network should fail for non-self relay (no real relay to call)");
}

/// When no issuer is set (get_my_id returns None), the self-relay detection
/// should not trigger even if relay_id is present (the normal relay Call path is attempted).
#[cfg_attr(not(target_os = "ios"), test)]
pub fn self_relay_no_issuer_does_not_match() {
    let (mock, _sent_vec) = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock));

    // Do NOT set issuer — get_my_id() will return None

    let relay_id = test_util::ADDRESS_SPEND;
    let relay_addr: SocketAddr = "10.0.0.1:5000".parse().expect("valid addr");
    let relay_nsk = NetworkEndpoint::new_relay(
        relay_id.to_string(),
        Some(relay_addr),
        None,
    );

    let target_uid = test_util::ADDRESS_RECEIVE.to_string();
    let ok = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.send_message_to_network(&relay_nsk, &target_uid, serde_json::json!({"test": true}), None)
    }).unwrap();

    // With no issuer, my_id is None, so is_self_relay is false.
    // The relay Call path is attempted but times out, so result should be false.
    assert!(!ok, "send_message_to_network should fail when issuer is not set (relay Call fails)");
}
