use crate::util::reusable_mock_api::{InnerBingleApiInternal, MockApiBoth, to_weak_api_both};
use rust_comms::api::bingle_api::NetworkEndpoint;
use rust_comms::ddb::InMemoryDdbBackend;
use rust_comms::messages::handlers::{DefaultPrintingHandler, FromStruct, MessageHandler};
use rust_comms::messages::types::{DdbMessage, DdbSignon, Message};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_on_ddb_signon_sends_response() {
    let handler = DefaultPrintingHandler;
    crate::util::test_util::init_test_logging();

    let api_weak = to_weak_api_both(MockApiBoth::new());
    let api = api_weak.upgrade().expect("upgrade");
    let router = Arc::new(rust_comms::messages::router::Router::new(api_weak.clone()));

    let sender_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234);
    let from = FromStruct::new(
        "NEWNODE".to_string() + rust_comms::protocol::ISSUER_SUFFIX,
        NetworkEndpoint::new_direct(sender_addr),
        router.clone(),
    );

    let signon = DdbSignon {
        app: "ddb".to_string(),
        start_id: "NEWNODE".to_string(),
        original_signature: Some("sig-123".to_string()),
        rippled: None,
        tag: Some("my-tag".to_string()),
        response_tag: None,
        text: None,
        data: None,
    };

    router.set_am_relay(true);

    // Set last response tag in router manually
    router.set_last_response_tag(Some("my-tag".to_string()));

    handler.on_ddb_signon(api.clone(), &from, &signon);

    let resp_json = from
        .take_responses()
        .into_iter()
        .next()
        .expect("should have outbound response");
    let resp_msg =
        rust_comms::messages::marshal::from_json_value(resp_json).expect("valid message");

    if let Message::Ddb(DdbMessage::SignonResponse(resp)) = resp_msg {
        assert_eq!(resp.app, "ddb");
        assert_eq!(resp.response_tag, Some("my-tag".to_string()));
    } else {
        panic!("Expected SignonResponse, got {:?}", resp_msg);
    }
}

struct RippleCaptureMock {
    rippled_messages: Arc<Mutex<Vec<(serde_json::Value, String)>>>,
}

impl InnerBingleApiInternal for RippleCaptureMock {
    fn ripple_message(
        &self,
        message: serde_json::Value,
        originator_id: String,
        _ddb_backend: &dyn rust_comms::ddb::DdbBackend,
    ) {
        let mut messages = self.rippled_messages.lock().unwrap();
        messages.push((message, originator_id));
    }

    fn is_relay(&self) -> bool {
        true
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_on_ddb_signon_ripples_to_peers() {
    let handler = DefaultPrintingHandler;
    crate::util::test_util::init_test_logging();

    let rippled_messages = Arc::new(Mutex::new(Vec::new()));
    let mock = RippleCaptureMock {
        rippled_messages: rippled_messages.clone(),
    };

    let api_weak = to_weak_api_both(MockApiBoth::new_with_internal_override(Arc::new(mock)));
    let api = api_weak.upgrade().expect("upgrade");
    let router = Arc::new(rust_comms::messages::router::Router::new(api_weak.clone()));

    let backend = Arc::new(Mutex::new(InMemoryDdbBackend::new()));
    router.set_ddb_backend(Some(backend));
    router.set_am_relay(true);

    let sender_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234);
    let from = FromStruct::new(
        "NEWNODE".to_string() + rust_comms::protocol::ISSUER_SUFFIX,
        NetworkEndpoint::new_direct(sender_addr),
        router.clone(),
    );

    let signon = DdbSignon {
        app: "ddb".to_string(),
        start_id: "NEWNODE".to_string(),
        original_signature: Some("sig-123".to_string()),
        rippled: None,
        tag: Some("my-tag".to_string()),
        response_tag: None,
        text: None,
        data: None,
    };

    handler.on_ddb_signon(api.clone(), &from, &signon);

    // Initial response should be sent immediately
    assert!(!from.take_responses().is_empty());

    // Ripple happens after 3 seconds. Wait 3.5s to be safe.
    std::thread::sleep(Duration::from_millis(3500));

    let messages = rippled_messages.lock().unwrap();
    assert_eq!(messages.len(), 1, "Should have rippled once");
    let (ripple_json, originator_id) = &messages[0];
    assert_eq!(originator_id, "NEWNODE");

    let ripple_msg: Message = serde_json::from_value(ripple_json.clone()).expect("valid message");
    if let Message::Ddb(DdbMessage::Signon(s)) = ripple_msg {
        assert_eq!(s.start_id, "NEWNODE");
        assert_eq!(s.rippled, Some(true));
    } else {
        panic!("Expected Signon message, got {:?}", ripple_msg);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_on_ddb_signon_does_not_ripple_if_already_rippled() {
    let handler = DefaultPrintingHandler;
    crate::util::test_util::init_test_logging();

    let rippled_messages = Arc::new(Mutex::new(Vec::new()));
    let mock = RippleCaptureMock {
        rippled_messages: rippled_messages.clone(),
    };

    let api_weak = to_weak_api_both(MockApiBoth::new_with_internal_override(Arc::new(mock)));
    let api = api_weak.upgrade().expect("upgrade");
    let router = Arc::new(rust_comms::messages::router::Router::new(api_weak.clone()));

    let backend = Arc::new(Mutex::new(InMemoryDdbBackend::new()));
    router.set_ddb_backend(Some(backend));
    router.set_am_relay(true);

    let sender_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234);
    let from = FromStruct::new(
        "NEWNODE".to_string() + rust_comms::protocol::ISSUER_SUFFIX,
        NetworkEndpoint::new_direct(sender_addr),
        router.clone(),
    );

    let signon = DdbSignon {
        app: "ddb".to_string(),
        start_id: "NEWNODE".to_string(),
        original_signature: Some("sig-123".to_string()),
        rippled: Some(true),
        tag: Some("my-tag".to_string()),
        response_tag: None,
        text: None,
        data: None,
    };

    handler.on_ddb_signon(api.clone(), &from, &signon);

    // Initial response should be sent immediately
    assert!(!from.take_responses().is_empty());

    // Wait 3.5s to ensure ripple doesn't happen
    std::thread::sleep(Duration::from_millis(3500));

    let messages = rippled_messages.lock().unwrap();
    assert_eq!(messages.len(), 0, "Should NOT have rippled");
}
