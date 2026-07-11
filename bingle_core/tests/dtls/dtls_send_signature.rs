use std::net::SocketAddr;
use std::sync::Arc;

use bingle_core::dtls::{Dtls, HandleMessage, HandlePeerCertificate, UdpNetworkMux};

#[derive(Default)]
struct DummyDtls;

impl Dtls for DummyDtls {
    fn start(&self, _mux: Arc<UdpNetworkMux>) -> Result<(), String> {
        Ok(())
    }
    fn stop(&self) -> Result<(), String> {
        Ok(())
    }
    fn send(
        &self,
        _to: &bingle_core::api::bingle_api::NetworkEndpoint,
        _data: &[u8],
    ) -> Result<(), String> {
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> {
        None
    }
    fn set_handle_message(&self, _handler: Option<HandleMessage>) {}
    fn set_handle_new_session(
        &self,
        _handler: Option<bingle_core::dtls::dtls_trait::HandleNewSession>,
    ) {
    }
    fn with_handle_message(self, _handler: HandleMessage) -> Self
    where
        Self: Sized,
    {
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

#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_send_takes_no_issuer() {
    let d = DummyDtls::default();
    let to: SocketAddr = "127.0.0.1:9000".parse().unwrap();
    let data = b"hello";
    // Should compile and run via NetworkEndpoint helper
    let nsk = bingle_core::api::bingle_api::NetworkEndpoint::new_direct(to);
    assert!(Dtls::send(&d, &nsk, data).is_ok());
}
