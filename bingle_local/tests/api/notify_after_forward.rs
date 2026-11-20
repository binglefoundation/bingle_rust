//! The notify nudge follows the store-and-forward Mailbox post (bingle_rust #447): a recipient is
//! alerted once while its post is outstanding and once more when the post lands, so the alert that
//! follows the post wakes it to a Mailbox that already holds the message. Uses the forwarded and
//! refused seams, so no Sidewinder node or chain is needed.

use std::sync::{Arc, Mutex};

use bingle_local::api::notify::{AlertPoster, AlertRequest};
use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig, MailboxConfig};

const TEST_MNEMONIC: &str = "square flat curtain negative three april hobby culture unit fit drip bronze cactus stage vault pluck captain nation pond pizza grief domain coin abstract path";
const UNREACHABLE: &str = "Recipient unreachable — will keep retrying";
const SENT_AT: i64 = 9_000;

/// Test poster: records the audience of every `/alert` it is handed.
#[derive(Default)]
struct RecordingPoster {
    audiences: Mutex<Vec<String>>,
}

impl RecordingPoster {
    fn audiences(&self) -> Vec<String> {
        self.audiences.lock().expect("audiences lock").clone()
    }
}

impl AlertPoster for RecordingPoster {
    fn post_alert(&self, gateway_url: &str, body: AlertRequest) {
        let _ = gateway_url;
        self.audiences
            .lock()
            .expect("audiences lock")
            .push(body.audience);
    }
}

/// A store with the nudge and a Mailbox configured, the send gate as given, and one pending message
/// to `recipients`. Returns the store, the recording poster and the message id.
fn store_with_message(
    send_gate: bool,
    recipients: &[&str],
) -> (BingleApiLocalImpl, Arc<RecordingPoster>, String) {
    let config = LocalApiConfig {
        notify_on_giveup: true,
        notify_gateway_url: Some("https://gw.example".to_string()),
        sidewinder: Some(MailboxConfig::discovered(1)),
        store_and_forward_send: send_gate,
        ..LocalApiConfig::default()
    };
    let mut api = BingleApiLocalImpl::new(config);
    let poster = Arc::new(RecordingPoster::default());
    api.set_alert_poster(poster.clone());
    api.import_keypair(TEST_MNEMONIC.to_string(), false)
        .expect("import test keypair");
    api.seed_own_handle_for_tests("sender".to_string());
    api.add_message(
        "sender".to_string(),
        recipients.iter().map(|r| r.to_string()).collect(),
        SENT_AT,
        "hello".to_string(),
        None,
    )
    .expect("add message");
    let id = api.id_of_timestamp_for_tests(SENT_AT);
    (api, poster, id)
}

fn fail_send(api: &mut BingleApiLocalImpl, id: &str) {
    api.update_message_status(id, 0.0, Some(UNREACHABLE.to_string()), None)
        .expect("update status");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_recipient_whose_post_landed_is_alerted_once() {
    let (mut api, poster, id) = store_with_message(true, &["alice"]);
    api.mark_forwarded_for_tests(&id, "alice");

    fail_send(&mut api, &id);
    fail_send(&mut api, &id);

    assert_eq!(poster.audiences(), vec!["alice"]);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn an_outstanding_recipient_is_alerted_again_when_its_post_lands() {
    let (mut api, poster, id) = store_with_message(true, &["alice"]);
    api.mark_forward_refused_for_tests(&id, "alice");

    fail_send(&mut api, &id);
    fail_send(&mut api, &id);
    assert_eq!(
        poster.audiences(),
        vec!["alice"],
        "one alert while the post is outstanding, however many retries"
    );

    api.mark_forwarded_for_tests(&id, "alice");
    fail_send(&mut api, &id);
    fail_send(&mut api, &id);
    assert_eq!(
        poster.audiences(),
        vec!["alice", "alice"],
        "one more alert once the post has landed"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn each_recipient_is_alerted_by_its_own_post() {
    let (mut api, poster, id) = store_with_message(true, &["alice", "bob"]);
    api.mark_forwarded_for_tests(&id, "alice");
    api.mark_forward_refused_for_tests(&id, "bob");

    fail_send(&mut api, &id);
    let mut first = poster.audiences();
    first.sort();
    assert_eq!(first, vec!["alice", "bob"]);

    api.mark_forwarded_for_tests(&id, "bob");
    fail_send(&mut api, &id);
    assert_eq!(
        poster.audiences()[2..],
        ["bob"],
        "only bob, whose post has just landed, is alerted again"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn with_the_send_gate_off_a_recipient_is_alerted_once() {
    let (mut api, poster, id) = store_with_message(false, &["alice"]);

    fail_send(&mut api, &id);
    fail_send(&mut api, &id);

    assert_eq!(poster.audiences(), vec!["alice"]);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_background_flush_alerts_recipients_whose_post_landed() {
    let (mut api, poster, id) = store_with_message(true, &["alice"]);
    // In flight (no failure yet), so the flush is what posts it.
    api.update_message_status(&id, 0.0, None, None)
        .expect("make pending");
    api.mark_forwarded_for_tests(&id, "alice");

    assert_eq!(api.forward_pending_to_mailbox().expect("flush"), 1);
    assert_eq!(poster.audiences(), vec!["alice"]);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_background_flush_alerts_nobody_while_a_post_is_outstanding() {
    let (mut api, poster, id) = store_with_message(true, &["alice"]);
    // In flight (no failure yet), so the flush is what posts it.
    api.update_message_status(&id, 0.0, None, None)
        .expect("make pending");
    api.mark_forward_refused_for_tests(&id, "alice");

    assert_eq!(api.forward_pending_to_mailbox().expect("flush"), 0);
    assert!(poster.audiences().is_empty());
}
