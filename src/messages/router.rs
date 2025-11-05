use crate::messages::handlers::MessageHandler;
use crate::messages::types::*;

// Global sender factory used by handlers to send replies without tight coupling to API implementation.
use std::sync::{Arc, OnceLock, Mutex};
use crate::api::bingle_api::{NetworkSourceKey, UserId};

static SENDER_GLOBAL: OnceLock<Mutex<Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>>>> = OnceLock::new();

pub fn set_sender(cb: Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>>) {
    let cell = SENDER_GLOBAL.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = cell.lock() { *g = cb; }
}

pub fn get_sender() -> Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>> {
    let cell = SENDER_GLOBAL.get_or_init(|| Mutex::new(None));
    match cell.lock() { Ok(g) => g.clone(), Err(_) => None }
}

pub fn route<H: MessageHandler + ?Sized>(handler: &H, msg: &Message, from_id: &str) {
    match msg {
        Message::PlainText(pt) => handler.on_plain_text(from_id, pt),
        Message::Relay(r) => match r {
            RelayMessage::Call(m) => handler.on_relay_call(from_id, m),
            RelayMessage::RelayResponse(m) => handler.on_relay_response(from_id, m),
            RelayMessage::TriangleTest1(m) => handler.on_triangle_test1(from_id, m),
            RelayMessage::TriangleTest2(m) => handler.on_triangle_test2(from_id, m),
            RelayMessage::TriangleTest3(m) => handler.on_triangle_test3(from_id, m),
            RelayMessage::Listen(m) => handler.on_relay_listen(from_id, m),
            RelayMessage::Check(m) => handler.on_relay_check(from_id, m),
            RelayMessage::ListenResponse(m) => handler.on_relay_listen_response(from_id, m),
            RelayMessage::CheckResponse(m) => handler.on_relay_check_response(from_id, m),
            RelayMessage::CallResponse(m) => handler.on_relay_call_response(from_id, m),
            RelayMessage::KeepAlive(m) => handler.on_relay_keep_alive(from_id, m),
        },
        Message::Unknown(v) => handler.on_unknown(v),
    }
}
