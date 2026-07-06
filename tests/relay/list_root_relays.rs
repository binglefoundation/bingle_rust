use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak};
use rust_comms::relay::relay_finder::{RelayFinder, RelayFinderTrait};

#[derive(Clone)]
struct MockApi;
impl InnerBingleApi for MockApi {
    fn send_message_to_network_with_response(
        &self,
        _nsk: &rust_comms::api::bingle_api::NetworkEndpoint,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> {
        Err(rust_comms::api::bingle_api::BingleError::Other("ni".into()))
    }
}

#[path = "../test_util.rs"]
pub mod test_util;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn list_root_relays_excludes_self_and_caches() {
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi);
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_clone = calls.clone();
    let discover = Arc::new(move || {
        calls_clone.fetch_add(1, Ordering::SeqCst);
        vec![
            test_util::signed_root_relay("AAA", "127.0.0.1:5001".parse().unwrap()),
            test_util::signed_root_relay("BBB", "127.0.0.1:5002".parse().unwrap()),
        ]
    });

    let finder = RelayFinder::new(to_weak(MockApiBoth::new_with_api_override(api)), discover);
    let list1 = finder.list_root_relays("AAA", false);
    assert_eq!(list1.len(), 1, "should exclude self");
    assert_eq!(list1[0].id(), "BBB");

    // Second call should use cache (no extra discover invocations)
    let list2 = finder.list_root_relays("AAA", false);
    assert_eq!(list2.len(), 1);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "discover should be called only once due to cache"
    );
}
