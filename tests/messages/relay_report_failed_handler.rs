use std::sync::{Arc, Mutex};

use crate::util::reusable_mock_api::{InnerBingleApi, InnerBingleApiInternal, MockApiBoth};
use rust_comms::engine::RelayState;
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::router::Router;
use rust_comms::messages::types::*;
use crate::util::test_util::signed_root_relay;

fn make_relay_report_failed(failed_relay_id: &str) -> Message {
    Message::ReportFail(ReportFailMessage::RelayReportFailed(RelayReportFailed {
        app: "reportFail".into(),
        tag: None,
        response_tag: None,
        failed_relay_id: failed_relay_id.into(),
        fail_type: "send_rejected".into(),
        timestamp: "2026-01-01T00:00:00Z".into(),
    }))
}

fn relays_status_response_json(relay_state: RelayState) -> serde_json::Value {
    let state_str = match relay_state {
        RelayState::Available => "available",
        RelayState::Off => "off",
        RelayState::Starting => "starting",
        RelayState::Loading => "loading",
        RelayState::Loaded => "loaded",
        RelayState::Own => "own",
        RelayState::Unknown => "unknown",
    };
    serde_json::json!({
        "app": "ddb",
        "type": "relaysStatusResponse",
        "responderState": state_str,
        "epochId": 0_i64,
        "treeOrder": 2_i32,
        "relayIds": [],
        "relayStates": []
    })
}

// A combined mock that implements both InnerBingleApi and InnerBingleApiInternal.
// - relay_id: the id returned by list_all_relays (empty string means no relay returned)
// - send_response: controls what send_message_to_network_with_response returns
// - ripple_called / ripple_originator: track calls to ripple_message
// - ddb_deleted / relay_cache_removed: track calls to mark_relay_as_failed helpers
struct TrackingApi {
    relay_id: String,
    send_response: Result<serde_json::Value, String>,
    ripple_called: Arc<Mutex<bool>>,
    ripple_originator: Arc<Mutex<Option<String>>>,
    ddb_deleted: Arc<Mutex<Vec<String>>>,
    relay_cache_removed: Arc<Mutex<Vec<String>>>,
}

impl InnerBingleApi for TrackingApi {
    fn list_all_relays(&self, _include_self: bool) -> Vec<rust_comms::relay::relay_finder::RelayInfo> {
        if self.relay_id.is_empty() {
            return Vec::new();
        }
        let addr: std::net::SocketAddr = "127.0.0.1:5000".parse().expect("valid addr");
        vec![signed_root_relay(&self.relay_id, addr)]
    }

    fn send_message_to_network_with_response(
        &self,
        _network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> {
        match &self.send_response {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(rust_comms::api::bingle_api::BingleError::Other(e.clone())),
        }
    }
}

impl InnerBingleApiInternal for TrackingApi {
    fn ripple_message(
        &self,
        _message: serde_json::Value,
        originator_id: String,
        _ddb_backend: &dyn rust_comms::ddb::DdbBackend,
    ) {
        let mut called = self.ripple_called.lock().expect("lock ripple_called");
        *called = true;
        let mut orig = self.ripple_originator.lock().expect("lock ripple_originator");
        *orig = Some(originator_id);
    }

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

// Test: not a relay - message is silently ignored (no ripple)
#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_report_failed_ignored_when_not_relay() {
    // PanicRipple verifies ripple_message is never called.
    struct PanicRipple;
    impl InnerBingleApiInternal for PanicRipple {
        fn ripple_message(
            &self,
            _message: serde_json::Value,
            _originator_id: String,
            _ddb_backend: &dyn rust_comms::ddb::DdbBackend,
        ) {
            panic!("ripple_message must not be called when we are not a relay");
        }
    }

    let panic_arc = Arc::new(PanicRipple);
    let weak = crate::util::mock_bingle_api::to_weak(MockApiBoth::new_with_internal_override(panic_arc));
    let router = Arc::new(Router::new(weak));
    router.set_am_relay(false);
    let backend = Arc::new(Mutex::new(rust_comms::ddb::InMemoryDdbBackend::new()));
    router.set_ddb_backend(Some(backend));

    let handler = DefaultPrintingHandler;
    let msg = make_relay_report_failed("RELAY_A");
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "SENDER.");
    });
    // reaching here without panic verifies ripple was not called
}

