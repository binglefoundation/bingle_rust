use rust_comms::api::bingle_api::NetworkEndpoint;
use rust_comms::dtls::dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use rust_comms::dtls::UdpNetworkMux;
use rust_comms::packet_transport::{
    DtlsReliablePacketTransport,
    PacketTransport,
    PacketTransportHandleMessage,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

struct MockDtls {
    handle_message: Option<HandleMessage>,
    sent_packets: Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
}

impl MockDtls {
    fn new() -> Self {
        Self {
            handle_message: None,
            sent_packets: Arc::new(Mutex::new(vec![])),
        }
    }
}

impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> DtlsResult<()> {
        Ok(())
    }

    fn stop(&mut self) -> DtlsResult<()> {
        Ok(())
    }

    fn send(&self, to: &NetworkEndpoint, data: &[u8]) -> DtlsResult<()> {
        let address = to
            .inet_socket_address()
            .expect("MockDtls::send requires inet socket address");
        self.sent_packets
            .lock()
            .expect("sent_packets lock should not be poisoned")
            .push((address, data.to_vec()));
        Ok(())
    }

    fn get_handle_message(&self) -> Option<HandleMessage> {
        self.handle_message.clone()
    }

    fn set_handle_message(&mut self, handler: Option<HandleMessage>) {
        self.handle_message = handler;
    }

    fn with_handle_message(mut self, handler: HandleMessage) -> Self
    where
        Self: Sized,
    {
        self.handle_message = Some(handler);
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
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn new_installs_dtls_handler_that_uses_transport_handler() {
    let mut transport = DtlsReliablePacketTransport::new(Box::new(MockDtls::new()));

    let calls: Arc<Mutex<Vec<(String, Vec<u8>)>>> = Arc::new(Mutex::new(vec![]));
    let calls_clone = calls.clone();
    let transport_handler: PacketTransportHandleMessage = Arc::new(move |_from, issuer, packet| {
        calls_clone
            .lock()
            .expect("calls lock should not be poisoned")
            .push((issuer.to_string(), packet.to_vec()));
        Ok(Some(packet.to_vec()))
    });
    transport.set_handle_message(Some(transport_handler));

    let dtls_handler = transport
        .dtls()
        .get_handle_message()
        .expect("DTLS handler should be installed by packet transport constructor");

    let from_addr: SocketAddr = "127.0.0.1:9090".parse().expect("valid socket address");
    let from = NetworkEndpoint::new_direct(from_addr);
    dtls_handler(transport.dtls(), &from, "peer-issuer", b"hello");

    let locked_calls = calls
        .lock()
        .expect("calls lock should not be poisoned after dtls callback invocation");
    assert_eq!(locked_calls.len(), 1);
    assert_eq!(locked_calls[0].0, "peer-issuer");
    assert_eq!(locked_calls[0].1, b"hello".to_vec());
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn with_handle_message_and_dispatch_handle_message_are_transport_instance_scoped() {
    let captured: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(vec![]));
    let captured_clone = captured.clone();
    let handler: PacketTransportHandleMessage = Arc::new(move |_from, _issuer, packet| {
        captured_clone
            .lock()
            .expect("captured lock should not be poisoned")
            .push(packet.to_vec());
        Ok(Some(vec![0xAA]))
    });

    let transport = DtlsReliablePacketTransport::new(Box::new(MockDtls::new())).with_handle_message(handler);
    assert!(transport.get_handle_message().is_some());

    let from_addr: SocketAddr = "127.0.0.1:9191".parse().expect("valid socket address");
    let from = NetworkEndpoint::new_direct(from_addr);
    let handled = transport
        .dispatch_handle_message(&from, "issuer-a", b"payload")
        .expect("transport handler should return success");
    assert!(handled.is_some());
    assert_eq!(handled.expect("handled payload should exist"), vec![0xAA]);

    let captured_packets = captured
        .lock()
        .expect("captured lock should not be poisoned after handle_message");
    assert_eq!(captured_packets.as_slice(), &[b"payload".to_vec()]);
}
