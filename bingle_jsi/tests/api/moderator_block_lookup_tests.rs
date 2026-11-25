//! Moderator-blocked users are left out of handle lookups, and messages to them are refused, while
//! the global blocklist is on (issue #539). The blocklist is seeded, so the tests need no chain.

use bingle_core::api::bingle_api::{
    BingleApi, BingleApiInternal, BingleError, Handle, HandleMatch, OnConnectHandler,
    OnListeningHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId,
};
use bingle_core::api::network_endpoint::NetworkEndpoint;
use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::{
    BingleJsiApiImpl, partition_moderator_blocked, without_moderator_blocked_matches,
};
use bingle_jsi::api::error::BingleJsiError;
use bingle_jsi::api::types::{BingleMessage, HandleLookupMatch};
use bingle_local::api::GlobalBlocklist;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BLOCKED: &str = "MALLORYADDRESS";
const CLEAR: &str = "ALICEADDRESS";
/// An address the seeded blocklist knows nothing about.
const UNKNOWN: &str = "CAROLADDRESS";

/// An engine whose handles resolve from a fixed table, recording the sends it is asked for and
/// counting handle lookups.
struct DirectoryApi {
    handles: HashMap<String, String>,
    sends: Mutex<Vec<String>>,
    lookups: AtomicUsize,
}

impl DirectoryApi {
    fn new() -> Arc<Self> {
        let handles = [("mallory", BLOCKED), ("alice", CLEAR), ("carol", UNKNOWN)]
            .into_iter()
            .map(|(h, a)| (h.to_string(), a.to_string()))
            .collect();
        Arc::new(Self {
            handles,
            sends: Mutex::new(Vec::new()),
            lookups: AtomicUsize::new(0),
        })
    }

    fn sends(&self) -> Vec<String> {
        self.sends.lock().expect("sends lock").clone()
    }

    fn record_send(&self, handle: &str) {
        self.sends
            .lock()
            .expect("sends lock")
            .push(handle.to_string());
    }
}

impl BingleApiInternal for DirectoryApi {
    fn get_relay_state(&self) -> String {
        "off".to_string()
    }
    fn notify_listening(&self, _: bool, _: bingle_core::engine::NatType) {}
}

