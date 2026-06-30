use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_comms::api::bingle_api::{BingleError, NetworkEndpoint, ProgressCallback, UserId};
use rust_comms::messages::handlers::{send_triangle_test1_response, TRIANGLE_TEST_1_DELAY};

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth};

// Short delay used in tests so they complete quickly.
const TEST_DELAY: Duration = Duration::from_millis(150);

struct TrackingApi {
    send_times: Mutex<Vec<Instant>>,
    send_messages: Mutex<Vec<serde_json::Value>>,
}

impl TrackingApi {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            send_times: Mutex::new(Vec::new()),
            send_messages: Mutex::new(Vec::new()),
        })
    }

    fn sent_count(&self) -> usize {
        self.send_times.lock().unwrap().len()
    }

    fn sent_message(&self, idx: usize) -> serde_json::Value {
        self.send_messages.lock().unwrap()[idx].clone()
    }
}

impl InnerBingleApi for TrackingApi {
    fn send_message_to_network(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        self.send_times.lock().unwrap().push(Instant::now());
        self.send_messages.lock().unwrap().push(message);
        Ok(true)
    }
}

fn make_api(tracking: Arc<TrackingApi>) -> Arc<dyn rust_comms::api::bingle_api::BingleApiBoth> {
    Arc::new(MockApiBoth::new_with_api_override(tracking))
}

fn assert_response_tag(msg: &serde_json::Value, expected: Option<&str>) {
    let tag = msg.get("responseTag").and_then(|v| v.as_str());
    assert_eq!(tag, expected, "responseTag mismatch in {:?}", msg);
}

/// When a corner node is available, TriangleTest1Response must not be sent before the delay expires,
/// and must carry the correct responseTag.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn response_not_sent_before_delay_when_corner_node_found() {
    let tracking = TrackingApi::new();
    let api = make_api(tracking.clone());
    let nsk = NetworkEndpoint::new_direct("127.0.0.1:19001".parse().unwrap());

    let t0 = Instant::now();
    let api2 = api.clone();
    let nsk2 = nsk.clone();
    let handle = std::thread::spawn(move || {
        send_triangle_test1_response(&api2, &nsk2, "USER123", Some("tag-abc".to_string()), false, TEST_DELAY);
    });

    // Poll for a window shorter than TEST_DELAY — response must not arrive yet.
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(tracking.sent_count(), 0, "TriangleTest1Response must not be sent before the delay expires");

    handle.join().unwrap();

    assert_eq!(tracking.sent_count(), 1, "TriangleTest1Response should be sent exactly once");
    assert!(t0.elapsed() >= TEST_DELAY, "total elapsed time should be at least TEST_DELAY");
    assert_response_tag(&tracking.sent_message(0), Some("tag-abc"));
}

/// When no corner node is available, TriangleTest1Response is sent immediately with the correct tag.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn response_sent_immediately_when_no_corner_node() {
    let tracking = TrackingApi::new();
    let api = make_api(tracking.clone());
    let nsk = NetworkEndpoint::new_direct("127.0.0.1:19002".parse().unwrap());

    let t0 = Instant::now();
    send_triangle_test1_response(&api, &nsk, "USER123", Some("tag-xyz".to_string()), true, Duration::ZERO);

    assert!(t0.elapsed() < Duration::from_millis(100), "no-corner-node response should be immediate");
    assert_eq!(tracking.sent_count(), 1, "response should be sent exactly once");
    assert_response_tag(&tracking.sent_message(0), Some("tag-xyz"));
}

/// When no response tag is provided, the field is absent from the JSON.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn response_tag_absent_when_none() {
    let tracking = TrackingApi::new();
    let api = make_api(tracking.clone());
    let nsk = NetworkEndpoint::new_direct("127.0.0.1:19003".parse().unwrap());

    send_triangle_test1_response(&api, &nsk, "USER123", None, false, Duration::ZERO);

    assert_eq!(tracking.sent_count(), 1);
    assert_response_tag(&tracking.sent_message(0), None);
}

/// Sanity check that the production delay constant is 10 seconds.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn triangle_test1_delay_constant_is_10s() {
    assert_eq!(TRIANGLE_TEST_1_DELAY, Duration::from_secs(10));
}
