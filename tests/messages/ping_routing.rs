use rust_comms::messages::Router;
use std::sync::{Arc, Mutex};

use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::types::{PingMessage, PingPing};
use rust_comms::messages::Message;

use crate::util::reusable_mock_api::MockApiBoth;
use rust_comms::api::bingle_api::{BingleApiBoth};

struct CapturingHandler {
    called: Arc<Mutex<bool>>,
}

impl CapturingHandler { fn new(flag: Arc<Mutex<bool>>) -> Self { Self { called: flag } } }

impl MessageHandler for CapturingHandler {
    fn on_ping_ping(&self, _api: Arc<dyn BingleApiBoth>, _from: &rust_comms::messages::handlers::FromStruct, msg: &PingPing) {
        // Ensure we received the ping message with expected fields
        assert_eq!(msg.app, "ping");
        assert_eq!(msg.text.as_deref(), Some("hello"));
        if let Ok(mut g) = self.called.lock() { *g = true; }
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn route_invokes_on_ping_ping() {
    let flag = Arc::new(Mutex::new(false));
    let handler = CapturingHandler::new(flag.clone());

    if let Some(router) = Router::current() {
        // Provide API to router so it can be passed into handler per new signature
        router.set_bingle_api(Some(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new())));

        let ping = PingPing { app: "ping".into(), tag: None, response_tag: None, text: Some("hello".into()), data: None };
        let msg = Message::Ping(PingMessage::Ping(ping));
        router.route(&handler, &msg, "SOMEISSUER.");

        let got = flag.lock().unwrap().clone();
        assert!(got, "on_ping_ping was not called");
    }
}
