use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bingle_core::api::bingle_api::{BingleError, NetworkEndpoint, ProgressCallback, UserId};
use bingle_core::messages::handlers::{
    DefaultPrintingHandler, FromStruct, MessageHandler, usable_endpoint, usable_endpoints,
};
use bingle_core::messages::types::{InetSocketAddress, RelayTriangleTest1, RelayTriangleTest2};

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak_api_both};

struct SendCapture {
    sent: Mutex<Vec<(NetworkEndpoint, serde_json::Value)>>,
}

impl InnerBingleApi for SendCapture {
    fn send_message_to_network(
        &self,
        nsk: &NetworkEndpoint,
        _user_id: &UserId,
        message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        self.sent.lock().unwrap().push((nsk.clone(), message));
        Ok(true)
    }
}

fn inet(host: &str, port: u16) -> InetSocketAddress {
    InetSocketAddress {
        host: host.to_string(),
        port,
    }
}

/// A capturing API plus a `FromStruct` for calling a handler method directly.
fn setup() -> (
    Arc<SendCapture>,
    Arc<dyn bingle_core::api::bingle_api::BingleApiBoth>,
    FromStruct,
) {
    let capture = Arc::new(SendCapture {
        sent: Mutex::new(Vec::new()),
    });
    let api: Arc<dyn bingle_core::api::bingle_api::BingleApiBoth> =
        Arc::new(MockApiBoth::new_with_api_override(capture.clone()));
    let router = Arc::new(bingle_core::messages::router::Router::new(
        to_weak_api_both(MockApiBoth::new()),
    ));
    let from = FromStruct::new(
        "SENDER".to_string(),
        NetworkEndpoint::new_direct("127.0.0.1:40000".parse().unwrap()),
        router,
    );
    (capture, api, from)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn usable_endpoint_accepts_ipv4() {
    let addr: SocketAddr = "192.0.2.7:3478".parse().unwrap();
    assert_eq!(usable_endpoint(&inet("192.0.2.7", 3478)), Some(addr));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn usable_endpoint_rejects_what_it_cannot_send_to() {
    for host in [
        "2001:db8::1",
        "[2001:db8::1]",
        "relay.example.com",
        "",
        "not an address",
    ] {
        assert_eq!(usable_endpoint(&inet(host, 3478)), None, "host {:?}", host);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn usable_endpoints_skips_unusable_entries_and_keeps_order() {
    let got = usable_endpoints(&[
        inet("192.0.2.1", 1),
        inet("2001:db8::1", 2),
        inet("garbage", 3),
        inet("192.0.2.4", 4),
    ]);
    let want: Vec<SocketAddr> = vec![
        "192.0.2.1:1".parse().unwrap(),
        "192.0.2.4:4".parse().unwrap(),
    ];
    assert_eq!(got, want);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn triangle_test2_sends_test3_to_a_usable_checking_endpoint() {
    let (capture, api, from) = setup();
    let t2 = RelayTriangleTest2 {
        app: None,
        checking_id: "CHECKED".to_string(),
        checking_endpoint: inet("192.0.2.9", 5555),
    };

    DefaultPrintingHandler.on_triangle_test2(api, &from, &t2);

    let sent = capture.sent.lock().unwrap();
    assert_eq!(sent.len(), 1, "one TriangleTest3");
    assert_eq!(
        sent[0].0.inet_socket_address(),
        Some("192.0.2.9:5555".parse().unwrap())
    );
    assert_eq!(
        sent[0].1.get("type").and_then(|v| v.as_str()),
        Some("TriangleTest3")
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn triangle_test2_with_unusable_checking_endpoint_is_dropped() {
    for host in ["2001:db8::1", "garbage"] {
        let (capture, api, from) = setup();
        let t2 = RelayTriangleTest2 {
            app: None,
            checking_id: "CHECKED".to_string(),
            checking_endpoint: inet(host, 5555),
        };

        DefaultPrintingHandler.on_triangle_test2(api, &from, &t2);

        assert!(capture.sent.lock().unwrap().is_empty(), "host {:?}", host);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn triangle_test1_with_unusable_checking_endpoint_is_dropped() {
    let (capture, api, from) = setup();
    let t1 = RelayTriangleTest1 {
        app: None,
        checking_endpoint: inet("2001:db8::1", 5555),
        do_not_use_endpoints: Vec::new(),
        tag: Some("t".to_string()),
    };

    DefaultPrintingHandler.on_triangle_test1(api, &from, &t1);

    // A usable endpoint would hand off to a worker thread that sends TriangleTest2 and a response;
    // an unusable one returns before that, so nothing is ever sent.
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(capture.sent.lock().unwrap().is_empty());
}
