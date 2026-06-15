use rust_comms::messages::Router;
use std::sync::{Arc, Mutex};

use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::types::{PingResponse};
use rust_comms::api::bingle_api::{Handle, NetworkEndpoint, UserId};

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth};

// A mock that returns a valid handle for the expected sender id, simulating an opted-in sender.
// The handler trims trailing "." (ISSUER_SUFFIX) — "SENDER.ISSUER" does not end with "." so
// the sender_id passed to handle_lookup_by_id is "SENDER.ISSUER".
struct LookupSender;
impl InnerBingleApi for LookupSender {
    fn handle_lookup_by_id(&self, user_id: &UserId) -> Option<Handle> {
        if user_id == "SENDER.ISSUER" { Some("sender_handle".to_string()) } else { None }
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_on_ping_response_no_tag_calls_on_message() {
    let received_json = Arc::new(Mutex::new(None));
    let received_json_clone = received_json.clone();

    let router = Arc::new(Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
    ));
    // Clear any previous state for a clean test
    router.clear_for_tests();

    // Provide API to router
    router.set_bingle_api(Some(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new())));

    // Set up on_message callback
    let on_message: Arc<rust_comms::api::bingle_api::OnMessageHandler> = Arc::new(move |_sender, _handle, json| {
        let mut g = received_json_clone.lock().unwrap();
        *g = Some(json);
    });
    router.set_on_message(Some(on_message));

    let handler = rust_comms::messages::DefaultPrintingHandler;
    let from = rust_comms::messages::handlers::FromStruct::new(
        "SENDER.ISSUER".to_string(),
        NetworkEndpoint::new_direct("1.2.3.4:5678".parse().unwrap()),
        router.clone(),
    );
    let resp = PingResponse {
        app: "ping".into(),
        verified_id: "VERIFIED".into(),
        response_tag: None,
        text: Some("hello".into()),
        data: None,
    };

    // Use an API that returns a valid handle for "SENDER" (sender id after stripping the issuer suffix)
    handler.on_ping_response(Arc::new(MockApiBoth::new_with_api_override(Arc::new(LookupSender))), &from, &resp);

    let got = received_json.lock().unwrap().clone();
    assert!(got.is_some(), "on_message was not called");
    let json = got.unwrap();
    assert_eq!(json["app"], "ping");
    assert_eq!(json["verifiedId"], "VERIFIED");
    assert_eq!(json["text"], "hello");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_on_ping_response_with_tag_does_not_call_on_message() {
    let received_json = Arc::new(Mutex::new(None));
    let received_json_clone = received_json.clone();

    let router = Arc::new(Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
    ));
    router.clear_for_tests();
    router.set_bingle_api(Some(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new())));

    let on_message: Arc<rust_comms::api::bingle_api::OnMessageHandler> = Arc::new(move |_sender, _handle, json| {
        let mut g = received_json_clone.lock().unwrap();
        *g = Some(json);
    });
    router.set_on_message(Some(on_message));

    let handler = rust_comms::messages::DefaultPrintingHandler;
    let from = rust_comms::messages::handlers::FromStruct::new(
        "SENDER.ISSUER".to_string(),
        NetworkEndpoint::new_direct("1.2.3.4:5678".parse().unwrap()),
        router.clone(),
    );
    let resp = PingResponse {
        app: "ping".into(),
        verified_id: "VERIFIED".into(),
        response_tag: Some("ping_tag".to_string()),
        text: Some("hello".into()),
        data: None,
    };

    handler.on_ping_response(Arc::new(crate::util::reusable_mock_api::MockApiBoth::new()), &from, &resp);

    let got = received_json.lock().unwrap().clone();
    assert!(got.is_none(), "on_message should NOT have been called for tagged response");
}
