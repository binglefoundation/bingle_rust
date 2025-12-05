use crate::messages::handlers::MessageHandler;
use crate::messages::types::*;

// Global sender factory used by handlers to send replies without tight coupling to API implementation.
use std::sync::{Arc, OnceLock, Mutex};
use std::net::SocketAddr;
use crate::api::bingle_api::{NetworkSourceKey, UserId, BingleApi};
use crate::api::bingle_api::BingleApiInternal;

static SENDER_GLOBAL: OnceLock<Mutex<Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>>>> = OnceLock::new();
static API_GLOBAL: OnceLock<Mutex<Option<Arc<dyn BingleApi>>>> = OnceLock::new();
static API_INTERNAL_GLOBAL: OnceLock<Mutex<Option<Arc<dyn BingleApiInternal>>>> = OnceLock::new();
static LAST_FROM_GLOBAL: OnceLock<Mutex<Option<SocketAddr>>> = OnceLock::new();
static LAST_RESPONSE_TAG: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub fn set_sender(cb: Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>>) {
    let cell = SENDER_GLOBAL.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = cell.lock() { *g = cb; }
}

pub fn get_sender() -> Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>> {
    let cell = SENDER_GLOBAL.get_or_init(|| Mutex::new(None));
    match cell.lock() { Ok(g) => g.clone(), Err(_) => None }
}

pub fn set_bingle_api(api: Option<Arc<dyn BingleApi>>) {
    let cell = API_GLOBAL.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = cell.lock() { *g = api; }
}

pub fn get_bingle_api() -> Option<Arc<dyn BingleApi>> {
    let cell = API_GLOBAL.get_or_init(|| Mutex::new(None));
    match cell.lock() { Ok(g) => g.clone(), Err(_) => None }
}

pub fn set_bingle_api_internal(api: Option<Arc<dyn BingleApiInternal>>) {
    let cell = API_INTERNAL_GLOBAL.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = cell.lock() { *g = api; }
}

pub fn get_bingle_api_internal() -> Option<Arc<dyn BingleApiInternal>> {
    let cell = API_INTERNAL_GLOBAL.get_or_init(|| Mutex::new(None));
    match cell.lock() { Ok(g) => g.clone(), Err(_) => None }
}

pub fn set_last_from(addr: Option<SocketAddr>) {
    let cell = LAST_FROM_GLOBAL.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = cell.lock() { *g = addr; }
}

pub fn get_last_from() -> Option<SocketAddr> {
    let cell = LAST_FROM_GLOBAL.get_or_init(|| Mutex::new(None));
    match cell.lock() { Ok(g) => *g, Err(_) => None }
}

pub fn set_last_response_tag(tag: Option<String>) {
    let cell = LAST_RESPONSE_TAG.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = cell.lock() { *g = tag; }
}

pub fn get_last_response_tag() -> Option<String> {
    let cell = LAST_RESPONSE_TAG.get_or_init(|| Mutex::new(None));
    match cell.lock() { Ok(g) => g.clone(), Err(_) => None }
}

pub fn route<H: MessageHandler + ?Sized>(handler: &H, msg: &Message, from_id: &str) {
    // Debug: print options via API if available
    if let Some(api) = get_bingle_api() { api.debug_print_options(); }
    let api_opt = get_bingle_api();
    if api_opt.is_none() { eprintln!("[router::route] No BingleApi available to pass to handler"); return; }
    let api = api_opt.unwrap();
    match msg {
        Message::PlainText(pt) => handler.on_plain_text(api.clone(), from_id, pt),
        Message::Relay(r) => match r {
            RelayMessage::Call(m) => handler.on_relay_call(api.clone(), from_id, m),
            RelayMessage::RelayResponse(m) => handler.on_relay_response(api.clone(), from_id, m),
            RelayMessage::TriangleTest1(m) => handler.on_triangle_test1(api.clone(), from_id, m),
            RelayMessage::TriangleTest2(m) => handler.on_triangle_test2(api.clone(), from_id, m),
            RelayMessage::TriangleTest3(m) => handler.on_triangle_test3(api.clone(), from_id, m),
            RelayMessage::Listen(m) => handler.on_relay_listen(api.clone(), from_id, m),
            RelayMessage::Check(m) => handler.on_relay_check(api.clone(), from_id, m),
            RelayMessage::ListenResponse(m) => handler.on_relay_listen_response(api.clone(), from_id, m),
            RelayMessage::CheckResponse(m) => handler.on_relay_check_response(api.clone(), from_id, m),
            RelayMessage::CallResponse(m) => handler.on_relay_call_response(api.clone(), from_id, m),
            RelayMessage::KeepAlive(m) => handler.on_relay_keep_alive(api.clone(), from_id, m),
        },
        Message::Unknown(v) => handler.on_unknown(api.clone(), v),
    }
}
