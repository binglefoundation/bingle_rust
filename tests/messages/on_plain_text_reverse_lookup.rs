use std::sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex};

use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::types::{Message, PlainTextMessage};

use crate::util::reusable_mock_api;

struct DefaultHandler;
impl MessageHandler for DefaultHandler {}

struct LookupOk;
impl reusable_mock_api::InnerBingleApi for LookupOk {
    fn handle_lookup_by_id(&self, user_id: &rust_comms::api::bingle_api::UserId) -> Option<rust_comms::api::bingle_api::Handle> {
        if user_id == "ID123" { Some("alice".to_string()) } else { None }
    }
}

struct LookupNone;
impl reusable_mock_api::InnerBingleApi for LookupNone {
    fn handle_lookup_by_id(&self, _user_id: &rust_comms::api::bingle_api::UserId) -> Option<rust_comms::api::bingle_api::Handle> { None }
}

#[cfg_attr(not(target_os = "ios"), test)]
fn on_plain_text_uses_reverse_lookup_success() {
    static CALLED: AtomicBool = AtomicBool::new(false);
    let payload: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));

    let api = reusable_mock_api::MockApiBoth::new_with_api_override(Arc::new(LookupOk));
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(reusable_mock_api::to_weak_api_both(api)));

    let called = &CALLED;
    let payload_store = payload.clone();
    let on_message: Arc<rust_comms::api::bingle_api::OnMessageHandler> = Arc::new(move |sender_id, sender_handle, msg| {
        assert_eq!(sender_id, "ID123");
        assert_eq!(sender_handle, "alice");
        called.store(true, Ordering::SeqCst);
        *payload_store.lock().unwrap() = Some(msg);
    });
    router.set_on_message(Some(on_message));

    let pt = PlainTextMessage { text: "hi".to_string(), app: None, r#type: None };
    let msg = Message::PlainText(pt);
    let handler = DefaultHandler;
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "ID123");
    });

    assert!(CALLED.load(Ordering::SeqCst), "on_message was not called");
    let got = payload.lock().unwrap().clone().expect("no payload captured");
    assert_eq!(got.get("text").and_then(|v| v.as_str()), Some("hi"));
}

#[cfg_attr(not(target_os = "ios"), test)]
fn on_plain_text_reverse_lookup_not_found_logs_and_skips_callback() {
    static CALLED: AtomicBool = AtomicBool::new(false);
    CALLED.store(false, Ordering::SeqCst);

    let api = reusable_mock_api::MockApiBoth::new_with_api_override(Arc::new(LookupNone));
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(reusable_mock_api::to_weak_api_both(api)));

    let called = &CALLED;
    let on_message: Arc<rust_comms::api::bingle_api::OnMessageHandler> = Arc::new(move |_sender_id, _sender_handle, _msg| {
        called.store(true, Ordering::SeqCst);
    });
    router.set_on_message(Some(on_message));

    let pt = PlainTextMessage { text: "hi".to_string(), app: None, r#type: None };
    let msg = Message::PlainText(pt);
    let handler = DefaultHandler;
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "UNKNOWN_ID");
    });

    assert!(!CALLED.load(Ordering::SeqCst), "on_message should not be called when reverse lookup fails");
}
