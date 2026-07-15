use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::util::test_util::{
    signed_non_root_relay, signed_non_root_relay_with, signed_root_relay, signed_root_relay_with,
};
use bingle_core::api::bingle_api::{BingleError, NetworkEndpoint, ProgressCallback, UserId};
use bingle_core::ddb::InetSocketAddress;
use bingle_core::engine::RelayState;
use bingle_core::messages::marshal::{from_json_value, to_json_value};
use bingle_core::messages::types::{
    DdbMessage, DdbRelaysStatusResponse, Message, ReportFailMessage,
};
use bingle_core::relay::relay_finder::{RelayFinderTrait, RelayInfo};
use bingle_core::relay::relay_updater::RelayUpdater;

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak_api_both};

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

type QueryResponder = dyn Fn(&str, usize) -> Result<serde_json::Value, BingleError> + Send + Sync;

struct QueryMockApi {
    queried_ids: Arc<Mutex<Vec<String>>>,
    responder: Arc<QueryResponder>,
}

impl InnerBingleApi for QueryMockApi {
    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        user_id: &UserId,
        message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        assert_eq!(
            message.get("type").and_then(|value| value.as_str()),
            Some("getRelaysStatus")
        );
        assert_eq!(
            message.get("app").and_then(|value| value.as_str()),
            Some("ddb")
        );

        let query_index = {
            let mut queried_ids = self
                .queried_ids
                .lock()
                .expect("queried_ids lock should succeed");
            queried_ids.push(user_id.clone());
            queried_ids.len() - 1
        };

        (self.responder)(user_id, query_index)
    }
}

fn relays_status_response_json(
    responder_state: RelayState,
    relays: Vec<(&str, SocketAddr, RelayState)>,
) -> serde_json::Value {
    let response = DdbRelaysStatusResponse {
        app: "ddb".to_string(),
        responder_state,
        epoch_id: 1,
        tree_order: 2,
        relay_ids: relays.iter().map(|(id, _, _)| id.to_string()).collect(),
        relay_endpoints: Some(
            relays
                .iter()
                .map(|(_, relay_addr, _)| InetSocketAddress::from(*relay_addr))
                .collect(),
        ),
        relay_states: relays.iter().map(|(_, _, state)| *state).collect(),
        response_tag: None,
        text: None,
        data: None,
    };

    to_json_value(&Message::Ddb(DdbMessage::RelaysStatusResponse(response)))
}

fn updater_with_api(
    my_id: &str,
    relays: Vec<RelayInfo>,
    api: Arc<dyn InnerBingleApi + Send + Sync>,
) -> RelayUpdater {
    RelayUpdater::new_with_api(
        my_id.to_string(),
        to_weak_api_both(MockApiBoth::new_with_api_override(api)),
        Arc::new(move || relays.clone()),
    )
}

/// Captures the response timeout passed to the relay-status query so we can
/// assert it is the short bounded probe timeout rather than the ~90s default.
struct TimeoutCapturingApi {
    captured_timeout: Arc<Mutex<Option<Duration>>>,
    response: serde_json::Value,
}

