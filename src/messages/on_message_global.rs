use std::sync::{Mutex, OnceLock, Arc};
use serde_json::Value as JsonValue;
use crate::api::bingle_api::OnMessageHandler;

// Global on_message dispatcher storage; used by MessageHandler::on_plain_text to delegate to API.
static GLOBAL_ON_MESSAGE: OnceLock<Mutex<Option<Arc<OnMessageHandler>>>> = OnceLock::new();

/// Set or clear the global on_message handler (used by plain-text router fallback).
pub fn global_on_message_set(handler: Option<Arc<OnMessageHandler>>) {
    let slot = GLOBAL_ON_MESSAGE.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = slot.lock() { *g = handler; }
}

/// Invoke the global on_message handler, if set.
pub fn global_on_message_call(sender: String, sender_handle: String, msg: JsonValue) {
    if let Some(slot) = GLOBAL_ON_MESSAGE.get() {
        if let Ok(g) = slot.lock() {
            if let Some(cb) = g.as_ref() { cb(sender, sender_handle, msg); }
        }
    }
}
