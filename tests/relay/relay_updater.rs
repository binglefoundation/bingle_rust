use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{BingleError, NetworkEndpoint, ProgressCallback, UserId};
use rust_comms::engine::RelayState;
use rust_comms::messages::marshal::to_json_value;
use rust_comms::messages::types::{DdbMessage, DdbRelaysStatusResponse, Message};
use rust_comms::ddb::InetSocketAddress;
use rust_comms::relay::relay_finder::{RelayFinderTrait, RelayInfo};
use rust_comms::relay::relay_updater::RelayUpdater;

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApi, MockApiBoth};

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
        assert_eq!(message.get("type").and_then(|value| value.as_str()), Some("getRelaysStatus"));
        assert_eq!(message.get("app").and_then(|value| value.as_str()), Some("ddb"));

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

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_updater_init_from_blockchain_sets_state_ttl_and_sorts() {
    let updater = RelayUpdater::new(
        "MYID.".to_string(),
        Arc::new(|| {
            vec![
                RelayInfo::root("ZZZ", addr(56003)),
                RelayInfo::root("MYID", addr(56001)),
                RelayInfo::root("AAA", addr(56002)),
            ]
        }),
    );

    updater.init_from_blockchain();

    let cache = updater.relay_info_cache();
    let relays = cache.list_all_relays("MYID", true);
    assert_eq!(relays.len(), 3);
    assert_eq!(relays[0].id, "AAA");
    assert_eq!(relays[1].id, "MYID");
    assert_eq!(relays[2].id, "ZZZ");

    let own = relays
        .iter()
        .find(|relay| relay.id == "MYID")
        .expect("MYID should be present in relay cache");
    assert!(own.is_root);
    assert_eq!(own.state, Some(RelayState::Own));
    assert_eq!(own.ttl, Some(30_000));

    let first_other = relays
        .iter()
        .find(|relay| relay.id == "AAA")
        .expect("AAA should be present in relay cache");
    assert!(first_other.is_root);
    assert_eq!(first_other.state, Some(RelayState::Unknown));
    assert_eq!(first_other.ttl, Some(30));

    let second_other = relays
        .iter()
        .find(|relay| relay.id == "ZZZ")
        .expect("ZZZ should be present in relay cache");
    assert!(second_other.is_root);
    assert_eq!(second_other.state, Some(RelayState::Unknown));
    assert_eq!(second_other.ttl, Some(30));
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_updater_init_from_blockchain_sets_unknown_when_own_not_found() {
    let updater = RelayUpdater::new(
        "MISSING.".to_string(),
        Arc::new(|| {
            vec![
                RelayInfo::root("ROOT1", addr(56101)),
                RelayInfo::root("ROOT2", addr(56102)),
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

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_select_and_query_returns_none_when_no_root_relays() {
    let updater = RelayUpdater::new("MYID.".to_string(), Arc::new(Vec::new));
    assert!(updater.relay_select_and_query().is_none());
}

#[cfg_attr(not(target_os = "ios"), test)]
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
            RelayInfo::root("MYID", addr(56200)),
            RelayInfo::root("ROOT1", addr(56201)),
        ],
        api,
    );
    updater.init_from_blockchain();

    let selected = updater
        .relay_select_and_query()
        .expect("single-root query should succeed");
    assert_eq!(selected.id, "ROOT1");
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
        .find(|relay| relay.id == "MYID")
        .expect("MYID should exist in cache");
    assert_eq!(own.state, Some(RelayState::Own));
    assert_eq!(own.ttl, Some(30_000));

    let root1 = relays
        .iter()
        .find(|relay| relay.id == "ROOT1")
        .expect("ROOT1 should exist in cache");
    assert_eq!(root1.state, Some(RelayState::Available));
    assert_eq!(root1.ttl, Some(300));

    let root2 = relays
        .iter()
        .find(|relay| relay.id == "ROOT2")
        .expect("ROOT2 should exist in cache");
    assert_eq!(root2.state, Some(RelayState::Off));
    assert_eq!(root2.ttl, Some(30));
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_select_and_query_falls_back_to_alternate_when_preferred_fails() {
    let queried_ids = Arc::new(Mutex::new(Vec::new()));

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(QueryMockApi {
        queried_ids: queried_ids.clone(),
        responder: Arc::new(move |queried_id: &str, query_index: usize| {
            if query_index == 0 {
                return Err(BingleError::Other("simulated failure".to_string()));
            }

            let alternate = if queried_id == "ROOTA" { "ROOTB" } else { "ROOTA" };
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
            RelayInfo::root("MYID", addr(56300)),
            RelayInfo::root("ROOTA", addr(56301)),
            RelayInfo::root("ROOTB", addr(56302)),
        ],
        api,
    );
    updater.init_from_blockchain();

    let selected = updater
        .relay_select_and_query()
        .expect("alternate root should be selected");

    let ids = queried_ids
        .lock()
        .expect("queried_ids lock should succeed")
        .clone();
    assert_eq!(ids.len(), 2);
    assert_eq!(selected.id, ids[1]);

    let relays = updater.relay_info_cache().list_all_relays("MYID", true);
    let first = relays
        .iter()
        .find(|relay| relay.id == ids[0])
        .expect("first queried relay should exist in cache");
    assert_eq!(first.state, Some(RelayState::Off));
    assert_eq!(first.ttl, Some(30));

    let second = relays
        .iter()
        .find(|relay| relay.id == ids[1])
        .expect("second queried relay should exist in cache");
    assert_eq!(second.state, Some(RelayState::Available));
    assert_eq!(second.ttl, Some(300));
}

#[cfg_attr(not(target_os = "ios"), test)]
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
            RelayInfo::root("MYID", addr(56400)),
            RelayInfo::root("ROOTA", addr(56401)),
            RelayInfo::root("ROOTB", addr(56402)),
        ],
        api,
    );
    updater.init_from_blockchain();

    assert!(updater.relay_select_and_query().is_none());

    let ids = queried_ids
        .lock()
        .expect("queried_ids lock should succeed")
        .clone();
    assert_eq!(ids.len(), 2);

    let relays = updater.relay_info_cache().list_all_relays("MYID", true);
    for queried_id in ids {
        let relay = relays
            .iter()
            .find(|candidate| candidate.id == queried_id)
            .expect("queried relay should exist in cache");
        assert_eq!(relay.state, Some(RelayState::Off));
        assert_eq!(relay.ttl, Some(30));
    }
}