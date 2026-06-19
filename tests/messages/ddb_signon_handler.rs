use crate::util::reusable_mock_api::{to_weak_api_both, MockApiBoth};
use rust_comms::api::bingle_api::NetworkEndpoint;
use rust_comms::messages::handlers::{DefaultPrintingHandler, FromStruct, MessageHandler};
use rust_comms::messages::types::{DdbMessage, DdbSignon, Message};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_on_ddb_signon_sends_response() {
    let handler = DefaultPrintingHandler;
    
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

    let resp_json = from.take_responses().into_iter().next().expect("should have outbound response");
    let resp_msg = rust_comms::messages::marshal::from_json_value(resp_json).expect("valid message");
    
    if let Message::Ddb(DdbMessage::SignonResponse(resp)) = resp_msg {
        assert_eq!(resp.app, "ddb");
        assert_eq!(resp.response_tag, Some("my-tag".to_string()));
    } else {
        panic!("Expected SignonResponse, got {:?}", resp_msg);
    }
}

// TODO: test that ripple happens

// TODO: test that ripple recipient does not ripple