use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::util::reusable_mock_api::MockApiBoth;
use rust_comms::api::bingle_api::{BingleApiBoth, NetworkEndpoint};
use rust_comms::messages::handlers::{FromStruct, MessageHandler};
use rust_comms::messages::router::Router;
use rust_comms::messages::types::PingPing;
use rust_comms::messages::{Message, PingMessage};

struct DelayedOutboundResponseHandler;

impl MessageHandler for DelayedOutboundResponseHandler {
    fn on_ping_ping(&self, _api: Arc<dyn BingleApiBoth>, from: &FromStruct, _msg: &PingPing) {
        std::thread::sleep(Duration::from_millis(150));
        from
            .router
            .set_outbound_response(Some(serde_json::json!({"type": "response", "app": "ping"})));
    }
}

struct DelayedMultipleOutboundResponsesHandler;

impl MessageHandler for DelayedMultipleOutboundResponsesHandler {
    fn on_ping_ping(&self, _api: Arc<dyn BingleApiBoth>, from: &FromStruct, _msg: &PingPing) {
        std::thread::sleep(Duration::from_millis(150));
        from
            .router
            .set_outbound_response(Some(serde_json::json!({"type": "response", "app": "ping", "seq": 1})));
        from
            .router
            .set_outbound_response(Some(serde_json::json!({"type": "response", "app": "ping", "seq": 2})));
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn outbound_response_should_be_sent_from_router_processing_thread() {
    let router = Arc::new(Router::new(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new())));
    let (sent_tx, sent_rx) = mpsc::channel::<(NetworkEndpoint, String, serde_json::Value)>();
    router.set_sender(Some(Arc::new(move |nsk, user_id, json| {
        sent_tx.send((nsk.clone(), user_id.clone(), json)).is_ok()
    })));

    let message = Message::Ping(PingMessage::Ping(PingPing {
        app: "ping".to_string(),
        tag: None,
        text: Some("race".to_string()),
        data: None,
    }));
    let endpoint = "127.0.0.1:12001"
        .parse()
        .expect("endpoint should parse");
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
    assert_eq!(sent_json, serde_json::json!({"type": "response", "app": "ping"}));
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn outbound_response_queue_should_send_all_responses_from_router_processing_thread() {
    let router = Arc::new(Router::new(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new())));
    let (sent_tx, sent_rx) = mpsc::channel::<(NetworkEndpoint, String, serde_json::Value)>();
    router.set_sender(Some(Arc::new(move |nsk, user_id, json| {
        sent_tx.send((nsk.clone(), user_id.clone(), json)).is_ok()
    })));

    let message = Message::Ping(PingMessage::Ping(PingPing {
        app: "ping".to_string(),
        tag: None,
        text: Some("queue".to_string()),
        data: None,
    }));
    let endpoint = "127.0.0.1:12002"
        .parse()
        .expect("endpoint should parse");
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
    assert_eq!(first_json, serde_json::json!({"type": "response", "app": "ping", "seq": 1}));

    assert_eq!(format!("{}", second_nsk), format!("{}", nsk));
    assert_eq!(second_user_id, "SENDER.ISSUER");
    assert_eq!(second_json, serde_json::json!({"type": "response", "app": "ping", "seq": 2}));

    assert!(
        sent_rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "expected exactly two outbound responses"
    );
}
