use std::sync::{Arc, Barrier, mpsc};
use std::time::Duration;

use crate::util::reusable_mock_api::MockApiBoth;
use bingle_core::api::bingle_api::{BingleApiBoth, NetworkEndpoint};
use bingle_core::messages::handlers::{FromStruct, MessageHandler};
use bingle_core::messages::router::Router;
use bingle_core::messages::types::PingPing;
use bingle_core::messages::{Message, PingMessage};

// Handler that uses a barrier to synchronize two concurrent dispatches so both
// write their response before either thread drains the shared queue. This forces
// the race condition to manifest deterministically.
struct BarrierTaggedHandler {
    barrier: Arc<Barrier>,
}
impl MessageHandler for BarrierTaggedHandler {
    fn on_ping_ping(&self, _api: Arc<dyn BingleApiBoth>, from: &FromStruct, msg: &PingPing) {
        let tag = msg.text.clone().unwrap_or_default();
        // Write our response first
        from.push_response(serde_json::json!({"type": "response", "app": "ping", "tag": tag}));
        // Wait for the other thread to also write its response before either drains.
        // After the barrier, both responses are in the shared queue; the drain in
        // route_with_network will pick them up but send them both to whichever
        // `from` context it holds — potentially the wrong one.
        self.barrier.wait();
    }
}

struct DelayedOutboundResponseHandler;

impl MessageHandler for DelayedOutboundResponseHandler {
    fn on_ping_ping(&self, _api: Arc<dyn BingleApiBoth>, from: &FromStruct, _msg: &PingPing) {
        std::thread::sleep(Duration::from_millis(150));
        from.push_response(serde_json::json!({"type": "response", "app": "ping"}));
    }
}

struct DelayedMultipleOutboundResponsesHandler;

impl MessageHandler for DelayedMultipleOutboundResponsesHandler {
    fn on_ping_ping(&self, _api: Arc<dyn BingleApiBoth>, from: &FromStruct, _msg: &PingPing) {
        std::thread::sleep(Duration::from_millis(150));
        from.push_response(serde_json::json!({"type": "response", "app": "ping", "seq": 1}));
        from.push_response(serde_json::json!({"type": "response", "app": "ping", "seq": 2}));
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn outbound_response_should_be_sent_from_router_processing_thread() {
    let router = Arc::new(Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
    ));
    let (sent_tx, sent_rx) = mpsc::channel::<(NetworkEndpoint, String, serde_json::Value)>();
    router.set_sender(Some(Arc::new(move |nsk, user_id, json| {
        sent_tx.send((nsk.clone(), user_id.clone(), json)).is_ok()
    })));

    let message = Message::Ping(PingMessage::Ping(PingPing {
        app: "ping".to_string(),
        tag: None,
        response_tag: None,
        text: Some("race".to_string()),
        data: None,
    }));
    let endpoint = "127.0.0.1:12001".parse().expect("endpoint should parse");
    let nsk = NetworkEndpoint::new_direct(endpoint);

    router.route_with_network(
        DelayedOutboundResponseHandler,
        &message,
        "SENDER.ISSUER",
        &nsk,
    );

    let (sent_nsk, sent_user_id, sent_json) = sent_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("expected response to be sent from router thread");

    assert_eq!(format!("{}", sent_nsk), format!("{}", nsk));
    assert_eq!(sent_user_id, "SENDER.ISSUER");
    assert_eq!(
        sent_json,
        serde_json::json!({"type": "response", "app": "ping"})
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn outbound_response_queue_should_send_all_responses_from_router_processing_thread() {
    let router = Arc::new(Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
    ));
    let (sent_tx, sent_rx) = mpsc::channel::<(NetworkEndpoint, String, serde_json::Value)>();
    router.set_sender(Some(Arc::new(move |nsk, user_id, json| {
        sent_tx.send((nsk.clone(), user_id.clone(), json)).is_ok()
    })));

    let message = Message::Ping(PingMessage::Ping(PingPing {
        app: "ping".to_string(),
        tag: None,
        response_tag: None,
        text: Some("queue".to_string()),
        data: None,
    }));
    let endpoint = "127.0.0.1:12002".parse().expect("endpoint should parse");
    let nsk = NetworkEndpoint::new_direct(endpoint);

    router.route_with_network(
        DelayedMultipleOutboundResponsesHandler,
        &message,
        "SENDER.ISSUER",
        &nsk,
    );

    let (first_nsk, first_user_id, first_json) = sent_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("expected first queued response to be sent from router thread");
    let (second_nsk, second_user_id, second_json) = sent_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("expected second queued response to be sent from router thread");

    assert_eq!(format!("{}", first_nsk), format!("{}", nsk));
    assert_eq!(first_user_id, "SENDER.ISSUER");
    assert_eq!(
        first_json,
        serde_json::json!({"type": "response", "app": "ping", "seq": 1})
    );

    assert_eq!(format!("{}", second_nsk), format!("{}", nsk));
    assert_eq!(second_user_id, "SENDER.ISSUER");
    assert_eq!(
        second_json,
        serde_json::json!({"type": "response", "app": "ping", "seq": 2})
    );

    assert!(
        sent_rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "expected exactly two outbound responses"
    );
}

/// Regression test for the concurrent-dispatch race condition.
///
/// Two messages arrive at nearly the same time from different senders. Each handler
/// writes its response to the shared outbound-response queue. With the old
/// shared-queue design, Thread A's drain may pick up Thread B's response (or vice
/// versa), sending the wrong response to the wrong requester.
///
/// After the fix (per-call `FromStruct.responses`), each response must arrive at
/// exactly the sender that produced it.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn concurrent_dispatch_responses_should_not_cross_contaminate() {
    let router = Arc::new(Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
    ));
    let (sent_tx, sent_rx) = mpsc::channel::<(NetworkEndpoint, String, serde_json::Value)>();
    let sent_tx2 = sent_tx.clone();
    router.set_sender(Some(Arc::new(move |nsk, user_id, json| {
        sent_tx2.send((nsk.clone(), user_id.clone(), json)).is_ok()
    })));

