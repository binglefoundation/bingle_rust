use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_comms::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId, BingleError};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo, RelayFinderTrait};

#[path = "../test_util.rs"]
pub mod test_util;
use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth};

struct MockApi {
    check_calls: Arc<Mutex<usize>>,
    get_epoch_calls: Arc<Mutex<usize>>,
    get_epoch_fail_for: Arc<Mutex<Option<SocketAddr>>>,
}

impl MockApi {
    fn new() -> Self {
        Self {
            check_calls: Arc::new(Mutex::new(0)),
            get_epoch_calls: Arc::new(Mutex::new(0)),
            get_epoch_fail_for: Arc::new(Mutex::new(None)),
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
        if ty == "getEpoch" {
            let mut count = self.get_epoch_calls.lock().unwrap();
            *count += 1;
            let addr = nsk.inet_socket_address().expect("direct endpoint required");
            if let Some(fail_addr) = *self.get_epoch_fail_for.lock().unwrap() {
                if addr == fail_addr {
                    return Err(BingleError::Other("DDB query failed".into()));
                }
            }
            return Ok(serde_json::json!({
                "app": "ddb",
                "type": "getEpochResponse",
                "epochId": -1,
                "treeOrder": 2,
                "relayIds": [],
                "relayEndpoints": [],
            }));
        }
        Err(BingleError::Other("unexpected message".into()))
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_unavailable_relays_no_retry() {
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001);

    let api_inner = Arc::new(MockApi::new());
    let check_calls = api_inner.check_calls.clone();
    
    // discover_roots returns the same relay twice
    let discover = {
        let r1 = RelayInfo { id: id1.clone(), address: addr1, state: None };
        let rs = vec![r1.clone(), r1];
        Arc::new(move || rs.clone())
    };

    let finder = RelayFinder::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(api_inner)),
        Duration::from_secs(30),
        discover
    );

    // load_relay_states will call list_all_relays, which will get [r1, r1] from discover fallback
    // then it will iterate and call relay_check for each.
    finder.load_relay_states("MYID");

    // The first relay_check for r1 should fail and add to unavailable_relays.
    // The second relay_check for r1 should see it in the list and skip.
    assert_eq!(*check_calls.lock().unwrap(), 1, "Should only have called Check once even if relay appears twice");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_unavailable_relays_reset_on_entry() {
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001);

    let api_inner = Arc::new(MockApi::new());
    let check_calls = api_inner.check_calls.clone();
    
    let discover = {
        let r1 = RelayInfo { id: id1.clone(), address: addr1, state: None };
        Arc::new(move || vec![r1.clone()])
    };

    let finder = RelayFinder::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(api_inner)),
        Duration::from_secs(30),
        discover
    );

    // First call
    finder.load_relay_states("MYID");
    assert_eq!(*check_calls.lock().unwrap(), 1);

    // Second call - should reset and try again
    finder.load_relay_states("MYID");
    assert_eq!(*check_calls.lock().unwrap(), 2, "Should have reset unavailable list and tried again");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_unavailable_relays_reset_on_find_relay() {
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001);

    let api_inner = Arc::new(MockApi::new());
    let check_calls = api_inner.check_calls.clone();
    
    let discover = {
        let r1 = RelayInfo { id: id1.clone(), address: addr1, state: None };
        Arc::new(move || vec![r1.clone()])
    };

    let finder = RelayFinder::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(api_inner)),
        Duration::from_secs(30),
        discover
    );

    finder.load_relay_states("MYID");
    assert_eq!(*check_calls.lock().unwrap(), 1);

    // find_relay should reset and try again
    let _ = finder.find_relay("MYID");
    assert_eq!(*check_calls.lock().unwrap(), 2);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_find_relay_respects_ddb_failure_internal() {
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001);

    let api_inner = Arc::new(MockApi::new());
    *api_inner.get_epoch_fail_for.lock().unwrap() = Some(addr1);
    let check_calls = api_inner.check_calls.clone();
    let get_epoch_calls = api_inner.get_epoch_calls.clone();
    
    // R1 is the only root
    let discover = {
        let r1 = RelayInfo { id: id1.clone(), address: addr1, state: None };
        Arc::new(move || vec![r1.clone()])
    };

    let finder = RelayFinder::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(api_inner)),
        Duration::from_secs(30),
        discover
    );

    // Call find_relay. It should fail because R1 is the only relay and it will be marked unavailable.
    let res = finder.find_relay("MYID");
    assert!(res.is_err());
    
    assert_eq!(*get_epoch_calls.lock().unwrap(), 1, "Should have attempted DDB query to R1");
    // Since R1 failed DDB query, it should be in unavailable list.
    // find_relay_internal calls relay_check(R1).
    // relay_check(R1) should see R1 is unavailable and skip network call.
    assert_eq!(*check_calls.lock().unwrap(), 0, "Should have skipped Check for the relay that failed DDB query");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_unavailable_relays_cleared_on_all_external_methods() {
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001);

    let api_inner = Arc::new(MockApi::new());
    let check_calls = api_inner.check_calls.clone();
    
    let discover = {
        let r1 = RelayInfo { id: id1.clone(), address: addr1, state: None };
        Arc::new(move || vec![r1.clone()])
    };

    let finder = RelayFinder::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(api_inner.clone())),
        Duration::from_secs(30),
        discover
    );

    // Helper to mark id1 as unavailable
    let mark_unavailable = |f: &RelayFinder| {
        f.load_relay_states("MYID");
    };

    mark_unavailable(&finder);
    assert_eq!(*check_calls.lock().unwrap(), 1);
    assert_eq!(*api_inner.get_epoch_calls.lock().unwrap(), 1);

    // list_all_relays clears
    let _ = finder.list_all_relays("MYID", false);
    // If it cleared, list_all_relays_internal will query R1 for DDB again.
    assert_eq!(*api_inner.get_epoch_calls.lock().unwrap(), 2);

    mark_unavailable(&finder);
    assert_eq!(*check_calls.lock().unwrap(), 2);

    // list_root_relays clears
    let _ = finder.list_root_relays("MYID", false);
    // Verify it's cleared by calling find_relay and checking if it tries R1 again
    let _ = finder.find_relay("MYID");
    assert_eq!(*check_calls.lock().unwrap(), 3);

    mark_unavailable(&finder);
    assert_eq!(*check_calls.lock().unwrap(), 4);

    // lookup_root_id clears
    let _ = finder.lookup_root_id(&id1);
    let _ = finder.find_relay("MYID");
    assert_eq!(*check_calls.lock().unwrap(), 5);

    mark_unavailable(&finder);
    assert_eq!(*check_calls.lock().unwrap(), 6);

    // find_relay_excluding clears
    let _ = finder.find_relay_excluding("MYID", &[]);
    assert_eq!(*check_calls.lock().unwrap(), 7);

    mark_unavailable(&finder);
    assert_eq!(*check_calls.lock().unwrap(), 8);

    // clear_state_cache clears
    finder.clear_state_cache();
    let _ = finder.find_relay("MYID");
    assert_eq!(*check_calls.lock().unwrap(), 9);
}
