use crate::util::reusable_mock_api::MockApiBoth;
use rust_comms::api::bingle_api::{NetworkEndpoint, StartOptions};
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::dtls::{Dtls, HandleMessage};
use rust_comms::engine::Engine;
use std::net::SocketAddr;
use crate::relay::lookup_root_id::test_util::init_test_logging;

#[derive(Default)]
struct FakeDtls {
    handler: std::sync::Mutex<Option<HandleMessage>>,
}

impl FakeDtls {
    fn new() -> Self { Self { handler: std::sync::Mutex::new(None) } }
}

impl Dtls for FakeDtls {
    fn start(&mut self, _mux: std::sync::Arc<UdpNetworkMux>) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn stop(&mut self) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn send(&self, to: &NetworkEndpoint, data: &[u8]) -> rust_comms::dtls::Result<()> {
        let has_frpt_header = data.len() >= 4;
        let is_data_single = has_frpt_header && data[0] == 0x11;

        if is_data_single {
            let tx_id_hi = data[2];
            let tx_id_lo = data[3];
            let ack_complete_packet = [0x14, 0x00, tx_id_hi, tx_id_lo];

            let handler = self
                .get_handle_message()
                .ok_or_else(|| "FakeDtls::send expected a handle_message callback".to_string())?;
            handler(self, to, "fake-dtls", &ack_complete_packet);
        }

        Ok(())
    }

    fn get_handle_message(&self) -> Option<HandleMessage> { self.handler.lock().ok().and_then(|g| g.clone()) }
    fn set_handle_message(&mut self, handler: Option<HandleMessage>) { let _ = self.handler.lock().map(|mut g| *g = handler); }
    fn with_handle_message(self, handler: HandleMessage) -> Self where Self: Sized { let _ = self.handler.lock().map(|mut g| *g = Some(handler)); self }

    fn get_handle_peer_certificate(&self) -> Option<rust_comms::dtls::HandlePeerCertificate> { None }
    fn set_handle_peer_certificate(&mut self, _handler: Option<rust_comms::dtls::HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: rust_comms::dtls::HandlePeerCertificate) -> Self where Self: Sized { self }

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
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self where Self: Sized { self }
}

fn make_engine_with_public_addr(addr: SocketAddr) -> Engine {
    let mut opts = StartOptions::default();
    opts.static_ip = Some(addr);
    Engine::new_with_dtls(
        &opts,
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
        Box::new(FakeDtls::new()),
    )
}

fn make_engine_no_public_addr() -> Engine {
    Engine::new_with_dtls(
        &StartOptions::default(),
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
        Box::new(FakeDtls::new()),
    )
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn send_to_peer_rejects_incomplete_relay_endpoint() {
    let engine = make_engine_no_public_addr();
    // Relay endpoint without a channel (incomplete) should be rejected
    let relay_nsk = NetworkEndpoint::new_relay(
        "SOME_RELAY_ID".to_string(),
        Some("10.0.0.1:5000".parse::<SocketAddr>().expect("valid addr")),
        None,
    );
    let result = engine.send_to_peer(&relay_nsk, b"hello");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("rejecting incomplete relay endpoint"), "unexpected error: {}", err);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn send_to_peer_allows_complete_relay_endpoint() {
    init_test_logging();
    
    let engine = make_engine_no_public_addr();
    // Relay endpoint with channel+address+id is valid (handled by TURN layer)
    let relay_nsk = NetworkEndpoint::new_relay(
        "SOME_RELAY_ID".to_string(),
        Some("10.0.0.1:5000".parse::<SocketAddr>().expect("valid addr")),
        Some(0x4000),
    );
    let result = engine.send_to_peer(&relay_nsk, b"hello");
    assert!(result.is_ok());
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn send_to_peer_rejects_send_to_self() {
    let my_addr: SocketAddr = "44.223.62.108:12121".parse().expect("valid addr");
    let engine = make_engine_with_public_addr(my_addr);
    let nsk = NetworkEndpoint::new_direct(my_addr);
    let result = engine.send_to_peer(&nsk, b"hello");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("rejecting send to self"), "unexpected error: {}", err);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn send_to_peer_allows_direct_to_different_addr() {
    let my_addr: SocketAddr = "44.223.62.108:12121".parse().expect("valid addr");
    let engine = make_engine_with_public_addr(my_addr);
    let other_addr: SocketAddr = "10.0.0.1:5000".parse().expect("valid addr");
    let nsk = NetworkEndpoint::new_direct(other_addr);
    let result = engine.send_to_peer(&nsk, b"hello");
    assert!(result.is_ok());
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn send_to_peer_allows_direct_when_no_public_addr() {
    let engine = make_engine_no_public_addr();
    let addr: SocketAddr = "10.0.0.1:5000".parse().expect("valid addr");
    let nsk = NetworkEndpoint::new_direct(addr);
    let result = engine.send_to_peer(&nsk, b"hello");
    assert!(result.is_ok());
}
