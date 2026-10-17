use std::net::SocketAddr;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

use bingle_core::api::bingle_api::{
    BingleError, NetworkEndpoint, ProgressCallback, StartOptions, UserId,
};
use bingle_core::ddb::InetSocketAddress;
use bingle_core::dtls::UdpNetworkMux;
use bingle_core::dtls::dtls_trait::{
    Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult,
};
use bingle_core::engine::{Engine, RelayState};
use bingle_core::messages::marshal::{from_json_value, to_json_value};
use bingle_core::messages::router::Router;
use bingle_core::messages::types::{DdbMessage, DdbRelaysStatusResponse, Message, RelayMessage};

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak_api_both};
use crate::util::test_util::signed_root_relay;

/// How the mock DTLS layer answers a send to `target` (every other send succeeds).
#[derive(Clone)]
enum SendMode {
    /// The send fails, like a DTLS handshake that times out.
    Fail,
    /// The send reports on `entered`, then blocks until `release` is set, like a handshake still
    /// in progress.
    Block {
        entered: Sender<()>,
        release: Arc<(Mutex<bool>, Condvar)>,
    },
}

#[derive(Default, Clone)]
struct MockDtls {
    handler: Arc<Mutex<Option<HandleMessage>>>,
    special: Arc<Mutex<Option<(SocketAddr, SendMode)>>>,
}

impl MockDtls {
    fn with_send_mode(target: SocketAddr, mode: SendMode) -> Self {
        let dtls = Self::default();
        *dtls.special.lock().expect("send mode") = Some((target, mode));
        dtls
    }
}

