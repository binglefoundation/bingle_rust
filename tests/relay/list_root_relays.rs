use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::Duration;

use rust_comms::relay::relay_finder::{RelayFinder, RootRelayInfo, RelayFinderTrait};
use crate::util::reusable_mock_api::{to_weak, InnerBingleApi, MockApiBoth};

#[derive(Clone)]
struct MockApi;
impl InnerBingleApi for MockApi { 
    fn send_message_to_network_with_response(&self, _nsk: &rust_comms::api::bingle_api::NetworkEndpoint, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> { Err(rust_comms::api::bingle_api::BingleError::Other("ni".into())) }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn list_root_relays_excludes_self_and_caches() {
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi);
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_clone = calls.clone();
    let discover = Arc::new(move || {
        calls_clone.fetch_add(1, Ordering::SeqCst);
        vec![
            RootRelayInfo { id: "AAA".into(), address: "127.0.0.1:5001".parse().unwrap(), state: None, ttl: None },
            RootRelayInfo { id: "BBB".into(), address: "127.0.0.1:5002".parse().unwrap(), state: None, ttl: None },
        ]
    });

    let finder = RelayFinder::new(to_weak(MockApiBoth::new_with_api_override(api)), Duration::from_millis(2000), discover);
    let list1 = finder.list_root_relays("AAA", false);
    assert_eq!(list1.len(), 1, "should exclude self");
    assert_eq!(list1[0].id, "BBB");

    // Second call should use cache (no extra discover invocations)
    let list2 = finder.list_root_relays("AAA", false);
    assert_eq!(list2.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "discover should be called only once due to cache");
}
