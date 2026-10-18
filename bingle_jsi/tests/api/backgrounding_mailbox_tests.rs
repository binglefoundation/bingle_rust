//! Tests for handing an undelivered message to the Mailbox when the app goes to the background
//! (issue #352). On iOS the app is suspended, and may then be killed, soon after `backgrounding`; a
//! message whose first send has not yet failed has never been offered to the Mailbox, so it is lost
//! unless backgrounding forwards it.
//!
//! Each test runs the JSI over a real local store with store-and-forward send on and the Sidewinder
//! identity hold-off seam set, so a store-and-forward attempt records its reason on the message
//! without a live node. A recorded `store_and_forward_failure` therefore shows that an attempt was
//! made. The control tests check that a failed send makes that attempt today.
//!
//! The backgrounding tests are known failures until #352 is fixed: each is marked `should_panic`
//! on its own assertion message, so it passes while the bug stands and fails once it is fixed, as
//! a prompt to remove the marker.

use super::message_queue_integration_tests::mock_api;
use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_local::api::MailboxConfig;
use bingle_local::api::bingle_local_api::{BingleLocalApi, Message};
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Which JSI send path drives the pending message.
#[derive(Clone, Copy)]
enum SendPath {
    /// The scheduler and worker loop the app uses (`send_pending_messages` unset).
    Legacy,
    /// The shared pending-message sender (`send_pending_messages` on, issue #283).
    Shared,
}

/// A local store with a Mailbox configured, store-and-forward send on, and the identity hold-off
/// set (checked before the keypair, so none is imported and `start` makes no chain read), holding
/// one pending message to `bob` queued at `timestamp`. Returns the store and the id.
fn store_with_pending_message(timestamp: i64) -> (BingleApiLocalImpl, String) {
    let config = LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_send: true,
        ..LocalApiConfig::default()
    };
    let local = BingleApiLocalImpl::new(config);
    local
        .add_message_shared(
            "testuser".to_string(),
            vec!["bob".to_string()],
            timestamp,
            "Hello".to_string(),
            None,
        )
        .expect("add message");
    let id = local.id_of_timestamp_for_tests(timestamp);
    local
        .update_message_status_shared(&id, 0.0, None, None)
        .expect("make pending");
    local.mark_mailbox_identity_refused_for_tests(Instant::now());
    (local, id)
}

/// A JSI over `local` on the given send path. The local store is a clone sharing `local`'s state.
fn jsi_over(
    path: SendPath,
    api: Arc<super::message_queue_integration_tests::MockBingleApi>,
    local: &BingleApiLocalImpl,
) -> Arc<BingleJsiApiImpl> {
    match path {
        SendPath::Legacy => {
            let boxed: Arc<Mutex<Box<dyn BingleLocalApi>>> =
                Arc::new(Mutex::new(Box::new(local.clone())));
            BingleJsiApiImpl::init_for_tests(api, Some(boxed))
        }
        SendPath::Shared => {
            BingleJsiApiImpl::init_for_tests_with_pending_sender(api, local.clone())
        }
    }
}

fn stored(local: &BingleApiLocalImpl, id: &str) -> Message {
    local
        .get_messages()
        .expect("messages")
        .into_iter()
        .find(|m| m.id == id)
        .expect("message present")
}

/// Poll until `done` holds or `timeout` passes.
fn wait_until(timeout: Duration, done: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    done()
}

/// Whether a store-and-forward attempt has been made for the message.
fn offered_to_mailbox(local: &BingleApiLocalImpl, id: &str) -> bool {
    stored(local, id).store_and_forward_failure.is_some()
        || local.forwarded_for_tests().iter().any(|(m, _)| m == id)
}

/// Control: a send that fails is offered to the Mailbox, so the observation below is sound.
fn a_failed_send_is_offered_to_the_mailbox(path: SendPath, timestamp: i64) {
    let (local, id) = store_with_pending_message(timestamp);
    let jsi = jsi_over(path, mock_api(vec![], 1), &local);

    jsi.start().expect("start");
    jsi.api_for_tests()
        .notify_listening(true, bingle_core::engine::NatType::Restricted);

    assert!(
        wait_until(Duration::from_secs(5), || offered_to_mailbox(&local, &id)),
        "a failed send makes a store-and-forward attempt: {:?}",
        stored(&local, &id)
    );
    jsi.stop().expect("stop");
}

