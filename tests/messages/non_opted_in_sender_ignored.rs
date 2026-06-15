// Tests that messages from senders who are not opted into the app are silently ignored —
// no on_message callback fires regardless of message type.
//
// Two sender IDs are tested:
//   1. A plausible Algorand address format that has never opted into the app.
//   2. A raw public key string that has never appeared on the blockchain.
//
// In both cases the mock API returns None from handle_lookup_by_id, simulating the
// blockchain lookup finding no opt-in record for the sender.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use rust_comms::api::bingle_api::UserId;
use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::router::Router;
use rust_comms::messages::types::{Message, PingMessage, PingPing, PlainTextMessage};
use crate::ddb::ddb_client_lookup::test_util::init_test_logging;
use crate::util::reusable_mock_api;

// A mock inner API that always returns None from handle_lookup_by_id,
// simulating a sender that is not opted into the app.
struct NotOptedIn;
impl reusable_mock_api::InnerBingleApi for NotOptedIn {
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<rust_comms::api::bingle_api::Handle> {
        None
    }
}

struct DefaultHandler;
impl MessageHandler for DefaultHandler {}

fn make_router() -> Arc<Router> {
    let api = reusable_mock_api::MockApiBoth::new_with_api_override(Arc::new(NotOptedIn));
    Arc::new(Router::new(reusable_mock_api::to_weak_api_both(api)))
}

// --- plain text message ---

#[test]
#[cfg(not(target_os = "ios"))]
fn non_opted_in_algorand_address_plain_text_ignored() {
    init_test_logging();
    
    static CALLED: AtomicBool = AtomicBool::new(false);
    CALLED.store(false, Ordering::SeqCst);

    let router = make_router();
    let on_message: Arc<rust_comms::api::bingle_api::OnMessageHandler> =
        Arc::new(move |_sender_id, _sender_handle, _msg| {
            CALLED.store(true, Ordering::SeqCst);
        });
    router.set_on_message(Some(on_message));

    // A plausible Algorand address that has never opted into the app.
    let sender = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5HFKQ";
    let msg = Message::PlainText(PlainTextMessage {
        text: "hello".to_string(),
        app: None,
        r#type: None,
        cipher_suite: None,
    });
    let handler = DefaultHandler;
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, sender);
    });

    assert!(
        !CALLED.load(Ordering::SeqCst),
        "on_message must not fire for a non-opted-in Algorand address (plain text)"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn non_opted_in_public_key_plain_text_ignored() {
    static CALLED: AtomicBool = AtomicBool::new(false);
    CALLED.store(false, Ordering::SeqCst);

    let router = make_router();
    let on_message: Arc<rust_comms::api::bingle_api::OnMessageHandler> =
        Arc::new(move |_sender_id, _sender_handle, _msg| {
            CALLED.store(true, Ordering::SeqCst);
        });
    router.set_on_message(Some(on_message));

    // A raw public key string that has never appeared on the blockchain.
    let sender = "pubkey:0000000000000000000000000000000000000000000000000000000000000000";
    let msg = Message::PlainText(PlainTextMessage {
        text: "hello".to_string(),
        app: None,
        r#type: None,
        cipher_suite: None,
    });
    let handler = DefaultHandler;
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, sender);
    });

    assert!(
        !CALLED.load(Ordering::SeqCst),
        "on_message must not fire for a public key not on the blockchain (plain text)"
    );
}

// --- ping message ---

#[test]
#[cfg(not(target_os = "ios"))]
fn non_opted_in_algorand_address_ping_ignored() {
    static CALLED: AtomicBool = AtomicBool::new(false);
    CALLED.store(false, Ordering::SeqCst);

    let router = make_router();
    let on_message: Arc<rust_comms::api::bingle_api::OnMessageHandler> =
        Arc::new(move |_sender_id, _sender_handle, _msg| {
            CALLED.store(true, Ordering::SeqCst);
        });
    router.set_on_message(Some(on_message));

    let sender = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5HFKQ";
    let msg = Message::Ping(PingMessage::Ping(PingPing {
        app: "ping".to_string(),
        tag: None,
        response_tag: None,
        text: None,
        data: None,
    }));
    let handler = DefaultHandler;
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, sender);
    });

    assert!(
        !CALLED.load(Ordering::SeqCst),
        "on_message must not fire for a non-opted-in Algorand address (ping)"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn non_opted_in_public_key_ping_ignored() {
    static CALLED: AtomicBool = AtomicBool::new(false);
    CALLED.store(false, Ordering::SeqCst);

    let router = make_router();
    let on_message: Arc<rust_comms::api::bingle_api::OnMessageHandler> =
        Arc::new(move |_sender_id, _sender_handle, _msg| {
            CALLED.store(true, Ordering::SeqCst);
        });
    router.set_on_message(Some(on_message));

    let sender = "pubkey:0000000000000000000000000000000000000000000000000000000000000000";
    let msg = Message::Ping(PingMessage::Ping(PingPing {
        app: "ping".to_string(),
        tag: None,
        response_tag: None,
        text: None,
        data: None,
    }));
    let handler = DefaultHandler;
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, sender);
    });

    assert!(
        !CALLED.load(Ordering::SeqCst),
        "on_message must not fire for a public key not on the blockchain (ping)"
    );
}
