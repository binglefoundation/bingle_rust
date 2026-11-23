//! Live messages from a sender a moderator has blocked on-chain are dropped when the global
//! blocklist is on (issue #95). The blocklist is seeded, so no chain read happens.

use super::message_queue_integration_tests::mock_api;
use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::callback::MessageCallback;
use bingle_jsi::api::types::BingleMessage;
use bingle_local::api::GlobalBlocklist;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BLOCKED: &str = "MALLORYID";
const CLEAR: &str = "ALICEID";

/// Counts message callback invocations.
struct CountingCallback {
    count: Arc<Mutex<usize>>,
}

impl MessageCallback for CountingCallback {
    fn on_message(&self, _sender_id: String, _sender_handle: String, _message: BingleMessage) {
        *self.count.lock().expect("count lock") += 1;
    }
}

/// A JSI over a fresh local store with a counting host callback, honouring `blocklist` when given.
/// Returns the JSI, a clone of the store sharing its state, and the callback count.
fn jsi(
    blocklist: Option<Arc<GlobalBlocklist>>,
) -> (Arc<BingleJsiApiImpl>, BingleApiLocalImpl, Arc<Mutex<usize>>) {
    let local = BingleApiLocalImpl::new(LocalApiConfig::default());
    let boxed: Arc<Mutex<Box<dyn BingleLocalApi>>> = Arc::new(Mutex::new(Box::new(local.clone())));
    let jsi = match blocklist {
        Some(blocklist) => BingleJsiApiImpl::init_for_tests_with_global_blocklist(
            mock_api(vec![], 0),
            Some(boxed),
            blocklist,
        ),
        None => BingleJsiApiImpl::init_for_tests(mock_api(vec![], 0), Some(boxed)),
    };
    let count = Arc::new(Mutex::new(0));
    jsi.set_message_callback(Box::new(CountingCallback {
        count: count.clone(),
    }));
    (jsi, local, count)
}

/// A blocklist that already knows `BLOCKED` is blocked and `CLEAR` is not.
fn seeded() -> Arc<GlobalBlocklist> {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    blocklist.record(BLOCKED, true);
    blocklist.record(CLEAR, false);
    Arc::new(blocklist)
}

fn live(jsi: &BingleJsiApiImpl, sender: &str, text: &str) {
    jsi.receive_live_message_for_tests(
        sender.to_string(),
        sender.to_lowercase(),
        json!({ "text": text, "sent_time": 1_700_000_000_000_i64 }),
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_live_message_from_a_blocked_sender_is_dropped() {
    let (jsi, local, count) = jsi(Some(seeded()));

    live(&jsi, BLOCKED, "spam");

    assert_eq!(*count.lock().expect("count"), 0, "the host is not told");
    assert!(
        local.get_messages().expect("messages").is_empty(),
        "nothing stored"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_live_message_from_a_sender_not_blocked_is_kept() {
    let (jsi, local, count) = jsi(Some(seeded()));

    live(&jsi, CLEAR, "hello");

    assert_eq!(*count.lock().expect("count"), 1);
    assert_eq!(local.get_messages().expect("messages").len(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn without_the_blocklist_nothing_is_dropped() {
    let (jsi, local, count) = jsi(None);

    live(&jsi, BLOCKED, "hello");

    assert_eq!(*count.lock().expect("count"), 1);
    assert_eq!(local.get_messages().expect("messages").len(), 1);
}
