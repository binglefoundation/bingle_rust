use std::sync::{Arc, Mutex};

use crate::util::reusable_mock_api::{InnerBingleApi, InnerBingleApiInternal, MockApiBoth};
use bingle_core::messages::handlers::DefaultPrintingHandler;
use bingle_core::messages::router::Router;
use bingle_core::messages::types::*;

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

// A combined mock that tracks calls to relay_mark_suspect.
// - known_relay_id: the relay id considered a known relay (for sender validation)
// - suspected: tracks calls to relay_mark_suspect
struct TrackingApi {
    known_relay_id: String,
    suspected: Arc<Mutex<Vec<String>>>,
}

impl InnerBingleApi for TrackingApi {
    fn list_all_relays(
        &self,
        _include_self: bool,
    ) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        if self.known_relay_id.is_empty() {
            return Vec::new();
        }
        let addr: std::net::SocketAddr = "127.0.0.1:5000".parse().expect("valid addr");
        vec![crate::util::test_util::signed_root_relay(
            &self.known_relay_id,
            addr,
        )]
    }
}

impl InnerBingleApiInternal for TrackingApi {
    fn relay_mark_suspect(&self, relay_id: &str) -> bool {
        self.suspected
            .lock()
            .expect("lock suspected")
            .push(relay_id.to_string());
        true
    }
}

fn router_with_api(api: Arc<TrackingApi>) -> Arc<Router> {
    let weak = crate::util::mock_bingle_api::to_weak(MockApiBoth::new_with_both_overrides(
        api.clone(),
        api.clone(),
    ));
    let router = Arc::new(Router::new(weak));
    router.set_am_relay(true);
    let backend = Arc::new(Mutex::new(bingle_core::ddb::InMemoryDdbBackend::new()));
    router.set_ddb_backend(Some(backend));
    router
}

// Test: we are not a relay - message is silently ignored, relay_mark_suspect not called
#[test]
#[cfg(not(target_os = "ios"))]
pub fn report_failed_ripple_ignored_when_not_relay() {
    struct PanicOnSuspect;
    impl InnerBingleApiInternal for PanicOnSuspect {
        fn relay_mark_suspect(&self, _relay_id: &str) -> bool {
            panic!("relay_mark_suspect must not be called when we are not a relay");
        }
    }

    let panic_arc = Arc::new(PanicOnSuspect);
    let weak =
        crate::util::mock_bingle_api::to_weak(MockApiBoth::new_with_internal_override(panic_arc));
    let router = Arc::new(Router::new(weak));
    router.set_am_relay(false);
    let backend = Arc::new(Mutex::new(bingle_core::ddb::InMemoryDdbBackend::new()));
    router.set_ddb_backend(Some(backend));

    let handler = DefaultPrintingHandler;
    let msg = make_report_failed_ripple(FAILED_RELAY);
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, RELAY_SENDER_ISSUER);
    });
    // reaching here without panic verifies relay_mark_suspect was not called
}

// Test: sender is not a known relay - message is silently ignored, relay_mark_suspect not called
#[test]
#[cfg(not(target_os = "ios"))]
pub fn report_failed_ripple_ignored_when_sender_not_relay() {
    let suspected: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // known_relay_id is different from RELAY_SENDER, so sender is not a relay
    let tracking_api = Arc::new(TrackingApi {
        known_relay_id: "SOME_OTHER_RELAY".into(),
        suspected: suspected.clone(),
    });

    let router = router_with_api(tracking_api);

    let handler = DefaultPrintingHandler;
    let msg = make_report_failed_ripple(FAILED_RELAY);
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, RELAY_SENDER_ISSUER);
    });

    assert!(
        suspected.lock().expect("lock suspected").is_empty(),
        "relay_mark_suspect must not be called when sender is not a relay"
    );
}

// Test: valid ripple from a known relay -> relay_mark_suspect called for failed_relay_id
#[test]
#[cfg(not(target_os = "ios"))]
pub fn report_failed_ripple_marks_failed_when_sender_is_relay() {
    let suspected: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let tracking_api = Arc::new(TrackingApi {
        known_relay_id: RELAY_SENDER.into(),
        suspected: suspected.clone(),
    });

    let router = router_with_api(tracking_api);

    let handler = DefaultPrintingHandler;
    let msg = make_report_failed_ripple(FAILED_RELAY);
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, RELAY_SENDER_ISSUER);
    });

    assert_eq!(
        *suspected.lock().expect("lock suspected"),
        vec![FAILED_RELAY.to_string()],
        "the failed relay should be marked suspect, not removed (#516)"
    );
}
