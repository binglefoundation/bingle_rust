use rust_comms::engine::BingleAccessUnsafeForTests;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{BingleApi, NetworkEndpoint, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result};
use std::time::Duration;
#[path = "../test_util.rs"]
pub mod test_util;

#[derive(Clone)]
struct MockDtls {
    pub sends: Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
    pub handle_message: Arc<Mutex<Option<HandleMessage>>>,
}

impl MockDtls {
    fn new() -> (Self, Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>) {
        let v = Arc::new(Mutex::new(vec![]));
        (
            Self {
                sends: v.clone(),
                handle_message: Arc::new(Mutex::new(None)),
            },
            v,
        )
    }
}

impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<rust_comms::dtls::UdpNetworkMux>) -> Result<()> {
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        Ok(())
    }
    fn send(&self, to: &rust_comms::api::bingle_api::NetworkEndpoint, data: &[u8]) -> Result<()> {
        let addr = to
            .inet_socket_address()
            .expect("MockDtls::send requires inet_socket_address");
        self.sends.lock().unwrap().push((addr, data.to_vec()));
        if data.len() >= 4 && (data[0] & 0x0F) == 0x01 {
            if let Some(h) = self.handle_message.lock().unwrap().clone() {
                h(
                    self,
                    to,
                    "mock-auto-ack",
                    &vec![0x14, 0x00, data[2], data[3]],
                );
            }
        }
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> {
        self.handle_message.lock().unwrap().clone()
    }
    fn set_handle_message(&mut self, handler: Option<HandleMessage>) {
        *self.handle_message.lock().unwrap() = handler;
    }
    fn set_handle_new_session(
        &mut self,
        _handler: Option<rust_comms::dtls::dtls_trait::HandleNewSession>,
    ) {
    }
    fn with_handle_message(mut self, handler: HandleMessage) -> Self
    where
        Self: Sized,
    {
        self.set_handle_message(Some(handler));
        self
    }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> {
        None
    }
    fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_ca_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_dangerous_debug(&mut self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_cipher_suite(
        &self,
        _endpoint: &rust_comms::api::bingle_api::NetworkEndpoint,
    ) -> Option<String> {
        None
    }
    fn forget_peers(&self) {}
}

/// When a relay endpoint's relay_id matches our own id, send_message_to_network
/// should bypass the relay Call and convert to a direct endpoint using the relay_address.
#[test]
#[cfg(not(target_os = "ios"))]
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
    let ok = api
        .access_unsafe_for_tests(|a: &mut BingleApiImpl| {
            a.send_message_to_network(
                &relay_nsk,
                &target_uid,
                serde_json::json!({"test": true}),
                None,
            )
        })
        .unwrap();

    assert!(
        ok,
        "send_message_to_network should succeed by converting self-relay to direct"
    );
    let locked = sent_vec.lock().unwrap();
    assert_eq!(
        locked.len(),
        1,
        "MockDtls::send should have been called once"
    );
    assert_eq!(
        locked[0].0, relay_addr,
        "send should target the relay_address directly"
    );
}

/// When a relay endpoint's relay_id matches our own id but relay_address is missing,
/// send_message_to_network should return false.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn self_relay_no_relay_address_returns_false() {
    let (mock, sent_vec) = MockDtls::new();
    let mut options = rust_comms::api::bingle_api::StartOptions::new("".into());
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
    let ok = api
        .access_unsafe_for_tests(|a: &mut BingleApiImpl| {
            a.send_message_to_network(
                &relay_nsk,
                &target_uid,
                serde_json::json!({"test": true}),
                None,
            )
        })
        .unwrap();

    assert!(
        !ok,
        "send_message_to_network should return false for self-relay with no relay_address"
    );
    assert_eq!(
        sent_vec.lock().unwrap().len(),
        0,
        "MockDtls::send should not have been called"
    );
}

/// When a relay endpoint's relay_id does NOT match our own id, the self-relay
/// detection should not trigger (the normal relay Call path would be attempted).
/// Since there is no real relay to call in this test, the call will fail and return false.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn non_self_relay_is_not_converted() {
    let (mock, _sent_vec) = MockDtls::new();
    let mut options = StartOptions::new("".into());
    options.wait_response_timeout = Some(Duration::from_millis(100));
    let api = BingleApiImpl::new_with_dtls_and_options(Box::new(mock), options);

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
    let ok = api
        .access_unsafe_for_tests(|a: &mut BingleApiImpl| {
            a.send_message_to_network(
                &relay_nsk,
                &target_uid,
                serde_json::json!({"test": true}),
                None,
            )
        })
        .unwrap();

    // The relay Call path is attempted (sends a Call message to the relay via MockDtls),
    // but the response times out since there is no actual relay, so result should be false.
    assert!(
        !ok,
        "send_message_to_network should fail for non-self relay (no real relay to call)"
    );
}

/// When no issuer is set (get_my_id returns None), the self-relay detection
/// should not trigger even if relay_id is present (the normal relay Call path is attempted).
#[test]
#[cfg(not(target_os = "ios"))]
pub fn self_relay_no_issuer_does_not_match() {
    let (mock, _sent_vec) = MockDtls::new();
    let mut options = StartOptions::new("".into());
    options.wait_response_timeout = Some(Duration::from_millis(100));
    let api = BingleApiImpl::new_with_dtls_and_options(Box::new(mock), options);

    // Do NOT set issuer — get_my_id() will return None

    let relay_id = test_util::ADDRESS_SPEND;
    let relay_addr: SocketAddr = "10.0.0.1:5000".parse().expect("valid addr");
    let relay_nsk = NetworkEndpoint::new_relay(relay_id.to_string(), Some(relay_addr), None);

    let target_uid = test_util::ADDRESS_RECEIVE.to_string();
    let ok = api
        .access_unsafe_for_tests(|a: &mut BingleApiImpl| {
            a.send_message_to_network(
                &relay_nsk,
                &target_uid,
                serde_json::json!({"test": true}),
                None,
            )
        })
        .unwrap();

    // With no issuer, my_id is None, so is_self_relay is false.
    // The relay Call path is attempted but times out, so result should be false.
    assert!(
        !ok,
        "send_message_to_network should fail when issuer is not set (relay Call fails)"
    );
}
