//! Live messages from a sender a moderator has blocked on-chain are dropped when the global
//! blocklist is on (issue #95). The blocklist is seeded, so no chain read happens.

use super::message_queue_integration_tests::mock_api;
use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::{BingleJsiApiImpl, moderator_blocked_contacts};
use bingle_jsi::api::callback::MessageCallback;
use bingle_jsi::api::types::{BingleMessage, Contact, ContactSource};
use bingle_local::api::GlobalBlocklist;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
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

// Moderator-blocked contacts (issue #533): the app hides their stored messages.

fn contact(id: &str) -> Contact {
    Contact {
        handle: id.to_lowercase(),
        id: id.to_string(),
        fields: HashMap::new(),
    }
}

fn ids(contacts: &[Contact]) -> Vec<&str> {
    contacts.iter().map(|c| c.id.as_str()).collect()
}

fn add_contacts(jsi: &BingleJsiApiImpl, ids: &[&str]) {
    for id in ids {
        jsi.add_contact(id.to_lowercase(), id.to_string(), ContactSource::Manual)
            .expect("add_contact");
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn moderator_blocked_contacts_lists_only_the_blocked_contacts() {
    let (jsi, _local, _count) = jsi(Some(seeded()));
    add_contacts(&jsi, &[BLOCKED, CLEAR]);

    let blocked = jsi
        .get_moderator_blocked_contacts()
        .expect("get_moderator_blocked_contacts");

    assert_eq!(ids(&blocked), vec![BLOCKED]);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn moderator_blocked_contacts_is_empty_without_the_blocklist() {
    let (jsi, _local, _count) = jsi(None);
    add_contacts(&jsi, &[BLOCKED, CLEAR]);

    let blocked = jsi
        .get_moderator_blocked_contacts()
        .expect("get_moderator_blocked_contacts");

    assert!(blocked.is_empty());
}

#[test]
fn a_contact_the_cache_does_not_know_is_read_once_then_cached() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    let reads = AtomicUsize::new(0);
    let lookup = |address: &str| {
        reads.fetch_add(1, Ordering::SeqCst);
        Ok(Some(address == BLOCKED))
    };

    let first = moderator_blocked_contacts(
        Some(&blocklist),
        vec![contact(BLOCKED), contact(CLEAR)],
        lookup,
    );
    assert_eq!(ids(&first), vec![BLOCKED]);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        2,
        "one read per unknown contact"
    );

    let second = moderator_blocked_contacts(
        Some(&blocklist),
        vec![contact(BLOCKED), contact(CLEAR)],
        lookup,
    );
    assert_eq!(ids(&second), vec![BLOCKED]);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        2,
        "known answers come from the cache"
    );
}

#[test]
fn every_unknown_contact_is_read_in_batches() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    let reads = AtomicUsize::new(0);
    let contacts: Vec<Contact> = (0..10).map(|i| contact(&format!("USER{i}"))).collect();

    let blocked = moderator_blocked_contacts(Some(&blocklist), contacts, |address: &str| {
        reads.fetch_add(1, Ordering::SeqCst);
        Ok(Some(address == "USER7"))
    });

    assert_eq!(ids(&blocked), vec!["USER7"]);
    assert_eq!(reads.load(Ordering::SeqCst), 10);
}

#[test]
fn a_failed_read_is_not_blocked_and_is_tried_again() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    let reads = AtomicUsize::new(0);
    let failing = |_: &str| {
        reads.fetch_add(1, Ordering::SeqCst);
        Err("algod unreachable".to_string())
    };

    assert!(
        moderator_blocked_contacts(Some(&blocklist), vec![contact(BLOCKED)], failing).is_empty()
    );
    assert!(
        moderator_blocked_contacts(Some(&blocklist), vec![contact(BLOCKED)], failing).is_empty()
    );
    assert_eq!(reads.load(Ordering::SeqCst), 2, "a failure is not cached");
}

#[test]
fn a_contact_not_opted_in_is_not_blocked() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));

    let blocked =
        moderator_blocked_contacts(Some(&blocklist), vec![contact(BLOCKED)], |_: &str| Ok(None));

    assert!(blocked.is_empty());
}

#[test]
fn without_a_blocklist_no_contact_is_read() {
    let reads = AtomicUsize::new(0);

    let blocked = moderator_blocked_contacts(None, vec![contact(BLOCKED)], |_: &str| {
        reads.fetch_add(1, Ordering::SeqCst);
        Ok(Some(true))
    });

    assert!(blocked.is_empty());
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}
