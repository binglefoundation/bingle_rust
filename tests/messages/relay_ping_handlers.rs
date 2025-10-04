use std::net::SocketAddr;
use std::sync::Arc;

use rust_comms::dtls::dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use rust_comms::dtls::UdpNetworkMux;
use rust_comms::messages::*;

#[derive(Default, Clone)]
struct MockDtls {
    pub sent: Arc<std::sync::Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
}

impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
    fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
    fn send(&self, to: SocketAddr, data: &[u8]) -> DtlsResult<()> {
        let mut g = self.sent.lock().unwrap();
        g.push((to, data.to_vec()));
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> { None }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
    fn with_handle_message(self, _handler: HandleMessage) -> Self { self }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> { None }
    fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self { self }
    fn get_ca_cert(&self) -> Option<&[u8]> { None }
    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self { self }
    fn get_client_cert(&self) -> Option<&[u8]> { None }
    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self { self }
    fn get_client_private_key(&self) -> Option<&[u8]> { None }
    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self { self }
    fn get_server_signing_cert(&self) -> Option<&[u8]> { None }
    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self { self }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> { None }
    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self { self }
}

#[test]
fn on_triangle_test1_sends_triangle_test2_to_peer() {
    let mock = Arc::new(MockDtls::default());
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let handler = RelayPingHandler::new(mock.clone(), Some(peer));

    let t1 = RelayTriangleTest1 { app: None, checkingEndpoint: "127.0.0.1:12345".parse().unwrap() };
    handler.on_triangle_test1(&t1);

    let records = mock.sent.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    let (to, data) = &records[0];
    assert_eq!(*to, peer);

    let text = std::str::from_utf8(&data).expect("utf8");
    let msg = from_json_str(text).expect("decode");
    match msg {
        Message::Relay(RelayMessage::TriangleTest2(m)) => {
            assert_eq!(m.checkingEndpoint.to_string(), "127.0.0.1:12345");
        }
        other => panic!("expected TriangleTest2, got {:?}", other),
    }
}

#[test]
fn on_triangle_test2_sends_triangle_test3_to_endpoint() {
    let mock = Arc::new(MockDtls::default());
    let handler = RelayPingHandler::new(mock.clone(), None);

    let endpoint: SocketAddr = "127.0.0.1:33333".parse().unwrap();
    let t2 = RelayTriangleTest2 { app: None, checkingId: "id-abc".into(), checkingEndpoint: endpoint };
    handler.on_triangle_test2(&t2);

    let records = mock.sent.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    let (to, data) = &records[0];
    assert_eq!(*to, endpoint);

    let text = std::str::from_utf8(&data).expect("utf8");
    let msg = from_json_str(text).expect("decode");
    match msg {
        Message::Relay(RelayMessage::TriangleTest3(_)) => {}
        other => panic!("expected TriangleTest3, got {:?}", other),
    }
}
