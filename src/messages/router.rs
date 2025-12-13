use crate::messages::handlers::{MessageHandler, FromStruct};
use crate::messages::types::*;

use std::cell::RefCell;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use crate::api::bingle_api::{BingleApi, NetworkSourceKey, UserId};
use crate::api::bingle_api::BingleApiInternal;

// Thread-local current router used to avoid globals and isolate per-API state.
thread_local! {
    static CURRENT_ROUTER: RefCell<Option<Arc<Router>>> = RefCell::new(None);
}

// No global fallbacks. Each API instance owns its own Router.
// A thread-local CURRENT_ROUTER is used to select the active Router context during routing and callbacks.

#[derive(Default)]
pub struct Router {
    sender: Mutex<Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>>>,
    api: Mutex<Option<Arc<dyn BingleApi>>>,
    api_internal: Mutex<Option<Arc<dyn BingleApiInternal>>>,
    last_from: Mutex<Option<SocketAddr>>,
    last_response_tag: Mutex<Option<String>>,
    on_message: Mutex<Option<Arc<crate::api::bingle_api::OnMessageHandler>>>,
}

impl Router {
    pub fn new(api: Arc<dyn BingleApi>) -> Self {
        Self {
            sender: Mutex::new(None),
            api: Mutex::new(Some(api)),
            api_internal: Mutex::new(None),
            last_from: Mutex::new(None),
            last_response_tag: Mutex::new(None),
            on_message: Mutex::new(None),
        }
    }

    pub fn with_current_router<R>(router: Arc<Router>, f: impl FnOnce() -> R) -> R {
        // Temporarily set the thread-local router for duration of f
        let prev = CURRENT_ROUTER.with(|cell| cell.replace(Some(router)));
        let out = f();
        // Restore previous value
        CURRENT_ROUTER.with(|cell| {
            let _ = cell.replace(prev);
        });
        out
    }

    pub fn set_sender(&self, cb: Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>>) {
        if let Ok(mut g) = self.sender.lock() { *g = cb; }
    }
    pub fn get_sender(&self) -> Option<Arc<dyn Fn(&NetworkSourceKey, &UserId, serde_json::Value) -> bool + Send + Sync + 'static>> {
        match self.sender.lock() { Ok(g) => g.clone(), Err(_) => None }
    }

    pub fn set_bingle_api(&self, api: Option<Arc<dyn BingleApi>>) { if let Ok(mut g) = self.api.lock() { *g = api; } }
    pub fn get_bingle_api(&self) -> Option<Arc<dyn BingleApi>> { match self.api.lock() { Ok(g) => g.clone(), Err(_) => None } }

    pub fn set_bingle_api_internal(&self, api: Option<Arc<dyn BingleApiInternal>>) { if let Ok(mut g) = self.api_internal.lock() { *g = api; } }
    pub fn get_bingle_api_internal(&self) -> Option<Arc<dyn BingleApiInternal>> { match self.api_internal.lock() { Ok(g) => g.clone(), Err(_) => None } }

    pub fn set_last_from(&self, addr: Option<SocketAddr>) { if let Ok(mut g) = self.last_from.lock() { *g = addr; } }
    pub fn get_last_from(&self) -> Option<SocketAddr> { match self.last_from.lock() { Ok(g) => *g, Err(_) => None } }

    pub fn set_last_response_tag(&self, tag: Option<String>) { if let Ok(mut g) = self.last_response_tag.lock() { *g = tag; } }
    pub fn get_last_response_tag(&self) -> Option<String> { match self.last_response_tag.lock() { Ok(g) => g.clone(), Err(_) => None } }

    pub fn set_on_message(&self, cb: Option<Arc<crate::api::bingle_api::OnMessageHandler>>) { if let Ok(mut g) = self.on_message.lock() { *g = cb; } }
    pub fn get_on_message(&self) -> Option<Arc<crate::api::bingle_api::OnMessageHandler>> { match self.on_message.lock() { Ok(g) => g.clone(), Err(_) => None } }

    pub fn route<H: MessageHandler + ?Sized>(&self, handler: &H, msg: &Message, from_id: &str) {
        let api_opt = self.get_bingle_api();
        if api_opt.is_none() {
            log::warn!("[router::route] No BingleApi available to pass to handler");
            return;
        }
        let api = api_opt.unwrap();
        // Build FromStruct with id and network source key (direct from last_from if available)
        let nsk = if let Some(addr) = self.get_last_from() {
            NetworkSourceKey::new_direct(addr)
        } else {
            NetworkSourceKey::new_direct("0.0.0.0:0".parse().unwrap())
        };
        let from = FromStruct { id: from_id.to_string(), network_source_key: nsk };
        match msg {
            Message::PlainText(pt) => handler.on_plain_text(api.clone(), &from, pt),
            Message::Relay(r) => match r {
                RelayMessage::Call(m) => handler.on_relay_call(api.clone(), &from, m),
                RelayMessage::RelayResponse(m) => handler.on_relay_response(api.clone(), &from, m),
                RelayMessage::TriangleTest1(m) => handler.on_triangle_test1(api.clone(), &from, m),
                RelayMessage::TriangleTest2(m) => handler.on_triangle_test2(api.clone(), &from, m),
                RelayMessage::TriangleTest3(m) => handler.on_triangle_test3(api.clone(), &from, m),
                RelayMessage::TriangleTest1Response(m) => handler.on_triangle_test1_response(api.clone(), &from, m),
                RelayMessage::Listen(m) => handler.on_relay_listen(api.clone(), &from, m),
                RelayMessage::Check(m) => handler.on_relay_check(api.clone(), &from, m),
                RelayMessage::ListenResponse(m) => handler.on_relay_listen_response(api.clone(), &from, m),
                RelayMessage::CheckResponse(m) => handler.on_relay_check_response(api.clone(), &from, m),
                RelayMessage::CallResponse(m) => handler.on_relay_call_response(api.clone(), &from, m),
                RelayMessage::KeepAlive(m) => handler.on_relay_keep_alive(api.clone(), &from, m),
            },
            Message::Unknown(v) => handler.on_unknown(api.clone(), v),
            Message::Ddb(_) => handler.on_unimplemented(msg),
        }
    }

    /// Test helper to clear all stored state (used by Engine::clear_api_bindings)
    pub fn clear_for_tests(&self) {
        self.set_sender(None);
        self.set_bingle_api(None);
        self.set_bingle_api_internal(None);
        self.set_last_from(None);
        self.set_last_response_tag(None);
        self.set_on_message(None);
    }
}

// Access the current thread-local Router if set.
impl Router {
    pub fn current() -> Option<Arc<Router>> {
        CURRENT_ROUTER.with(|cell| cell.borrow().clone())
    }
}
