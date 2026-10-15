// Issue #122: relay selection with permissioned-but-dead relays among the candidates. Probes run
// concurrently, selection is bounded by one short budget, and an unreachable relay is skipped by
// the next pass for a cooldown.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::util::test_util::signed_root_relay;
use bingle_core::api::bingle_api::{BingleError, NetworkEndpoint, ProgressCallback, UserId};
use bingle_core::ddb::InetSocketAddress;
use bingle_core::engine::RelayState;
use bingle_core::messages::marshal::to_json_value;
use bingle_core::messages::types::{DdbMessage, DdbRelaysStatusResponse, Message};
use bingle_core::relay::relay_finder::{RelayFinder, RelayFinderTestTrait, RelayInfo};
use bingle_core::relay::relay_probe_failures::RelayProbeFailures;
use bingle_core::relay::relay_updater::RelayUpdater;

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak_api_both};

const MY_ID: &str = "MYID";
const RELAY_IDS: [&str; 4] = ["RELAYA", "RELAYB", "RELAYC", "RELAYD"];

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

/// How a mocked relay answers a status probe.
#[derive(Clone, Copy)]
enum Behaviour {
    /// Answers Available after the delay.
    Live(Duration),
    /// Fails after the delay, as an unreachable relay does once its connect times out.
    Dead(Duration),
}

struct ScriptedRelays {
    relays: Vec<(String, SocketAddr)>,
    behaviour: Arc<dyn Fn(&str) -> Behaviour + Send + Sync>,
    probed: Arc<Mutex<Vec<String>>>,
}

impl ScriptedRelays {
    fn status_for(&self, responder: &str) -> serde_json::Value {
        let states = self
            .relays
            .iter()
            .map(|(id, _)| match (self.behaviour)(id) {
                Behaviour::Live(_) => RelayState::Available,
                Behaviour::Dead(_) => RelayState::Off,
            })
            .collect();
        let response = DdbRelaysStatusResponse {
            app: "ddb".to_string(),
            responder_state: match (self.behaviour)(responder) {
                Behaviour::Live(_) => RelayState::Available,
                Behaviour::Dead(_) => RelayState::Off,
            },
            epoch_id: 1,
            tree_order: 2,
            relay_ids: self.relays.iter().map(|(id, _)| id.clone()).collect(),
            relay_endpoints: Some(
                self.relays
                    .iter()
                    .map(|(_, a)| InetSocketAddress::from(*a))
                    .collect(),
            ),
            relay_states: states,
            response_tag: None,
            text: None,
            data: None,
        };
        to_json_value(&Message::Ddb(DdbMessage::RelaysStatusResponse(response)))
    }
}

impl InnerBingleApi for ScriptedRelays {
    fn send_message_to_network_with_response_timeout(
        &self,
        _nsk: &NetworkEndpoint,
        user_id: &UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
        _timeout: Duration,
    ) -> Result<serde_json::Value, BingleError> {
        self.probed.lock().unwrap().push(user_id.clone());
        match (self.behaviour)(user_id) {
            Behaviour::Live(delay) => {
                std::thread::sleep(delay);
                Ok(self.status_for(user_id))
            }
            Behaviour::Dead(delay) => {
                std::thread::sleep(delay);
                Err(BingleError::Other("connect timed out".to_string()))
            }
        }
    }
}

fn relay_list(base_port: u16) -> Vec<(String, SocketAddr)> {
    RELAY_IDS
        .iter()
        .enumerate()
        .map(|(i, id)| (id.to_string(), addr(base_port + i as u16)))
        .collect()
}

/// An updater over the four relays, whose probes behave as `behaviour` says.
fn updater(
    base_port: u16,
    behaviour: Arc<dyn Fn(&str) -> Behaviour + Send + Sync>,
) -> (RelayUpdater, Arc<Mutex<Vec<String>>>) {
    let relays = relay_list(base_port);
    let probed = Arc::new(Mutex::new(Vec::new()));
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(ScriptedRelays {
        relays: relays.clone(),
        behaviour,
        probed: probed.clone(),
    });
    let roots: Vec<RelayInfo> = relays
        .iter()
        .map(|(id, a)| signed_root_relay(id, *a))
        .collect();
    let updater = RelayUpdater::new_with_api(
        format!("{MY_ID}."),
        to_weak_api_both(MockApiBoth::new_with_api_override(api)),
        Arc::new(move || roots.clone()),
    );
    updater.init_from_blockchain();
    (updater, probed)
}

/// The relay this node prefers, using the same ranking as relay selection.
fn preferred_relay_id(base_port: u16) -> String {
    let relays: Vec<RelayInfo> = relay_list(base_port)
        .iter()
        .map(|(id, a)| signed_root_relay(id, *a))
        .collect();
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(ScriptedRelays {
        relays: relay_list(base_port),
        behaviour: Arc::new(|_| Behaviour::Live(Duration::ZERO)),
        probed: Arc::new(Mutex::new(Vec::new())),
    });
    let finder = RelayFinder::new(
        to_weak_api_both(MockApiBoth::new_with_api_override(api)),
        Arc::new(Vec::new),
    );
    let (preferred, _) = finder.select_indices(&relays, MY_ID);
    relays[preferred].id().to_string()
}

/// Grace used where a test steps through dead relays, so it runs quickly.
const TEST_GRACE: Duration = Duration::from_millis(300);