/// The send is still in flight (no failure yet) when the app backgrounds.
fn a_send_in_flight_at_backgrounding_is_offered_to_the_mailbox(path: SendPath, timestamp: i64) {
    let (local, id) = store_with_pending_message(timestamp);
    let api = mock_api(vec![], 0);
    api.hold_sends.store(true, Ordering::SeqCst);
    let jsi = jsi_over(path, api.clone(), &local);

    jsi.start().expect("start");
    jsi.api_for_tests()
        .notify_listening(true, bingle_core::engine::NatType::Restricted);
    assert!(
        wait_until(Duration::from_secs(5), || !api
            .payloads
            .lock()
            .expect("payloads")
            .is_empty()),
        "the send starts"
    );

    jsi.backgrounding();

    let offered = wait_until(Duration::from_secs(3), || offered_to_mailbox(&local, &id));
    let m = stored(&local, &id);
    api.hold_sends.store(false, Ordering::SeqCst);
    jsi.stop().expect("stop");
    assert!(
        offered,
        "backgrounding offers the in-flight message to the Mailbox: {m:?}"
    );
}

/// The send has not started (the node is not listening) when the app backgrounds.
fn a_send_not_yet_tried_at_backgrounding_is_offered_to_the_mailbox(path: SendPath, timestamp: i64) {
    let (local, id) = store_with_pending_message(timestamp);
    let api = mock_api(vec![], 0);
    let jsi = jsi_over(path, api.clone(), &local);

    jsi.start().expect("start");
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        api.payloads.lock().expect("payloads").is_empty(),
        "nothing is sent before the node is listening"
    );

    jsi.backgrounding();

    let offered = wait_until(Duration::from_secs(3), || offered_to_mailbox(&local, &id));
    let m = stored(&local, &id);
    jsi.stop().expect("stop");
    assert!(
        offered,
        "backgrounding offers the unsent message to the Mailbox: {m:?}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn legacy_loop_offers_a_failed_send_to_the_mailbox() {
    a_failed_send_is_offered_to_the_mailbox(SendPath::Legacy, 35_201);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn shared_sender_offers_a_failed_send_to_the_mailbox() {
    a_failed_send_is_offered_to_the_mailbox(SendPath::Shared, 35_202);
}

#[test]
#[cfg(not(target_os = "ios"))]
// known failure (#352): remove the should_panic when backgrounding hands the message off
#[should_panic(expected = "backgrounding offers the in-flight message")]
fn legacy_loop_backgrounding_offers_an_in_flight_send_to_the_mailbox() {
    a_send_in_flight_at_backgrounding_is_offered_to_the_mailbox(SendPath::Legacy, 35_203);
}

#[test]
#[cfg(not(target_os = "ios"))]
// known failure (#352): remove the should_panic when backgrounding hands the message off
#[should_panic(expected = "backgrounding offers the in-flight message")]
fn shared_sender_backgrounding_offers_an_in_flight_send_to_the_mailbox() {
    a_send_in_flight_at_backgrounding_is_offered_to_the_mailbox(SendPath::Shared, 35_204);
}

#[test]
#[cfg(not(target_os = "ios"))]
// known failure (#352): remove the should_panic when backgrounding hands the message off
#[should_panic(expected = "backgrounding offers the unsent message")]
fn legacy_loop_backgrounding_offers_an_untried_send_to_the_mailbox() {
    a_send_not_yet_tried_at_backgrounding_is_offered_to_the_mailbox(SendPath::Legacy, 35_205);
}

#[test]
#[cfg(not(target_os = "ios"))]
// known failure (#352): remove the should_panic when backgrounding hands the message off
#[should_panic(expected = "backgrounding offers the unsent message")]
fn shared_sender_backgrounding_offers_an_untried_send_to_the_mailbox() {
    a_send_not_yet_tried_at_backgrounding_is_offered_to_the_mailbox(SendPath::Shared, 35_206);
}
