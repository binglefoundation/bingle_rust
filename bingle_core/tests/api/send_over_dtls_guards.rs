use bingle_core::engine::BingleAccessUnsafeForTests;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bingle_core::api::bingle_api::{BingleApi, BingleError, NetworkEndpoint, SendFailureKind};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result};
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
    fn start(&self, _mux: Arc<bingle_core::dtls::UdpNetworkMux>) -> Result<()> {
        Ok(())
    }
    fn stop(&self) -> Result<()> {
        Ok(())
    }
    fn send(&self, to: &bingle_core::api::bingle_api::NetworkEndpoint, data: &[u8]) -> Result<()> {
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
    fn set_handle_message(&self, handler: Option<HandleMessage>) {
        *self.handle_message.lock().unwrap() = handler;
    }
    fn set_handle_new_session(
        &self,
        _handler: Option<bingle_core::dtls::dtls_trait::HandleNewSession>,
    ) {
    }
    fn with_handle_message(self, handler: HandleMessage) -> Self
    where
        Self: Sized,
    {
        self.set_handle_message(Some(handler));
        self
    }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> {
        None
    }
    fn set_handle_peer_certificate(&self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_ca_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_ca_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_private_key(&self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_private_key(&self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_app_layer_only_verification(&self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_dangerous_debug(&self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_null_encryption(&self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_cipher_suite(
        &self,
        _endpoint: &bingle_core::api::bingle_api::NetworkEndpoint,
    ) -> Option<String> {
        None
    }
    fn forget_peers(&self) {}
}

/// An incomplete relay endpoint (missing channel) drives the relay Call path to allocate a channel;
/// with no real relay that allocation fails, so send_message_to_network surfaces the typed
/// RelayAllocationFailed cause and never reaches the DTLS send (issue #99).
#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_over_dtls_rejects_incomplete_relay_endpoint() {
    let (mock, sent_vec) = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock));
    let relay_nsk = NetworkEndpoint::new_relay(
        "SOME_RELAY_ID".to_string(),
        Some("10.0.0.1:5000".parse::<SocketAddr>().expect("valid addr")),
        None,
    );
    let uid = test_util::ADDRESS_SPEND.to_string();
    let result = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.send_message_to_network(&relay_nsk, &uid, serde_json::json!({"test": true}), None)
    });
    match result {
        Err(BingleError::Send { kind, .. }) => assert_eq!(
            kind,
            SendFailureKind::RelayAllocationFailed,
            "incomplete relay endpoint should fail relay allocation"
        ),
        other => panic!("expected a typed RelayAllocationFailed send error, got {other:?}"),
    }
    assert_eq!(
        sent_vec.lock().unwrap().len(),
        0,
        "MockDtls::send should not have been called"
    );
}

/// send_message_to_network returns false when the target address matches our own public address
/// because send_over_dtls rejects sending to self.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_over_dtls_rejects_send_to_self() {
    let (mock, sent_vec) = MockDtls::new();
    let my_addr: SocketAddr = "44.223.62.108:12121".parse().expect("valid addr");
    let api = BingleApiImpl::new_with_dtls(Box::new(mock));
    // Set the engine's public address to our own address
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.engine_set_public_addr_for_tests(Some(my_addr));
    });
    let nsk = NetworkEndpoint::new_direct(my_addr);
    let uid = test_util::ADDRESS_SPEND.to_string();
    let ok = api
        .access_unsafe_for_tests(|a: &mut BingleApiImpl| {
            a.send_message_to_network(&nsk, &uid, serde_json::json!({"test": true}), None)
        })
        .unwrap();
    assert!(
        !ok,
        "send_message_to_network should return false when sending to self"
    );
    assert_eq!(
        sent_vec.lock().unwrap().len(),
        0,
        "MockDtls::send should not have been called"
    );
}

/// send_message_to_network succeeds for a direct endpoint to a different address.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_over_dtls_allows_direct_to_different_addr() {
    let (mock, sent_vec) = MockDtls::new();
    let my_addr: SocketAddr = "44.223.62.108:12121".parse().expect("valid addr");
    let api = BingleApiImpl::new_with_dtls(Box::new(mock));
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| {
        a.engine_set_public_addr_for_tests(Some(my_addr));
    });
    let other_addr: SocketAddr = "10.0.0.1:5000".parse().expect("valid addr");
    let nsk = NetworkEndpoint::new_direct(other_addr);
    let uid = test_util::ADDRESS_SPEND.to_string();
    let ok = api
        .access_unsafe_for_tests(|a: &mut BingleApiImpl| {
            a.send_message_to_network(&nsk, &uid, serde_json::json!({"test": true}), None)
        })
        .unwrap();
    assert!(
        ok,
        "send_message_to_network should succeed for direct endpoint to different addr"
    );
    assert_eq!(
        sent_vec.lock().unwrap().len(),
        1,
        "MockDtls::send should have been called once"
    );
}