#[test]
#[cfg(not(target_os = "ios"))]
fn live_relay_is_selected_promptly_whatever_its_rank_among_dead_ones() {
    // Dead relays take a full connect timeout to fail. Selection moves past each after the grace,
    // so even with the live relay ranked last it is reached after three graces, not three
    // connect timeouts.
    for live in RELAY_IDS {
        let (updater, _probed) = updater(
            57100,
            Arc::new(move |id: &str| {
                if id == live {
                    Behaviour::Live(Duration::from_millis(20))
                } else {
                    Behaviour::Dead(Duration::from_secs(10))
                }
            }),
        );
        let updater = updater.with_selection_timing(Duration::from_secs(12), TEST_GRACE);
        let started = Instant::now();
        let selected = updater
            .relay_select_and_query(&[])
            .unwrap_or_else(|| panic!("live relay {live} should be selected"));
        let took = started.elapsed();
        assert_eq!(selected.id(), live);
        assert!(
            took < TEST_GRACE * 3 + Duration::from_millis(500),
            "selecting {live} took {took:?} with three dead relays"
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn selection_with_only_dead_relays_is_bounded_by_the_budget() {
    let (updater, _probed) = updater(
        57200,
        Arc::new(|_: &str| Behaviour::Dead(Duration::from_secs(10))),
    );
    let updater = updater.with_selection_timing(Duration::from_secs(1), Duration::from_millis(200));
    let started = Instant::now();
    assert!(updater.relay_select_and_query(&[]).is_none());
    let took = started.elapsed();
    assert!(
        took < Duration::from_secs(2),
        "selection took {took:?}, beyond its 1s budget"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_live_preferred_relay_is_the_only_one_contacted() {
    // A client's triangle test needs a corner relay it has not sent to, so selection must not
    // contact more relays than it needs.
    let preferred = preferred_relay_id(57300);
    let (updater, probed) = updater(
        57300,
        Arc::new(|_: &str| Behaviour::Live(Duration::from_millis(300))),
    );
    let selected = updater
        .relay_select_and_query(&[])
        .expect("a relay should be selected");
    assert_eq!(selected.id(), preferred);
    assert_eq!(*probed.lock().unwrap(), vec![preferred]);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_failed_relay_hands_over_to_the_next_without_waiting_for_the_grace() {
    let preferred = preferred_relay_id(57350);
    let dead = preferred.clone();
    let (updater, probed) = updater(
        57350,
        Arc::new(move |id: &str| {
            if id == dead {
                Behaviour::Dead(Duration::ZERO)
            } else {
                Behaviour::Live(Duration::ZERO)
            }
        }),
    );
    let updater = updater.with_selection_timing(Duration::from_secs(12), Duration::from_secs(5));
    let started = Instant::now();
    let selected = updater
        .relay_select_and_query(&[])
        .expect("the alternate should be selected");
    assert_ne!(selected.id(), preferred);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "a fast failure should not wait out the grace"
    );
    assert_eq!(probed.lock().unwrap().len(), 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn unreachable_relays_are_not_reprobed_on_the_next_pass() {
    let (updater, probed) = updater(
        57400,
        Arc::new(|id: &str| {
            if id == "RELAYD" {
                Behaviour::Live(Duration::ZERO)
            } else {
                Behaviour::Dead(Duration::ZERO)
            }
        }),
    );
    assert_eq!(updater.relay_select_and_query(&[]).unwrap().id(), "RELAYD");
    let first_pass_failures: Vec<String> = probed
        .lock()
        .unwrap()
        .iter()
        .filter(|id| *id != "RELAYD")
        .cloned()
        .collect();
    probed.lock().unwrap().clear();

    assert_eq!(updater.relay_select_and_query(&[]).unwrap().id(), "RELAYD");
    let second_pass = probed.lock().unwrap().clone();
    for failed in &first_pass_failures {
        assert!(
            !second_pass.contains(failed),
            "{failed} failed in the first pass and should not be probed again: {second_pass:?}"
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn all_relays_cooling_down_are_probed_anyway() {
    let (updater, probed) = updater(57500, Arc::new(|_: &str| Behaviour::Dead(Duration::ZERO)));
    assert!(updater.relay_select_and_query(&[]).is_none());
    probed.lock().unwrap().clear();

    assert!(updater.relay_select_and_query(&[]).is_none());
    let mut second_pass = probed.lock().unwrap().clone();
    second_pass.sort();
    assert_eq!(
        second_pass, RELAY_IDS,
        "with every relay cooling down, selection should still try them all"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_shared_negative_cache_carries_failures_to_a_new_updater() {
    let failures = Arc::new(RelayProbeFailures::new());
    let dead = preferred_relay_id(57600);
    let dead_for_closure = dead.clone();
    let behaviour: Arc<dyn Fn(&str) -> Behaviour + Send + Sync> = Arc::new(move |id: &str| {
        if id == dead_for_closure {
            Behaviour::Dead(Duration::ZERO)
        } else {
            Behaviour::Live(Duration::ZERO)
        }
    });

    let (first, _) = updater(57600, behaviour.clone());
    let first = first.with_probe_failures(failures.clone());
    assert!(first.relay_select_and_query(&[]).is_some());
    assert!(failures.is_cooling_down(&dead));

    // A new updater (as a new relay finder creates) sharing the cache skips the dead relay.
    let (second, probed) = updater(57600, behaviour);
    let second = second.with_probe_failures(failures);
    assert!(second.relay_select_and_query(&[]).is_some());
    assert!(
        !probed.lock().unwrap().contains(&dead),
        "the dead relay should not be probed again"
    );
}
