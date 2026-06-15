// Tests that messages from senders who are not opted into the app are silently ignored —
// no on_message callback fires regardless of message type.
//
// Two sender IDs are tested:
//   1. A plausible Algorand address format that has never opted into the app.
//   2. A raw public key string that has never appeared on the blockchain.
//
// In both cases the mock API returns None from handle_lookup_by_id, simulating the
// blockchain lookup finding no opt-in record for the sender.
//
// The test covers every variant of the Message enum (and every sub-variant of Relay,
// Ddb, Ping, Mutex, and ReportFail) via the shared sample list in util::message_mocks.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use rust_comms::api::bingle_api::UserId;
use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::router::Router;

use crate::ddb::ddb_client_lookup::test_util::init_test_logging;
use crate::util::message_mocks::all_message_samples;
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

fn assert_all_messages_ignored_for_sender(sender: &str) {
    let samples = all_message_samples();
    for (label, msg) in &samples {
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let router = make_router();
        let on_message: Arc<rust_comms::api::bingle_api::OnMessageHandler> =
            Arc::new(move |_sender_id, _sender_handle, _msg| {
                called_clone.store(true, Ordering::SeqCst);
            });
        router.set_on_message(Some(on_message));

        let handler = DefaultHandler;
        Router::with_current_router(router.clone(), || {
            router.route(&handler, msg, sender);
        });

        assert!(
            !called.load(Ordering::SeqCst),
            "on_message must not fire for non-opted-in sender '{}' with message type '{}'",
            sender,
            label
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn non_opted_in_algorand_address_all_message_types_ignored() {
    init_test_logging();
    // A plausible Algorand address that has never opted into the app.
    assert_all_messages_ignored_for_sender(
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5HFKQ",
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn non_opted_in_public_key_all_message_types_ignored() {
    init_test_logging();
    // A raw public key string that has never appeared on the blockchain.
    assert_all_messages_ignored_for_sender(
        "pubkey:0000000000000000000000000000000000000000000000000000000000000000",
    );
}