impl BingleApi for DirectoryApi {
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> {
        Some("MEADDRESS".to_string())
    }
    fn get_user_id(&self) -> Option<String> {
        Some("MEADDRESS".to_string())
    }
    fn get_handle(&self) -> Option<String> {
        Some("me".to_string())
    }
    fn get_app_id(&self) -> Option<u64> {
        None
    }
    fn get_algo_provider_config(&self) -> Option<algo_ops::AlgoChainConfig> {
        None
    }
    fn start(&self, _: &StartOptions) -> Result<(), BingleError> {
        Ok(())
    }
    fn stop(&self) {}
    fn network_change(&self) {}
    fn list_all_relays(&self, _: bool) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        vec![]
    }
    fn handle_lookup(&self, handle: &Handle) -> Result<Option<UserId>, BingleError> {
        self.lookups.fetch_add(1, Ordering::SeqCst);
        Ok(self.handles.get(handle).cloned())
    }
    fn handle_lookup_partial(
        &self,
        prefix: &Handle,
    ) -> Result<Option<(UserId, Handle)>, BingleError> {
        Ok(self
            .handles
            .iter()
            .find(|(h, _)| h.starts_with(prefix.as_str()))
            .map(|(h, a)| (a.clone(), h.clone())))
    }
    fn handle_lookup_matches(
        &self,
        _prefix: &Handle,
        limit: usize,
    ) -> Result<Vec<HandleMatch>, BingleError> {
        // Every handle matches, in a fixed order.
        Ok(["alice", "mallory", "carol"]
            .into_iter()
            .take(limit)
            .map(|h| HandleMatch {
                id: self.handles[h].clone(),
                canonical_handle: h.to_string(),
                exact: false,
            })
            .collect())
    }
    fn handle_lookup_by_id(&self, _: &UserId) -> Option<Handle> {
        None
    }
    fn send_message_to_id(
        &self,
        _: &UserId,
        _: serde_json::Value,
        _: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_handle(
        &self,
        handle: &Handle,
        _: serde_json::Value,
        _: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        self.record_send(handle);
        Ok(true)
    }
    fn send_message_to_network(
        &self,
        _: &NetworkEndpoint,
        _: &UserId,
        _: serde_json::Value,
        _: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_id_with_response(
        &self,
        _: &UserId,
        _: serde_json::Value,
        _: Option<Arc<ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        Ok(serde_json::json!({}))
    }
    fn send_message_to_handle_with_response(
        &self,
        handle: &Handle,
        _: serde_json::Value,
        _: Option<Arc<ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        self.record_send(handle);
        Ok(serde_json::json!({}))
    }
    fn send_message_to_network_with_response(
        &self,
        _: &NetworkEndpoint,
        _: &UserId,
        _: serde_json::Value,
        _: Option<Arc<ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        Ok(serde_json::json!({}))
    }
    fn set_on_message(&self, _: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&self, _: Option<Arc<OnConnectHandler>>) {}
    fn set_on_listening(&self, _: Option<Arc<OnListeningHandler>>) {}
}

/// A blocklist that already knows `BLOCKED` is blocked and `CLEAR` is not.
fn seeded() -> Arc<GlobalBlocklist> {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    blocklist.record(BLOCKED, true);
    blocklist.record(CLEAR, false);
    Arc::new(blocklist)
}

/// A JSI over `api` and a fresh local store, honouring `blocklist` when given. Returns the JSI and a
/// clone of the store sharing its state.
fn jsi(
    api: Arc<DirectoryApi>,
    blocklist: Option<Arc<GlobalBlocklist>>,
) -> (Arc<BingleJsiApiImpl>, BingleApiLocalImpl) {
    let local = BingleApiLocalImpl::new(LocalApiConfig::default());
    // A registered sender, so messages can be queued.
    local.seed_own_handle_for_tests("me".to_string());
    let boxed: Arc<Mutex<Box<dyn BingleLocalApi>>> = Arc::new(Mutex::new(Box::new(local.clone())));
    let jsi = match blocklist {
        Some(blocklist) => {
            BingleJsiApiImpl::init_for_tests_with_global_blocklist(api, Some(boxed), blocklist)
        }
        None => BingleJsiApiImpl::init_for_tests(api, Some(boxed)),
    };
    (jsi, local)
}

fn message() -> BingleMessage {
    BingleMessage {
        app: None,
        r#type: None,
        tag: None,
        response_tag: None,
        text: Some("hello".to_string()),
        data: None,
        cipher_suite: None,
        id: None,
        delivery_route: None,
        delivered_time: None,
    }
}

fn matched_handles(jsi: &BingleJsiApiImpl) -> Vec<String> {
    jsi.handle_lookup_matches("a".to_string(), 10)
        .expect("handle_lookup_matches")
        .into_iter()
        .map(|m| m.canonical_handle)
        .collect()
}

fn is_invalid_request<T: std::fmt::Debug>(result: &Result<T, BingleJsiError>) -> bool {
    matches!(result, Err(BingleJsiError::InvalidRequest { .. }))
}

fn is_not_found<T: std::fmt::Debug>(result: &Result<T, BingleJsiError>) -> bool {
    matches!(result, Err(BingleJsiError::NotFound { .. }))
}

// With the blocklist on.

#[test]
fn a_moderator_blocked_user_is_left_out_of_handle_search() {
    let (jsi, _local) = jsi(DirectoryApi::new(), Some(seeded()));
    assert_eq!(matched_handles(&jsi), vec!["alice", "carol"]);
}

#[test]
fn a_moderator_blocked_user_is_not_found_by_handle_lookup() {
    let (jsi, _local) = jsi(DirectoryApi::new(), Some(seeded()));

    let blocked = jsi.handle_lookup("mallory".to_string());
    assert!(is_not_found(&blocked), "got {blocked:?}");
    assert_eq!(
        jsi.handle_lookup("alice".to_string()).expect("alice"),
        CLEAR
    );

    let partial = jsi.handle_lookup_partial("mal".to_string());
    assert!(is_not_found(&partial), "got {partial:?}");
}

#[test]
fn sending_to_a_moderator_blocked_user_is_refused_and_sends_nothing() {
    let api = DirectoryApi::new();
    let (jsi, _local) = jsi(api.clone(), Some(seeded()));

    let result = jsi.send_message_to_handle("mallory".to_string(), message());
    assert!(is_invalid_request(&result), "got {result:?}");
    let result = jsi.send_message_to_handle_with_response("mallory".to_string(), message());
    assert!(is_invalid_request(&result), "got {result:?}");
    assert!(api.sends().is_empty(), "nothing sent");

    assert!(
        jsi.send_message_to_handle("alice".to_string(), message())
            .expect("send to alice")
    );
    assert_eq!(api.sends(), vec!["alice"]);
}

#[test]
fn queuing_to_a_moderator_blocked_user_is_refused_and_queues_nothing() {
    let (jsi, local) = jsi(DirectoryApi::new(), Some(seeded()));

    let result = jsi.queue_message(vec!["mallory".to_string()], "hello".to_string());
    assert!(is_invalid_request(&result), "got {result:?}");
    // One blocked recipient among several refuses the whole message.
    let result = jsi.queue_message(
        vec!["alice".to_string(), "mallory".to_string()],
        "hello".to_string(),
    );
    assert!(is_invalid_request(&result), "got {result:?}");
    assert!(
        local.get_messages().expect("messages").is_empty(),
        "nothing queued"
    );

    jsi.queue_message(vec!["alice".to_string()], "hello".to_string())
        .expect("queue to alice");
    assert_eq!(local.get_messages().expect("messages").len(), 1);
}

#[test]
fn an_address_the_cache_does_not_know_is_read_once_then_cached() {
    let blocklist = seeded();
    let (jsi, _local) = jsi(DirectoryApi::new(), Some(blocklist.clone()));

    // No Bingle app is configured, so the read answers "not opted in", which is not blocked.
    assert_eq!(blocklist.cached(UNKNOWN), None);
    assert_eq!(matched_handles(&jsi), vec!["alice", "carol"]);
    assert_eq!(
        blocklist.cached(UNKNOWN),
        Some(false),
        "the answer is cached"
    );
}

// With the blocklist off or absent: as before, with no reads.

#[test]
fn with_the_blocklist_switched_off_lookups_and_sends_are_unchanged() {
    let api = DirectoryApi::new();
    let blocklist = seeded();
    blocklist.set_enabled(false);
    let (jsi, local) = jsi(api.clone(), Some(blocklist.clone()));

    assert_eq!(matched_handles(&jsi), vec!["alice", "mallory", "carol"]);
    assert_eq!(
        jsi.handle_lookup("mallory".to_string()).expect("mallory"),
        BLOCKED
    );
    let lookups_before_sends = api.lookups.load(Ordering::SeqCst);
    assert!(
        jsi.send_message_to_handle("mallory".to_string(), message())
            .expect("send to mallory")
    );
    jsi.queue_message(vec!["mallory".to_string()], "hello".to_string())
        .expect("queue to mallory");

    assert_eq!(api.sends(), vec!["mallory"]);
    assert_eq!(local.get_messages().expect("messages").len(), 1);
    assert_eq!(
        api.lookups.load(Ordering::SeqCst),
        lookups_before_sends,
        "the sends don't resolve the handle to check it"
    );
    assert_eq!(blocklist.cached(UNKNOWN), None, "nothing was read");
}

#[test]
fn without_a_blocklist_lookups_and_sends_are_unchanged() {
    let api = DirectoryApi::new();
    let (jsi, local) = jsi(api.clone(), None);

    assert_eq!(matched_handles(&jsi), vec!["alice", "mallory", "carol"]);
    assert_eq!(
        jsi.handle_lookup("mallory".to_string()).expect("mallory"),
        BLOCKED
    );
    assert!(
        jsi.send_message_to_handle("mallory".to_string(), message())
            .expect("send to mallory")
    );
    jsi.queue_message(vec!["mallory".to_string()], "hello".to_string())
        .expect("queue to mallory");
    assert_eq!(api.sends(), vec!["mallory"]);
    assert_eq!(local.get_messages().expect("messages").len(), 1);
}

// The batched read behind the filtering.

fn lookup_match(id: &str) -> HandleLookupMatch {
    HandleLookupMatch {
        id: id.to_string(),
        canonical_handle: id.to_lowercase(),
        exact: false,
    }
}

fn no_chain_read(_: &str) -> Result<Option<bool>, String> {
    panic!("no chain read expected")
}

#[test]
fn search_results_are_read_once_then_answered_from_the_cache() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    let reads = AtomicUsize::new(0);
    let lookup = |address: &str| {
        reads.fetch_add(1, Ordering::SeqCst);
        Ok(Some(address == BLOCKED))
    };
    let results = || vec![lookup_match(CLEAR), lookup_match(BLOCKED)];

    let first = without_moderator_blocked_matches(Some(&blocklist), results(), lookup);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id, CLEAR);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        2,
        "one read per unknown match"
    );

    let second = without_moderator_blocked_matches(Some(&blocklist), results(), lookup);
    assert_eq!(second.len(), 1);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        2,
        "known answers come from the cache"
    );
}