// Test: relay address found but send fails -> marks failed and ripples
#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_report_failed_ripples_on_send_failure() {
    let ripple_called = Arc::new(Mutex::new(false));
    let ripple_originator: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let ddb_deleted: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let relay_cache_removed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let tracking_api = Arc::new(TrackingApi {
        relay_id: "RELAY_B".into(),
        send_response: Err("connection refused".into()),
        ripple_called: ripple_called.clone(),
        ripple_originator: ripple_originator.clone(),
        ddb_deleted: ddb_deleted.clone(),
        relay_cache_removed: relay_cache_removed.clone(),
    });

    let router = router_with_api(tracking_api);

    let handler = DefaultPrintingHandler;
    let msg = make_relay_report_failed("RELAY_B");
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "SENDER.");
    });

    let called = ripple_called.lock().expect("lock ripple_called");
    assert!(*called, "ripple_message should have been called when send fails");
    let orig = ripple_originator.lock().expect("lock ripple_originator");
    assert_eq!(orig.as_deref(), Some("RELAY_B"), "originator_id should be the failed relay id");
    let deleted = ddb_deleted.lock().expect("lock ddb_deleted");
    assert!(deleted.contains(&"RELAY_B".to_string()), "ddb_delete_record should have been called for RELAY_B");
    let removed = relay_cache_removed.lock().expect("lock relay_cache_removed");
    assert!(removed.contains(&"RELAY_B".to_string()), "relay_finder_remove_relay should have been called for RELAY_B");
}

// Test: relay found, responds with non-Available state -> marks failed and ripples
#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_report_failed_ripples_on_non_available_response() {
    let ripple_called = Arc::new(Mutex::new(false));
    let ripple_originator: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let ddb_deleted: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let relay_cache_removed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let tracking_api = Arc::new(TrackingApi {
        relay_id: "RELAY_C".into(),
        send_response: Ok(relays_status_response_json(RelayState::Starting)),
        ripple_called: ripple_called.clone(),
        ripple_originator: ripple_originator.clone(),
        ddb_deleted: ddb_deleted.clone(),
        relay_cache_removed: relay_cache_removed.clone(),
    });

    let router = router_with_api(tracking_api);

    let handler = DefaultPrintingHandler;
    let msg = make_relay_report_failed("RELAY_C");
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "SENDER.");
    });

    let called = ripple_called.lock().expect("lock ripple_called");
    assert!(*called, "ripple_message should have been called when relay responds with non-Available state");
    let orig = ripple_originator.lock().expect("lock ripple_originator");
    assert_eq!(orig.as_deref(), Some("RELAY_C"));
    let deleted = ddb_deleted.lock().expect("lock ddb_deleted");
    assert!(deleted.contains(&"RELAY_C".to_string()), "ddb_delete_record should have been called for RELAY_C");
    let removed = relay_cache_removed.lock().expect("lock relay_cache_removed");
    assert!(removed.contains(&"RELAY_C".to_string()), "relay_finder_remove_relay should have been called for RELAY_C");
}

// Test: relay found, responds Available -> no ripple (just WARN)
#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_report_failed_no_ripple_when_available() {
    // PanicOnRipple verifies ripple_message is never called.
    struct PanicOnRipple;
    impl InnerBingleApi for PanicOnRipple {
        fn list_all_relays(&self, _include_self: bool) -> Vec<rust_comms::relay::relay_finder::RelayInfo> {
            let addr: std::net::SocketAddr = "127.0.0.1:5003".parse().expect("valid addr");
            vec![signed_root_relay("RELAY_D", addr)]
        }
        fn send_message_to_network_with_response(
            &self,
            _network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint,
            _user_id: &rust_comms::api::bingle_api::UserId,
            _message: serde_json::Value,
            _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
        ) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> {
            Ok(relays_status_response_json(RelayState::Available))
        }
    }
    impl InnerBingleApiInternal for PanicOnRipple {
        fn ripple_message(
            &self,
            _message: serde_json::Value,
            _originator_id: String,
            _ddb_backend: &dyn rust_comms::ddb::DdbBackend,
        ) {
            panic!("ripple_message must not be called when relay responds Available");
        }
    }

    let panic_api = Arc::new(PanicOnRipple);
    let weak = crate::util::mock_bingle_api::to_weak(MockApiBoth::new_with_both_overrides(panic_api.clone(), panic_api));
    let router = Arc::new(Router::new(weak));
    router.set_am_relay(true);
    let backend = Arc::new(Mutex::new(rust_comms::ddb::InMemoryDdbBackend::new()));
    router.set_ddb_backend(Some(backend));

    let handler = DefaultPrintingHandler;
    let msg = make_relay_report_failed("RELAY_D");
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "SENDER.");
    });
    // reaching here without panic verifies ripple was not called
}
