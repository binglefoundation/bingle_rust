use std::sync::{Arc, Mutex};

use crate::util::reusable_mock_api::{InnerBingleApi, InnerBingleApiInternal, MockApiBoth};
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::router::Router;
use rust_comms::messages::types::*;

const RELAY_SENDER: &str = "RELAY_SENDER";
const RELAY_SENDER_ISSUER: &str = "RELAY_SENDER.";
const FAILED_RELAY: &str = "RELAY_FAILED";

fn make_report_failed_ripple(failed_relay_id: &str) -> Message {
    Message::ReportFail(ReportFailMessage::ReportFailedRipple(ReportFailedRipple {
        app: "reportFail".into(),
        tag: None,
        response_tag: None,
        failed_relay_id: failed_relay_id.into(),
        fail_type: "send_rejected".into(),
        timestamp: "2026-01-01T00:00:00Z".into(),
        confirmations: vec![],
        disputes: vec![],
    }))
}

// A combined mock that tracks calls to mark_relay_as_failed helpers.
// - known_relay_id: the relay id considered a known relay (for sender validation)
// - ddb_deleted / relay_cache_removed: track calls to mark_relay_as_failed helpers
struct TrackingApi {
    known_relay_id: String,
    ddb_deleted: Arc<Mutex<Vec<String>>>,
    relay_cache_removed: Arc<Mutex<Vec<String>>>,
}

impl InnerBingleApi for TrackingApi {
    fn list_all_relays(&self, _include_self: bool) -> Vec<rust_comms::relay::relay_finder::RelayInfo> {
        if self.known_relay_id.is_empty() {
            return Vec::new();
        }
        let addr: std::net::SocketAddr = "127.0.0.1:5000".parse().expect("valid addr");
        vec![rust_comms::relay::relay_finder::RelayInfo::root(self.known_relay_id.clone(), addr)]
    }
}

impl InnerBingleApiInternal for TrackingApi {
    fn ddb_delete_record(&self, id: &str) {
        self.ddb_deleted.lock().expect("lock ddb_deleted").push(id.to_string());
    }

    fn relay_finder_remove_relay(&self, relay_id: &str) {
        self.relay_cache_removed.lock().expect("lock relay_cache_removed").push(relay_id.to_string());
    }
}

fn router_with_api(api: Arc<TrackingApi>) -> Arc<Router> {
    let weak = crate::util::mock_bingle_api::to_weak(
        MockApiBoth::new_with_both_overrides(api.clone(), api.clone()),
    );
    let router = Arc::new(Router::new(weak));
    router.set_am_relay(true);
    let backend = Arc::new(Mutex::new(rust_comms::ddb::InMemoryDdbBackend::new()));
    router.set_ddb_backend(Some(backend));
    router
}

// Test: we are not a relay - message is silently ignored, mark_relay_as_failed not called
#[cfg_attr(not(target_os = "ios"), test)]
pub fn report_failed_ripple_ignored_when_not_relay() {
    struct PanicOnDelete;
    impl InnerBingleApiInternal for PanicOnDelete {
        fn ddb_delete_record(&self, _id: &str) {
            panic!("ddb_delete_record must not be called when we are not a relay");
        }
        fn relay_finder_remove_relay(&self, _relay_id: &str) {
            panic!("relay_finder_remove_relay must not be called when we are not a relay");
        }
    }

    let panic_arc = Arc::new(PanicOnDelete);
    let weak = crate::util::mock_bingle_api::to_weak(MockApiBoth::new_with_internal_override(panic_arc));
    let router = Arc::new(Router::new(weak));
    router.set_am_relay(false);
    let backend = Arc::new(Mutex::new(rust_comms::ddb::InMemoryDdbBackend::new()));
    router.set_ddb_backend(Some(backend));

    let handler = DefaultPrintingHandler;
    let msg = make_report_failed_ripple(FAILED_RELAY);
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, RELAY_SENDER_ISSUER);
    });
    // reaching here without panic verifies mark_relay_as_failed was not called
}

// Test: sender is not a known relay - message is silently ignored, mark_relay_as_failed not called
#[cfg_attr(not(target_os = "ios"), test)]
pub fn report_failed_ripple_ignored_when_sender_not_relay() {
    let ddb_deleted: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let relay_cache_removed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // known_relay_id is different from RELAY_SENDER, so sender is not a relay
    let tracking_api = Arc::new(TrackingApi {
        known_relay_id: "SOME_OTHER_RELAY".into(),
        ddb_deleted: ddb_deleted.clone(),
        relay_cache_removed: relay_cache_removed.clone(),
    });

    let router = router_with_api(tracking_api);

    let handler = DefaultPrintingHandler;
    let msg = make_report_failed_ripple(FAILED_RELAY);
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, RELAY_SENDER_ISSUER);
    });

    let deleted = ddb_deleted.lock().expect("lock ddb_deleted");
    assert!(deleted.is_empty(), "ddb_delete_record must not be called when sender is not a relay");
    let removed = relay_cache_removed.lock().expect("lock relay_cache_removed");
    assert!(removed.is_empty(), "relay_finder_remove_relay must not be called when sender is not a relay");
}

// Test: valid ripple from a known relay -> mark_relay_as_failed called for failed_relay_id
#[cfg_attr(not(target_os = "ios"), test)]
pub fn report_failed_ripple_marks_failed_when_sender_is_relay() {
    let ddb_deleted: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let relay_cache_removed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let tracking_api = Arc::new(TrackingApi {
        known_relay_id: RELAY_SENDER.into(),
        ddb_deleted: ddb_deleted.clone(),
        relay_cache_removed: relay_cache_removed.clone(),
    });

    let router = router_with_api(tracking_api);

    let handler = DefaultPrintingHandler;
    let msg = make_report_failed_ripple(FAILED_RELAY);
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, RELAY_SENDER_ISSUER);
    });

    let deleted = ddb_deleted.lock().expect("lock ddb_deleted");
    assert!(
        deleted.contains(&FAILED_RELAY.to_string()),
        "ddb_delete_record should have been called for the failed relay"
    );
    let removed = relay_cache_removed.lock().expect("lock relay_cache_removed");
    assert!(
        removed.contains(&FAILED_RELAY.to_string()),
        "relay_finder_remove_relay should have been called for the failed relay"
    );
}
