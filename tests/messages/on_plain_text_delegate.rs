#![cfg(not(target_os = "ios"))]

use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

use rust_comms::messages::handlers::{DefaultPrintingHandler, MessageHandler};
use rust_comms::messages::router::route;
use rust_comms::messages::types::{Message, PlainTextMessage};

#[test]
fn on_plain_text_delegates_to_bingle_on_message() {
    // Install a global on_message handler that flips a flag when invoked and captures payload
    static CALLED: AtomicBool = AtomicBool::new(false);

    let received = Arc::new(Mutex::new(None::<serde_json::Value>));
    let recv_clone = received.clone();

    rust_comms::api::bingle_api_impl::global_on_message_set(Some(Arc::new(move |_sender, handle, msg| {
        assert_eq!(handle, "from-handle");
        CALLED.store(true, Ordering::SeqCst);
        *recv_clone.lock().unwrap() = Some(msg);
    })));

    // Build a PlainText message and route it through DefaultPrintingHandler
    let pt = PlainTextMessage { text: "Hello".to_string(), app: None, r#type: None };
    let msg = Message::PlainText(pt.clone());

    let handler = DefaultPrintingHandler;
    route(&handler, &msg, "from-handle");

    assert!(CALLED.load(Ordering::SeqCst), "global on_message was not called by on_plain_text");

    let got = received.lock().unwrap().clone().expect("no payload captured");
    // Expect the JSON to include the text field
    assert_eq!(got.get("text").and_then(|v| v.as_str()), Some("Hello"));
}