    let make_message = |tag: &str| {
        Message::Ping(PingMessage::Ping(PingPing {
            app: "ping".to_string(),
            tag: None,
            response_tag: None,
            text: Some(tag.to_string()),
            data: None,
        }))
    };

    let endpoint_a: std::net::SocketAddr =
        "127.0.0.1:12101".parse().expect("endpoint should parse");
    let endpoint_b: std::net::SocketAddr =
        "127.0.0.1:12102".parse().expect("endpoint should parse");
    let nsk_a = NetworkEndpoint::new_direct(endpoint_a);
    let nsk_b = NetworkEndpoint::new_direct(endpoint_b);

    // Use a barrier so both handlers write before either drains, forcing the race.
    let barrier = Arc::new(Barrier::new(2));
    router.route_with_network(
        BarrierTaggedHandler {
            barrier: barrier.clone(),
        },
        &make_message("tag-a"),
        "SENDER_A.ISSUER",
        &nsk_a,
    );
    router.route_with_network(
        BarrierTaggedHandler {
            barrier: barrier.clone(),
        },
        &make_message("tag-b"),
        "SENDER_B.ISSUER",
        &nsk_b,
    );

    // Collect both responses (order is non-deterministic)
    let r1 = sent_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("expected first concurrent response");
    let r2 = sent_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("expected second concurrent response");

    let mut responses = vec![r1, r2];
    responses.sort_by_key(|(_, user_id, _)| user_id.clone());

    let (nsk_got_a, uid_a, json_a) = &responses[0];
    let (nsk_got_b, uid_b, json_b) = &responses[1];

    // Each response must have been sent to the correct sender with the correct tag
    assert_eq!(
        uid_a, "SENDER_A.ISSUER",
        "first response must go to SENDER_A"
    );
    assert_eq!(
        format!("{}", nsk_got_a),
        format!("{}", nsk_a),
        "first response must be sent to endpoint_a"
    );
    assert_eq!(
        json_a["tag"],
        serde_json::json!("tag-a"),
        "first response must carry tag-a"
    );

    assert_eq!(
        uid_b, "SENDER_B.ISSUER",
        "second response must go to SENDER_B"
    );
    assert_eq!(
        format!("{}", nsk_got_b),
        format!("{}", nsk_b),
        "second response must be sent to endpoint_b"
    );
    assert_eq!(
        json_b["tag"],
        serde_json::json!("tag-b"),
        "second response must carry tag-b"
    );

    assert!(
        sent_rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "expected exactly two outbound responses total"
    );
}