impl Dtls for MockDtls {
    fn start(&self, _mux: Arc<UdpNetworkMux>) -> DtlsResult<()> {
        Ok(())
    }
    fn stop(&self) -> DtlsResult<()> {
        Ok(())
    }
    fn send(&self, to: &NetworkEndpoint, data: &[u8]) -> DtlsResult<()> {
        let special = self.special.lock().expect("send mode").clone();
        if let Some((target, mode)) = special
            && to.inet_socket_address() == Some(target)
        {
            match mode {
                SendMode::Fail => return Err("mock handshake timed out".to_string()),
                SendMode::Block { entered, release } => {
                    let _ = entered.send(());
                    let (lock, cvar) = &*release;
                    let mut released = lock.lock().expect("release lock");
                    while !*released {
                        released = cvar.wait(released).expect("release wait");
                    }
                    // once released, acknowledge like any other send
                }
            }
        }
        if data.len() >= 4 && (data[0] & 0x0F) == 0x01 {
            if let Some(h) = self.handler.lock().ok().and_then(|g| g.clone()) {
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
        self.handler.lock().ok().and_then(|g| g.clone())
    }
    fn set_handle_message(&self, handler: Option<HandleMessage>) {
        let _ = self.handler.lock().map(|mut g| *g = handler);
    }
    fn set_handle_new_session(
        &self,
        _handler: Option<bingle_core::dtls::dtls_trait::HandleNewSession>,
    ) {
    }
    fn with_handle_message(self, handler: HandleMessage) -> Self {
        let _ = self.handler.lock().map(|mut g| *g = Some(handler));
        self
    }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> {
        None
    }
    fn set_handle_peer_certificate(&self, _handler: Option<HandlePeerCertificate>) {}
    fn set_public_endpoint(&self, _addr: Option<std::net::SocketAddr>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self {
        self
    }
    fn get_ca_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_ca_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self {
        self
    }
    fn get_client_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self {
        self
    }
    fn get_client_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_private_key(&self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self {
        self
    }
    fn get_server_signing_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self {
        self
    }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_private_key(&self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self {
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
    fn forget_peer(&self, _endpoint: &NetworkEndpoint) {}
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn engine_tracks_seen_endpoints() {
    let options = StartOptions::new("".into());
    // Use MockApiBoth for engine creation (requires Arc/Weak)
    let api = Arc::new(crate::util::reusable_mock_api::MockApiBoth::new());
    let engine = Engine::new_with_dtls(
        &options,
        Arc::downgrade(&api) as _,
        Box::new(MockDtls::default()),
    );

    let addr1: SocketAddr = "127.0.0.1:1111".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:2222".parse().unwrap();
    let nsk1 = NetworkEndpoint::new_direct(addr1);
    let nsk2 = NetworkEndpoint::new_direct(addr2);

    // Act: send to endpoints
    engine.send_to_peer(&nsk1, b"hello").unwrap();
    engine.send_to_peer(&nsk2, b"world").unwrap();
    // Duplicate send should not duplicate entries in the set (HashSet handles this)
    engine.send_to_peer(&nsk1, b"hello again").unwrap();

    // Assert: both unique addresses recorded
    let seen = engine.seen_endpoints_for_tests();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().any(|a| a.to_string() == addr1.to_string()));
    assert!(seen.iter().any(|a| a.to_string() == addr2.to_string()));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn failed_send_still_marks_endpoint_seen() {
    // #347: a send that fails has still put packets on the wire (e.g. a DTLS ClientHello), opening
    // a NAT path to the peer, so the endpoint must count as seen for the triangle test
    let target: SocketAddr = "203.0.113.7:4000".parse().unwrap();
    let api = Arc::new(crate::util::reusable_mock_api::MockApiBoth::new());
    let engine = Engine::new_with_dtls(
        &StartOptions::new("".into()),
        Arc::downgrade(&api) as _,
        Box::new(MockDtls::with_send_mode(target, SendMode::Fail)),
    );

    let result = engine.send_to_peer(&NetworkEndpoint::new_direct(target), b"hello");

    assert!(result.is_err(), "the mock send fails");
    let seen = engine.seen_endpoints_for_tests();
    assert!(
        seen.iter().any(|a| a.to_string() == target.to_string()),
        "a failed send is still seen: {seen:?}"
    );
}

/// A gate that a blocked mock send waits on, and the channel it reports entry on.
fn send_gate() -> (Sender<()>, Receiver<()>, Arc<(Mutex<bool>, Condvar)>) {
    let (entered_tx, entered_rx) = channel();
    (
        entered_tx,
        entered_rx,
        Arc::new((Mutex::new(false), Condvar::new())),
    )
}

fn open_gate(release: &Arc<(Mutex<bool>, Condvar)>) {
    let (lock, cvar) = &**release;
    *lock.lock().expect("release lock") = true;
    cvar.notify_all();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn in_flight_send_marks_endpoint_seen_before_it_completes() {
    // #347: a handshake still in progress has already sent its first packet
    let target: SocketAddr = "203.0.113.8:4000".parse().unwrap();
    let (entered_tx, entered_rx, release) = send_gate();
    let api = Arc::new(crate::util::reusable_mock_api::MockApiBoth::new());
    let engine = Engine::new_with_dtls(
        &StartOptions::new("".into()),
        Arc::downgrade(&api) as _,
        Box::new(MockDtls::with_send_mode(
            target,
            SendMode::Block {
                entered: entered_tx,
                release: release.clone(),
            },
        )),
    );

    std::thread::scope(|scope| {
        let sender =
            scope.spawn(|| engine.send_to_peer(&NetworkEndpoint::new_direct(target), b"hi"));
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the send reaches the transport");

        let seen = engine.seen_endpoints_for_tests();
        open_gate(&release);
        assert!(
            seen.iter().any(|a| a.to_string() == target.to_string()),
            "an in-flight send is already seen: {seen:?}"
        );
        sender
            .join()
            .expect("sender thread")
            .expect("the send completes once released");
    });
}

/// Inner mock API that answers relay-status queries, so relay selection picks `relay` without
/// touching the network.
struct RelayStatusApi {
    response: serde_json::Value,
}

impl InnerBingleApi for RelayStatusApi {
    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        if message.get("type").and_then(|value| value.as_str()) == Some("getRelaysStatus") {
            Ok(self.response.clone())
        } else {
            Err(BingleError::Other("unexpected request".to_string()))
        }
    }
}

fn relays_status_response(relay_id: &str, relay_addr: SocketAddr) -> serde_json::Value {
    to_json_value(&Message::Ddb(DdbMessage::RelaysStatusResponse(
        DdbRelaysStatusResponse {
            app: "ddb".to_string(),
            responder_state: RelayState::Available,
            epoch_id: 1,
            tree_order: 1,
            relay_ids: vec![relay_id.to_string()],
            relay_endpoints: Some(vec![InetSocketAddress::from(relay_addr)]),
            relay_states: vec![RelayState::Available],
            response_tag: None,
            text: None,
            data: None,
        },
    )))
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn triangle_test_excludes_a_relay_whose_probe_is_still_in_flight() {
    // #347 race: the preferred relay P is slow, so relay selection also probes Q, which answers
    // first and is sent TriangleTest1. P's probe is still mid-handshake but has opened a NAT path,
    // so P must be in do_not_use_endpoints or Q could pick it as the corner and fake a full cone.
    let slow_relay: SocketAddr = "198.51.100.10:7000".parse().unwrap();
    let fast_relay: SocketAddr = "198.51.100.20:7000".parse().unwrap();
    let (entered_tx, entered_rx, release) = send_gate();
    let inner: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(RelayStatusApi {
        response: relays_status_response("FAST", fast_relay),
    });
    let api = to_weak_api_both(MockApiBoth::new_with_api_override(inner));
    let engine = Engine::new_with_dtls(
        &StartOptions::new("test".into()),
        api.clone(),
        Box::new(MockDtls::with_send_mode(
            slow_relay,
            SendMode::Block {
                entered: entered_tx,
                release: release.clone(),
            },
        )),
    );
    engine.set_router(Arc::new(Router::new(api)));

    let sent: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sent_clone = sent.clone();
    engine.set_send_via_bingle(Some(Arc::new(
        move |_nsk: &NetworkEndpoint, _uid: &UserId, json: serde_json::Value| {
            sent_clone.lock().expect("sent lock").push(json);
            true
        },
    )));
    engine.test_set_relay_finder_for_inconsistent(vec![signed_root_relay("FAST", fast_relay)]);
    engine.set_last_public_addr(Some("1.2.3.4:5000".parse().unwrap()));

    std::thread::scope(|scope| {
        let probe = scope.spawn(|| {
            engine.send_to_peer(&NetworkEndpoint::new_direct(slow_relay), b"getRelaysStatus")
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the slow probe reaches the transport");

        engine.test_stun_consistent_process_with_addr("5.6.7.8:9000".parse().unwrap());
        open_gate(&release);
        probe
            .join()
            .expect("probe thread")
            .expect("the slow probe completes");
    });

    let messages = sent.lock().expect("sent lock").clone();
    let do_not_use: Vec<SocketAddr> = messages
        .into_iter()
        .find_map(|json| match from_json_value(json) {
            Ok(Message::Relay(RelayMessage::TriangleTest1(t1))) => Some(t1.do_not_use_endpoints),
            _ => None,
        })
        .expect("TriangleTest1 was sent")
        .into_iter()
        .map(|endpoint| SocketAddr::try_from(endpoint).expect("valid endpoint"))
        .collect();
    assert!(
        do_not_use.contains(&slow_relay),
        "the in-flight relay is excluded from the corner choice: {do_not_use:?}"
    );
}