impl InnerBingleApi for TimeoutCapturingApi {
    fn send_message_to_network_with_response_timeout(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
        timeout: Duration,
    ) -> Result<serde_json::Value, BingleError> {
        assert_eq!(
            message.get("type").and_then(|value| value.as_str()),
            Some("getRelaysStatus")
        );
        *self
            .captured_timeout
            .lock()
            .expect("captured_timeout lock should succeed") = Some(timeout);
        Ok(self.response.clone())
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_status_query_uses_bounded_timeout() {
    // Regression (bingle_rust #31/#43): the relay-status probe must fail fast when
    // the relay is unreachable so the single-threaded pending-message drain loop
    // is not wedged for the full ~90s default response timeout while offline.
    let captured = Arc::new(Mutex::new(None));
    let response = relays_status_response_json(
        RelayState::Available,
        vec![
            ("MYID", addr(60001), RelayState::Own),
            ("ROOT1", addr(60002), RelayState::Available),
        ],
    );

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(TimeoutCapturingApi {
        captured_timeout: captured.clone(),
        response,
    });

    let updater = updater_with_api(
        "MYID.",
        vec![
            signed_root_relay("MYID", addr(60001)),
            signed_root_relay("ROOT1", addr(60002)),
        ],
        api,
    );
    updater.init_from_blockchain();

    let selected = updater
        .relay_select_and_query(&[])
        .expect("relay-status query should succeed");
    assert_eq!(selected.id(), "ROOT1");

    let timeout = captured
        .lock()
        .expect("captured_timeout lock should succeed")
        .expect("relay-status query should pass an explicit timeout");
    assert!(
        timeout > Duration::ZERO && timeout <= Duration::from_secs(10),
        "relay-status query should use a short bounded timeout (not the ~90s default), got {:?}",
        timeout
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_updater_init_from_blockchain_sets_state_ttl_and_sorts() {
    let updater = RelayUpdater::new(
        "MYID.".to_string(),
        Arc::new(|| {
            vec![
                signed_root_relay("ZZZ", addr(56003)),
                signed_root_relay("MYID", addr(56001)),
                signed_root_relay("AAA", addr(56002)),
            ]
        }),
    );

    updater.init_from_blockchain();

    let cache = updater.relay_info_cache();
    let relays = cache.list_all_relays("MYID", true);
    assert_eq!(relays.len(), 3);
    assert_eq!(relays[0].id(), "AAA");
    assert_eq!(relays[1].id(), "MYID");
    assert_eq!(relays[2].id(), "ZZZ");

    let own = relays
        .iter()
        .find(|relay| relay.id() == "MYID")
        .expect("MYID should be present in relay cache");
    assert!(own.is_root);
    assert_eq!(own.state, Some(RelayState::Own));
    assert_eq!(own.ttl, Some(30_000));

    let first_other = relays
        .iter()
        .find(|relay| relay.id() == "AAA")
        .expect("AAA should be present in relay cache");
    assert!(first_other.is_root);
    assert_eq!(first_other.state, Some(RelayState::Unknown));
    assert_eq!(first_other.ttl, Some(30));

    let second_other = relays
        .iter()
        .find(|relay| relay.id() == "ZZZ")
        .expect("ZZZ should be present in relay cache");
    assert!(second_other.is_root);
    assert_eq!(second_other.state, Some(RelayState::Unknown));
    assert_eq!(second_other.ttl, Some(30));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_updater_init_from_blockchain_sets_unknown_when_own_not_found() {
    let updater = RelayUpdater::new(
        "MISSING.".to_string(),
        Arc::new(|| {
            vec![
                signed_root_relay("ROOT1", addr(56101)),
                signed_root_relay("ROOT2", addr(56102)),
            ]
        }),
    );

    updater.init_from_blockchain();

    let cache = updater.relay_info_cache();
    let relays = cache.list_all_relays("MISSING", true);
    assert_eq!(relays.len(), 2);
    for relay in relays {
        assert!(relay.is_root);
        assert_eq!(relay.state, Some(RelayState::Unknown));
        assert_eq!(relay.ttl, Some(30));
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_select_and_query_returns_none_when_no_root_relays() {
    let updater = RelayUpdater::new("MYID.".to_string(), Arc::new(Vec::new));
    assert!(updater.relay_select_and_query(&[]).is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_select_and_query_single_root_updates_cache_and_returns_root() {
    let queried_ids = Arc::new(Mutex::new(Vec::new()));
    let response = relays_status_response_json(
        RelayState::Available,
        vec![
            ("MYID", addr(56200), RelayState::Own),
            ("ROOT1", addr(56201), RelayState::Available),
            ("ROOT2", addr(56202), RelayState::Off),
        ],
    );

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(QueryMockApi {
        queried_ids: queried_ids.clone(),
        responder: Arc::new(move |_user_id: &str, _query_index: usize| Ok(response.clone())),
    });

    let updater = updater_with_api(
        "MYID.",
        vec![
            signed_root_relay("MYID", addr(56200)),
            signed_root_relay("ROOT1", addr(56201)),
        ],
        api,
    );
    updater.init_from_blockchain();

    let selected = updater
        .relay_select_and_query(&[])
        .expect("single-root query should succeed");
    assert_eq!(selected.id(), "ROOT1");
    assert_eq!(selected.state, Some(RelayState::Available));
    assert_eq!(selected.ttl, Some(300));

    let ids = queried_ids
        .lock()
        .expect("queried_ids lock should succeed")
        .clone();
    assert_eq!(ids, vec!["ROOT1".to_string()]);

    let relays = updater.relay_info_cache().list_all_relays("MYID", true);
    let own = relays
        .iter()
        .find(|relay| relay.id() == "MYID")
        .expect("MYID should exist in cache");
    assert_eq!(own.state, Some(RelayState::Own));
    assert_eq!(own.ttl, Some(30_000));

    let root1 = relays
        .iter()
        .find(|relay| relay.id() == "ROOT1")
        .expect("ROOT1 should exist in cache");
    assert_eq!(root1.state, Some(RelayState::Available));
    assert_eq!(root1.ttl, Some(300));

    let root2 = relays
        .iter()
        .find(|relay| relay.id() == "ROOT2")
        .expect("ROOT2 should exist in cache");
    assert_eq!(root2.state, Some(RelayState::Off));
    assert_eq!(root2.ttl, Some(30));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_select_and_query_falls_back_to_alternate_when_preferred_fails() {
    let queried_ids = Arc::new(Mutex::new(Vec::new()));

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(QueryMockApi {
        queried_ids: queried_ids.clone(),
        responder: Arc::new(move |queried_id: &str, query_index: usize| {
            if query_index == 0 {
                return Err(BingleError::Other("simulated failure".to_string()));
            }

            let alternate = if queried_id == "ROOTA" {
                "ROOTB"
            } else {
                "ROOTA"
            };
            Ok(relays_status_response_json(
                RelayState::Available,
                vec![
                    ("MYID", addr(56300), RelayState::Own),
                    (queried_id, addr(56301), RelayState::Available),
                    (alternate, addr(56302), RelayState::Off),
                ],
            ))
        }),
    });

    let updater = updater_with_api(
        "MYID.",
        vec![
            signed_root_relay("MYID", addr(56300)),
            signed_root_relay("ROOTA", addr(56301)),
            signed_root_relay("ROOTB", addr(56302)),
        ],
        api,
    );
    updater.init_from_blockchain();

    let selected = updater
        .relay_select_and_query(&[])
        .expect("alternate root should be selected");

    let ids = queried_ids
        .lock()
        .expect("queried_ids lock should succeed")
        .clone();
    assert_eq!(ids.len(), 2);
    assert_eq!(selected.id(), ids[1]);

    let relays = updater.relay_info_cache().list_all_relays("MYID", true);
    let first = relays
        .iter()
        .find(|relay| relay.id() == ids[0])
        .expect("first queried relay should exist in cache");
    assert_eq!(first.state, Some(RelayState::Off));
    assert_eq!(first.ttl, Some(30));

    let second = relays
        .iter()
        .find(|relay| relay.id() == ids[1])
        .expect("second queried relay should exist in cache");
    assert_eq!(second.state, Some(RelayState::Available));
    assert_eq!(second.ttl, Some(300));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_select_and_query_returns_none_after_all_candidates_fail() {
    let queried_ids = Arc::new(Mutex::new(Vec::new()));

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(QueryMockApi {
        queried_ids: queried_ids.clone(),
        responder: Arc::new(move |_queried_id: &str, _query_index: usize| {
            Ok(relays_status_response_json(
                RelayState::Off,
                vec![
                    ("MYID", addr(56400), RelayState::Own),
                    ("ROOTA", addr(56401), RelayState::Off),
                    ("ROOTB", addr(56402), RelayState::Off),
                ],
            ))
        }),
    });

    let updater = updater_with_api(
        "MYID.",
        vec![
            signed_root_relay("MYID", addr(56400)),
            signed_root_relay("ROOTA", addr(56401)),
            signed_root_relay("ROOTB", addr(56402)),
        ],
        api,
    );
    updater.init_from_blockchain();

    assert!(updater.relay_select_and_query(&[]).is_none());

    let ids = queried_ids
        .lock()
        .expect("queried_ids lock should succeed")
        .clone();
    assert_eq!(ids.len(), 2);

    let relays = updater.relay_info_cache().list_all_relays("MYID", true);
    for queried_id in ids {
        let relay = relays
            .iter()
            .find(|candidate| candidate.id() == queried_id)
            .expect("queried relay should exist in cache");
        assert_eq!(relay.state, Some(RelayState::Off));
        assert_eq!(relay.ttl, Some(30));
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn update_when_expired_is_noop_when_nothing_expired() {
    // All entries have a long TTL (30_000 s) so nothing is expired immediately after construction.
    let discover_call_count = Arc::new(Mutex::new(0u32));
    let counter_clone = discover_call_count.clone();
    let updater = RelayUpdater::new(
        "MYID.".to_string(),
        Arc::new(move || {
            *counter_clone.lock().expect("lock should succeed") += 1;
            vec![signed_root_relay_with(
                "ROOT1",
                addr(57001),
                Some(RelayState::Unknown),
                Some(30_000),
            )]
        }),
    );
    updater.init_from_blockchain();

    // Reset the counter after init so we only track calls from update_when_expired
    *discover_call_count.lock().expect("lock should succeed") = 0;

    updater.update_when_expired();

    let calls = *discover_call_count.lock().expect("lock should succeed");
    assert_eq!(
        calls, 0,
        "discover_roots should not be called when no entries are expired"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn update_when_expired_calls_init_from_blockchain_when_root_expired() {
    let discover_call_count = Arc::new(Mutex::new(0u32));
    let counter_clone = discover_call_count.clone();
    let updater = RelayUpdater::new(
        "MYID.".to_string(),
        Arc::new(move || {
            *counter_clone.lock().expect("lock should succeed") += 1;
            vec![signed_root_relay_with(
                "ROOT1",
                addr(57101),
                Some(RelayState::Unknown),
                Some(30_000),
            )]
        }),
    );

    // Directly populate the cache with a ttl=0 root entry so we can trigger expiry without
    // waiting 30 seconds (init_from_blockchain always sets SHORT_TTL_SECS=30 for non-own relays)
    updater
        .relay_info_cache()
        .replace_relays(vec![signed_root_relay_with(
            "ROOT1",
            addr(57101),
            Some(RelayState::Unknown),
            Some(0),
        )]);

    // Sleep so the ttl=0 entry is now expired (last_updated + 0s < now)
    std::thread::sleep(Duration::from_millis(5));

    updater.update_when_expired();

    let calls = *discover_call_count.lock().expect("lock should succeed");
    assert!(
        calls > 0,
        "discover_roots should be called when a root entry is expired"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn update_when_expired_calls_relay_select_when_only_non_root_expired() {
    let queried_ids = Arc::new(Mutex::new(Vec::<String>::new()));
    let queried_clone = queried_ids.clone();

    let response = relays_status_response_json(
        RelayState::Available,
        vec![
            ("MYID", addr(57201), RelayState::Own),
            ("ROOT1", addr(57202), RelayState::Available),
        ],
    );

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(QueryMockApi {
        queried_ids: queried_clone,
        responder: Arc::new(move |_user_id: &str, _query_index: usize| Ok(response.clone())),
    });

    // Root relay has a long TTL (not expired); non-root has ttl=0 (will expire immediately)
    let relays = vec![
        signed_root_relay_with("MYID", addr(57201), Some(RelayState::Own), Some(30_000)),
        signed_root_relay_with(
            "ROOT1",
            addr(57202),
            Some(RelayState::Unknown),
            Some(30_000),
        ),
        signed_non_root_relay_with("NR1", addr(57203), Some(RelayState::Unknown), Some(0)),
    ];
    let updater = updater_with_api("MYID.", relays.clone(), api);
    updater.relay_info_cache().replace_relays(relays);

    // Sleep so the ttl=0 non-root entry becomes expired
    std::thread::sleep(Duration::from_millis(5));

    updater.update_when_expired();

    let ids = queried_ids.lock().expect("lock should succeed").clone();
    assert!(
        !ids.is_empty(),
        "relay_select_and_query should have queried a root relay for non-root expiry"
    );
}

struct ReportTrackingApi {
    queried_ids: Arc<Mutex<Vec<String>>>,
    responder: Arc<QueryResponder>,
    reported_failed_ids: Arc<Mutex<Vec<String>>>,
    reported_to_relay_id: Arc<Mutex<Option<String>>>,
}

impl InnerBingleApi for ReportTrackingApi {
    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        user_id: &UserId,
        message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        assert_eq!(
            message.get("type").and_then(|value| value.as_str()),
            Some("getRelaysStatus")
        );

        let query_index = {
            let mut queried_ids = self
                .queried_ids
                .lock()
                .expect("queried_ids lock should succeed");
            queried_ids.push(user_id.clone());
            queried_ids.len() - 1
        };

        (self.responder)(user_id, query_index)
    }

    fn send_message_to_network(
        &self,
        _nsk: &NetworkEndpoint,
        user_id: &UserId,
        message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        let parsed = from_json_value(message).expect("relay report message should parse");
        if let Message::ReportFail(ReportFailMessage::RelayReportFailed(report)) = parsed {
            self.reported_failed_ids
                .lock()
                .expect("reported_failed_ids lock should succeed")
                .push(report.failed_relay_id);
            *self
                .reported_to_relay_id
                .lock()
                .expect("reported_to_relay_id lock should succeed") = Some(user_id.clone());
        }
        Ok(true)
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_report_failed_sent_when_preferred_relay_errors() {
    let queried_ids = Arc::new(Mutex::new(Vec::new()));
    let reported_failed_ids = Arc::new(Mutex::new(Vec::new()));
    let reported_to_relay_id = Arc::new(Mutex::new(None));

    let alternate_response = relays_status_response_json(
        RelayState::Available,
        vec![
            ("MYID", addr(58001), RelayState::Own),
            ("ROOTA", addr(58002), RelayState::Off),
            ("ROOTB", addr(58003), RelayState::Available),
        ],
    );

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(ReportTrackingApi {
        queried_ids: queried_ids.clone(),
        responder: Arc::new(move |_user_id: &str, query_index: usize| {
            if query_index == 0 {
                return Err(BingleError::Other(
                    "simulated preferred failure".to_string(),
                ));
            }
            Ok(alternate_response.clone())
        }),
        reported_failed_ids: reported_failed_ids.clone(),
        reported_to_relay_id: reported_to_relay_id.clone(),
    });

    let updater = updater_with_api(
        "MYID.",
        vec![
            signed_root_relay("MYID", addr(58001)),
            signed_root_relay("ROOTA", addr(58002)),
            signed_root_relay("ROOTB", addr(58003)),
        ],
        api,
    );
    updater.init_from_blockchain();

    let selected = updater
        .relay_select_and_query(&[])
        .expect("alternate should be selected after preferred fails");

    let ids = queried_ids
        .lock()
        .expect("queried_ids lock should succeed")
        .clone();
    assert_eq!(ids.len(), 2, "should have queried preferred then alternate");

    let failed = reported_failed_ids
        .lock()
        .expect("reported_failed_ids lock should succeed")
        .clone();
    assert_eq!(failed.len(), 1, "should have reported one failed relay");
    assert_eq!(
        failed[0], ids[0],
        "reported failed relay should be the preferred one"
    );

    let reported_to = reported_to_relay_id
        .lock()
        .expect("reported_to_relay_id lock should succeed")
        .clone()
        .expect("should have a relay to report to");
    assert_eq!(
        reported_to,
        selected.id(),
        "should have reported to the selected relay"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_report_failed_sent_when_preferred_relay_not_available() {
    let queried_ids = Arc::new(Mutex::new(Vec::new()));
    let reported_failed_ids = Arc::new(Mutex::new(Vec::new()));
    let reported_to_relay_id = Arc::new(Mutex::new(None));

    let preferred_response = relays_status_response_json(
        RelayState::Off,
        vec![
            ("MYID", addr(58101), RelayState::Own),
            ("ROOTA", addr(58102), RelayState::Off),
            ("ROOTB", addr(58103), RelayState::Available),
        ],
    );

    let alternate_response = relays_status_response_json(
        RelayState::Available,
        vec![
            ("MYID", addr(58101), RelayState::Own),
            ("ROOTA", addr(58102), RelayState::Off),
            ("ROOTB", addr(58103), RelayState::Available),
        ],
    );

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(ReportTrackingApi {
        queried_ids: queried_ids.clone(),
        responder: Arc::new(move |_user_id: &str, query_index: usize| {
            if query_index == 0 {
                return Ok(preferred_response.clone());
            }
            Ok(alternate_response.clone())
        }),
        reported_failed_ids: reported_failed_ids.clone(),
        reported_to_relay_id: reported_to_relay_id.clone(),
    });

    let updater = updater_with_api(
        "MYID.",
        vec![
            signed_root_relay("MYID", addr(58101)),
            signed_root_relay("ROOTA", addr(58102)),
            signed_root_relay("ROOTB", addr(58103)),
        ],
        api,
    );
    updater.init_from_blockchain();

    let selected = updater
        .relay_select_and_query(&[])
        .expect("alternate should be selected after preferred is not available");

    let ids = queried_ids
        .lock()
        .expect("queried_ids lock should succeed")
        .clone();
    assert_eq!(ids.len(), 2, "should have queried preferred then alternate");

    let failed = reported_failed_ids
        .lock()
        .expect("reported_failed_ids lock should succeed")
        .clone();
    assert_eq!(failed.len(), 1, "should have reported one failed relay");
    assert_eq!(
        failed[0], ids[0],
        "reported failed relay should be the preferred one"
    );

    let reported_to = reported_to_relay_id
        .lock()
        .expect("reported_to_relay_id lock should succeed")
        .clone()
        .expect("should have a relay to report to");
    assert_eq!(
        reported_to,
        selected.id(),
        "should have reported to the selected relay"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_report_failed_not_sent_when_preferred_succeeds() {
    let queried_ids = Arc::new(Mutex::new(Vec::new()));
    let reported_failed_ids = Arc::new(Mutex::new(Vec::new()));
    let reported_to_relay_id = Arc::new(Mutex::new(None));

    let response = relays_status_response_json(
        RelayState::Available,
        vec![
            ("MYID", addr(58201), RelayState::Own),
            ("ROOT1", addr(58202), RelayState::Available),
        ],
    );

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(ReportTrackingApi {
        queried_ids: queried_ids.clone(),
        responder: Arc::new(move |_user_id: &str, _query_index: usize| Ok(response.clone())),
        reported_failed_ids: reported_failed_ids.clone(),
        reported_to_relay_id: reported_to_relay_id.clone(),
    });

    let updater = updater_with_api(
        "MYID.",
        vec![
            signed_root_relay("MYID", addr(58201)),
            signed_root_relay("ROOT1", addr(58202)),
        ],
        api,
    );
    updater.init_from_blockchain();

    let selected = updater
        .relay_select_and_query(&[])
        .expect("preferred relay should be selected");
    assert_eq!(selected.id(), "ROOT1");

    let failed = reported_failed_ids
        .lock()
        .expect("reported_failed_ids lock should succeed")
        .clone();
    assert!(
        failed.is_empty(),
        "no failed relays should be reported when preferred succeeds"
    );

    let reported_to = reported_to_relay_id
        .lock()
        .expect("reported_to_relay_id lock should succeed")
        .clone();
    assert!(
        reported_to.is_none(),
        "no report should be sent when preferred succeeds"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_select_and_query_excludes_id_from_candidates() {
    let queried_ids = Arc::new(Mutex::new(Vec::new()));
    let response = relays_status_response_json(
        RelayState::Available,
        vec![
            ("MYID", addr(59001), RelayState::Own),
            ("ROOTA", addr(59002), RelayState::Available),
            ("ROOTB", addr(59003), RelayState::Available),
        ],
    );

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(QueryMockApi {
        queried_ids: queried_ids.clone(),
        responder: Arc::new(move |_user_id: &str, _query_index: usize| Ok(response.clone())),
    });

    let updater = updater_with_api(
        "MYID.",
        vec![
            signed_root_relay("MYID", addr(59001)),
            signed_root_relay("ROOTA", addr(59002)),
            signed_root_relay("ROOTB", addr(59003)),
        ],
        api,
    );
    updater.init_from_blockchain();

    let exclude = vec!["ROOTA".to_string()];
    let selected = updater
        .relay_select_and_query(&exclude)
        .expect("should select ROOTB when ROOTA is excluded");

    assert_eq!(
        selected.id(),
        "ROOTB",
        "excluded relay should not be selected"
    );

    let ids = queried_ids
        .lock()
        .expect("queried_ids lock should succeed")
        .clone();
    assert!(
        !ids.contains(&"ROOTA".to_string()),
        "excluded relay should never be queried"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_select_and_query_returns_none_when_all_candidates_excluded() {
    let queried_ids = Arc::new(Mutex::new(Vec::new()));

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(QueryMockApi {
        queried_ids: queried_ids.clone(),
        responder: Arc::new(|_user_id: &str, _query_index: usize| {
            panic!("should not query any relay when all are excluded")
        }),
    });

    let updater = updater_with_api(
        "MYID.",
        vec![
            signed_root_relay("MYID", addr(59101)),
            signed_root_relay("ROOTA", addr(59102)),
        ],
        api,
    );
    updater.init_from_blockchain();

    let exclude = vec!["ROOTA".to_string()];
    assert!(
        updater.relay_select_and_query(&exclude).is_none(),
        "should return None when all non-self candidates are excluded"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_select_and_query_considers_non_root_relays() {
    let queried_ids = Arc::new(Mutex::new(Vec::new()));
    let response = relays_status_response_json(
        RelayState::Available,
        vec![
            ("MYID", addr(59201), RelayState::Own),
            ("NONROOT1", addr(59202), RelayState::Available),
        ],
    );

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(QueryMockApi {
        queried_ids: queried_ids.clone(),
        responder: Arc::new(move |_user_id: &str, _query_index: usize| Ok(response.clone())),
    });

    let updater = updater_with_api("MYID.", vec![], api);

    // Manually populate the cache with a non-root relay so the updater sees it
    updater
        .relay_info_cache()
        .replace_relays(vec![signed_non_root_relay("NONROOT1", addr(59202))]);

    let selected = updater
        .relay_select_and_query(&[])
        .expect("non-root relay should be a valid candidate");

    assert_eq!(
        selected.id(),
        "NONROOT1",
        "non-root relay should be selectable"
    );
    let ids = queried_ids
        .lock()
        .expect("queried_ids lock should succeed")
        .clone();
    assert_eq!(
        ids,
        vec!["NONROOT1".to_string()],
        "non-root relay should have been queried"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_select_and_query_excludes_my_id_always() {
    let queried_ids = Arc::new(Mutex::new(Vec::new()));
    let response = relays_status_response_json(
        RelayState::Available,
        vec![
            ("MYID", addr(59301), RelayState::Own),
            ("ROOT1", addr(59302), RelayState::Available),
        ],
    );

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(QueryMockApi {
        queried_ids: queried_ids.clone(),
        responder: Arc::new(move |_user_id: &str, _query_index: usize| Ok(response.clone())),
    });

    let updater = updater_with_api(
        "MYID.",
        vec![
            signed_root_relay("MYID", addr(59301)),
            signed_root_relay("ROOT1", addr(59302)),
        ],
        api,
    );
    updater.init_from_blockchain();

    let selected = updater
        .relay_select_and_query(&[])
        .expect("should select ROOT1, not MYID");

    assert_eq!(selected.id(), "ROOT1", "own id should never be selected");

    let ids = queried_ids
        .lock()
        .expect("queried_ids lock should succeed")
        .clone();
    assert!(
        !ids.contains(&"MYID".to_string()),
        "own id should never be queried"
    );
}
