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
        response_tag: None,
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
