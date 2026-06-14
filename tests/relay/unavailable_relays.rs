use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId, BingleError};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo, RelayFinderTrait};
use crate::ddb::ddb_client_lookup::test_util::init_test_logging;

#[path = "../test_util.rs"]
pub mod test_util;
use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth};

struct MockApi {
    check_calls: Arc<Mutex<usize>>,
    get_relays_status_calls: Arc<Mutex<usize>>,
    get_relays_status_fail_for: Arc<Mutex<Option<SocketAddr>>>,
    relay_id: String,
    relay_addr: SocketAddr,
}

impl MockApi {
    fn new(relay_id: String, relay_addr: SocketAddr) -> Self {
        Self {
            check_calls: Arc::new(Mutex::new(0)),
            get_relays_status_calls: Arc::new(Mutex::new(0)),
            get_relays_status_fail_for: Arc::new(Mutex::new(None)),
            relay_id,
            relay_addr,
        }
    }
}

impl InnerBingleApi for MockApi {
    fn send_message_to_network_with_response(&self, nsk: &NetworkEndpoint, _user_id: &UserId, message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, BingleError> {
        let ty = message.get("type").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
        if ty == "Check" {
            let mut count = self.check_calls.lock().unwrap();
            *count += 1;
            // Always fail
            return Err(BingleError::Other("Connection failed".into()));
        }
        if ty == "getRelaysStatus" {
            let mut count = self.get_relays_status_calls.lock().unwrap();
            *count += 1;
            let addr = nsk.inet_socket_address().expect("direct endpoint required");
            if let Some(fail_addr) = *self.get_relays_status_fail_for.lock().unwrap() {
                if addr == fail_addr {
                    return Err(BingleError::Other("DDB query failed".into()));
                }
            }
            let host = self.relay_addr.ip().to_string();
            let port = self.relay_addr.port();
            return Ok(serde_json::json!({
                "app": "ddb",
                "type": "relaysStatusResponse",
                "epochId": -1,
                "treeOrder": 2,
                "responderState": "available",
                "relayIds": [self.relay_id],
                "relayEndpoints": [{"host": host, "port": port}],
                "relayStates": ["available"],
            }));
        }
        Err(BingleError::Other("unexpected message".into()))
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_unavailable_relays_no_retry() {
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001);

    let api_inner = Arc::new(MockApi::new(id1.clone(), addr1));
    let get_relays_status_calls = api_inner.get_relays_status_calls.clone();

    // discover_roots returns the same relay twice (deduped internally by relay_select_and_query)
    let discover = {
        let r1 = RelayInfo::root(id1.clone(), addr1);
        let rs = vec![r1.clone(), r1];
        Arc::new(move || rs.clone())
    };

    let finder = RelayFinder::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(api_inner)),
        discover
    );

    // find_relay uses relay_select_and_query which calls getRelaysStatus (not relay_check/Check).
    // Even though discover_roots returns r1 twice, relay_select_and_query deduplicates by sorting & iterating.
    let _ = finder.find_relay("MYID");

    // getRelaysStatus is called once per candidate attempt via relay_select_and_query.
    assert_eq!(*get_relays_status_calls.lock().unwrap(), 1, "Should only have called getRelaysStatus once");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_unavailable_relays_reset_on_entry() {
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001);

    let api_inner = Arc::new(MockApi::new(id1.clone(), addr1));
    let get_relays_status_calls = api_inner.get_relays_status_calls.clone();

    let discover = {
        let r1 = RelayInfo::root(id1.clone(), addr1);
        Arc::new(move || vec![r1.clone()])
    };

    let finder = RelayFinder::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(api_inner)),
        discover
    );

    // First call: relay_select_and_query queries r1 via getRelaysStatus
    let _ = finder.find_relay("MYID");
    assert_eq!(*get_relays_status_calls.lock().unwrap(), 1);

    // Second call: result is cached but we still do a status check
    let _ = finder.find_relay("MYID");
    assert_eq!(*get_relays_status_calls.lock().unwrap(), 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_unavailable_relays_reset_on_find_relay() {
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001);

    let api_inner = Arc::new(MockApi::new(id1.clone(), addr1));
    let get_relays_status_calls = api_inner.get_relays_status_calls.clone();

    let discover = {
        let r1 = RelayInfo::root(id1.clone(), addr1);
        Arc::new(move || vec![r1.clone()])
    };

    let finder = RelayFinder::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(api_inner)),
        discover
    );

    let _ = finder.find_relay("MYID");
    assert_eq!(*get_relays_status_calls.lock().unwrap(), 1);

    // Second call will now check status
    let _ = finder.find_relay("MYID");
    assert_eq!(*get_relays_status_calls.lock().unwrap(), 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_find_relay_respects_ddb_failure_internal() {
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001);

    let api_inner = Arc::new(MockApi::new(id1.clone(), addr1));
    *api_inner.get_relays_status_fail_for.lock().unwrap() = Some(addr1);
    let check_calls = api_inner.check_calls.clone();
    let get_relays_status_calls = api_inner.get_relays_status_calls.clone();
    
    // R1 is the only root
    let discover = {
        let r1 = RelayInfo::root(id1.clone(), addr1);
        Arc::new(move || vec![r1.clone()])
    };

    let finder = RelayFinder::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(api_inner)),
        discover
    );

    // Call find_relay. It should fail because R1 is the only relay and it will be marked unavailable.
    let res = finder.find_relay("MYID");
    assert!(res.is_err());
    
    assert_eq!(*get_relays_status_calls.lock().unwrap(), 1, "Should have attempted DDB query to R1");
    // Since R1 failed DDB query, it should be in unavailable list.
    // find_relay_internal calls relay_check(R1).
    // relay_check(R1) should see R1 is unavailable and skip network call.
    assert_eq!(*check_calls.lock().unwrap(), 0, "Should have skipped Check for the relay that failed DDB query");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_unavailable_relays_cleared_on_all_external_methods() {
    init_test_logging();

    let id1 = test_util::ADDRESS_SPEND.to_string();
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001);

    let api_inner = Arc::new(MockApi::new(id1.clone(), addr1));
    let get_relays_status_calls = api_inner.get_relays_status_calls.clone();

    let discover = {
        let r1 = RelayInfo::root(id1.clone(), addr1);
        Arc::new(move || vec![r1.clone()])
    };

    let finder = RelayFinder::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(api_inner.clone())),
        discover
    );

    // find_relay calls relay_select_and_query which sends getRelaysStatus, then caches result
    let _ = finder.find_relay("MYID");
    assert_eq!(*get_relays_status_calls.lock().unwrap(), 1);

    // list_all_relays bypasses the finder cache and calls relay_select_and_query again
    let _ = finder.list_all_relays("MYID", false);
    assert_eq!(*get_relays_status_calls.lock().unwrap(), 2);

    // find_relay returns cresult after a status check
    let _ = finder.find_relay("MYID");
    assert_eq!(*get_relays_status_calls.lock().unwrap(), 3);

    // find_relay_excluding returns cached result after a status check
    let _ = finder.find_relay_excluding("MYID", &[]);
    assert_eq!(*get_relays_status_calls.lock().unwrap(), 4);

    // clear_state_cache clears finder cache so next find_relay calls relay_select_and_query
    finder.clear_state_cache();
    let _ = finder.find_relay("MYID");
    assert_eq!(*get_relays_status_calls.lock().unwrap(), 5);
}