#[test]
fn search_results_are_not_read_with_the_blocklist_off_or_absent() {
    let blocklist = GlobalBlocklist::with_enabled(Duration::from_secs(600), false);
    let results = || vec![lookup_match(CLEAR), lookup_match(BLOCKED)];

    assert_eq!(
        without_moderator_blocked_matches(Some(&blocklist), results(), no_chain_read).len(),
        2
    );
    assert_eq!(
        without_moderator_blocked_matches(None, results(), no_chain_read).len(),
        2
    );
}

#[test]
fn reads_run_a_few_at_a_time_and_keep_the_order() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    let running = AtomicUsize::new(0);
    let most = AtomicUsize::new(0);
    let reads = AtomicUsize::new(0);
    let items: Vec<String> = (0..10).map(|i| format!("USER{i}")).collect();

    let (blocked, rest) = partition_moderator_blocked(
        Some(&blocklist),
        items,
        |s| s.as_str(),
        |address: &str| {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20));
            running.fetch_sub(1, Ordering::SeqCst);
            reads.fetch_add(1, Ordering::SeqCst);
            Ok(Some(address == "USER3" || address == "USER7"))
        },
    );

    assert_eq!(blocked, vec!["USER3", "USER7"]);
    assert_eq!(
        rest,
        vec![
            "USER0", "USER1", "USER2", "USER4", "USER5", "USER6", "USER8", "USER9"
        ]
    );
    assert_eq!(reads.load(Ordering::SeqCst), 10);
    assert!(most.load(Ordering::SeqCst) <= 4, "at most 4 reads at once");
}
