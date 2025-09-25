use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, Condvar};
use std::time::{Duration, Instant};

use serde_json::{Value as JsonValue, Map as JsonMap};
use uuid::Uuid;

use crate::api::bingle_api::{BingleApi, Handle, NetworkSourceKey, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use crate::dtls::Dtls;

/// Concrete implementation of the BingleApi trait.
///
/// Minimal functionality implemented per task requirements:
/// - start: instantiate a DTLS implementation (DtlsOpenSsl on non-iOS) but do not start the accept loop (no address yet).
/// - send_message_to_network: when given a direct socket address, call DTLS send with the JSON message bytes.
pub struct BingleApiImpl {
    dtls: Option<Box<dyn Dtls + Send + Sync>>, // boxed trait object for flexibility/mocking
    on_message: Option<Arc<OnMessageHandler>>,
    on_connect: Option<Arc<OnConnectHandler>>,
    started_options: Option<StartOptions>,
    // Map of pending response tags to their wait primitives
    pending_responses: Arc<Mutex<HashMap<Uuid, Arc<(Mutex<Pending>, Condvar)>>>>,
    // Shared on_message handler accessible from DTLS callback without needing &self
    shared_on_message: Arc<Mutex<Option<Arc<OnMessageHandler>>>>,
}

impl Default for BingleApiImpl {
    fn default() -> Self {
        Self {
            dtls: None,
            on_message: None,
            on_connect: None,
            started_options: None,
            pending_responses: Arc::new(Mutex::new(HashMap::new())),
            shared_on_message: Arc::new(Mutex::new(None)),
        }
    }
}

#[derive(Debug)]
struct Pending {
    responded: bool,
    response: Option<JsonValue>,
}

impl Default for Pending {
    fn default() -> Self {
        Self { responded: false, response: None }
    }
}

impl BingleApiImpl {
    pub fn new() -> Self { Self::default() }
}

impl BingleApiImpl {
    /// Test-oriented constructor to inject a custom DTLS implementation.
    pub fn new_with_dtls(dtls: Box<dyn Dtls + Send + Sync>) -> Self {
        Self { dtls: Some(dtls), ..Default::default() }
    }

    /// Exposed for integration tests: whether a DTLS instance has been created.
    pub fn has_dtls(&self) -> bool { self.dtls.is_some() }

    fn ensure_dtls(&mut self) {
        if self.dtls.is_none() {
            // Only available on non-iOS targets.
            #[cfg(not(target_os = "ios"))]
            {
                let dtls = crate::dtls::DtlsOpenSsl::new();
                self.dtls = Some(Box::new(dtls));
            }
            #[cfg(target_os = "ios")]
            {
                // Placeholder for iOS where OpenSSL-backed DTLS is not available in this crate.
                self.dtls = None;
            }
        }
    }

    fn send_over_dtls(&self, addr: SocketAddr, message: JsonValue) -> bool {
        let bytes = match serde_json::to_vec(&message) {
            Ok(b) => b,
            Err(_) => return false,
        };
        if let Some(dtls) = &self.dtls {
            dtls.send(addr, &bytes).is_ok()
        } else {
            false
        }
    }
}

impl BingleApi for BingleApiImpl {
    fn start(&mut self, options: StartOptions) -> Result<(), String> {
        // Persist options and create a DTLS instance, but do NOT start acceptor (no address yet).
        self.started_options = Some(options);
        self.ensure_dtls();
        // Install a per-instance DTLS message handler that captures shared state (no globals needed)
        if let Some(dtls) = self.dtls.as_mut() {
            let pending = Arc::clone(&self.pending_responses);
            let onmsg_shared = Arc::clone(&self.shared_on_message);
            dtls.set_handle_message(Some(Arc::new(move |_server, from_address, data| {
                // Try to parse incoming bytes as JSON
                let json_opt: Option<JsonValue> = match std::str::from_utf8(data) {
                    Ok(s) => serde_json::from_str::<JsonValue>(s).ok(),
                    Err(_) => None,
                };
                if let Some(msg) = json_opt {
                    // Extract optional sender fields from message if present
                    let sender = msg.get("sender").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                    let sender_handle = msg
                        .get("senderHandle")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| from_address.to_string());

                    // Route tagged response or dispatch to on_message
                    let tag_opt = msg.get("responseTag").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());
                    if let Some(tag) = tag_opt {
                        let pair_opt = {
                            let map = match pending.lock() { Ok(m) => m, Err(_) => { eprintln!("pending map lock poisoned"); return; } };
                            map.get(&tag).cloned()
                        };
                        match pair_opt {
                            None => { eprintln!("Received tagged response for unknown tag {}. Discarding.", tag); }
                            Some(pair) => {
                                let (lock, cvar) = (&pair.0, &pair.1);
                                let mut guard = match lock.lock() { Ok(g) => g, Err(_) => { eprintln!("pending lock poisoned"); return; } };
                                guard.responded = true;
                                guard.response = Some(msg);
                                cvar.notify_all();
                            }
                        }
                    } else {
                        if let Ok(g) = onmsg_shared.lock() {
                            if let Some(cb) = g.as_ref() { cb(sender, sender_handle, msg); }
                        }
                    }
                }
            })));
        }
        Ok(())
    }

    fn stop(&mut self) {
        // For now, simply drop the DTLS instance; more graceful shutdown can be added later.
        self.dtls = None;
    }

    fn network_change(&mut self) {
        // Placeholder: in a full implementation, we would rescan STUN/static IP and update listeners.
    }

    fn send_message_to_id(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool {
        // Not implemented yet
        false
    }

    fn send_message_to_handle(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool {
        // Not implemented yet
        false
    }

    fn send_message_to_network(
        &self,
        network_source_key: &NetworkSourceKey,
        _user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> bool {
        if let Some(cb) = progress.as_ref() { cb(10, "Preparing send".to_string()); }
        // Only direct socket address path is implemented at this stage.
        if let Some(addr) = network_source_key.inet_socket_address {
            let ok = self.send_over_dtls(addr, message);
            if let Some(cb) = progress.as_ref() { cb(100, if ok { "Sent" } else { "Failed to send" }.to_string()); }
            ok
        } else {
            if let Some(cb) = progress.as_ref() { cb(100, "Relay send not yet implemented".to_string()); }
            false
        }
    }

    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        Err("not implemented".to_string())
    }

    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        Err("not implemented".to_string())
    }

    fn send_message_to_network_with_response(
        &self,
        network_source_key: &NetworkSourceKey,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, String> {
        // Create a unique tag and register a pending waiter
        let tag = Uuid::new_v4();
        let pair: Arc<(Mutex<Pending>, Condvar)> = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        {
            let mut map = self.pending_responses.lock().map_err(|_| "lock poisoned")?;
            map.insert(tag, Arc::clone(&pair));
        }

        // Ensure message has the responseTag field
        let mut msg_with_tag = match message {
            JsonValue::Object(mut m) => {
                m.insert("responseTag".to_string(), JsonValue::String(tag.to_string()));
                JsonValue::Object(m)
            }
            other => {
                let mut m = JsonMap::new();
                m.insert("payload".to_string(), other);
                m.insert("responseTag".to_string(), JsonValue::String(tag.to_string()));
                JsonValue::Object(m)
            }
        };

        // Now spawn a scoped thread to send the message while the current thread waits
        if let Some(cb) = progress.as_ref() { cb(5, "Queueing send".to_string()); }
        let send_progress = progress.clone();
        std::thread::scope(|s| {
            // Sender thread
            s.spawn(|| {
                let ok = self.send_message_to_network(network_source_key, user_id, msg_with_tag.take(), send_progress.clone());
                if let Some(cb) = send_progress.as_ref() {
                    cb(20, if ok { "Sent request" } else { "Failed to send request" }.to_string());
                }
            });

            // Waiting in the current thread
            let timeout = Duration::from_secs(10);
            let start = Instant::now();
            let (lock, cvar) = (&pair.0, &pair.1);
            let mut guard = lock.lock().map_err(|_| "lock poisoned")?;
            while !guard.responded {
                let remaining = timeout.saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    break;
                }
                let (g, res) = cvar.wait_timeout(guard, remaining).map_err(|_| "condvar wait failed")?;
                guard = g;
                if res.timed_out() && !guard.responded { break; }
            }
            if guard.responded {
                let resp = guard.response.take();
                drop(guard);
                // Clean up the map
                let mut map = self.pending_responses.lock().map_err(|_| "lock poisoned")?;
                map.remove(&tag);
                if let Some(cb) = progress.as_ref() { cb(100, "Received response".to_string()); }
                resp.ok_or_else(|| "no response payload".to_string())
            } else {
                drop(guard);
                let mut map = self.pending_responses.lock().map_err(|_| "lock poisoned")?;
                map.remove(&tag);
                if let Some(cb) = progress.as_ref() { cb(100, "Timed out waiting for response".to_string()); }
                Err("timeout waiting for response".to_string())
            }
        })
    }

    fn set_on_message(&mut self, handler: Option<Arc<OnMessageHandler>>) { 
            self.on_message = handler.clone(); 
            if let Ok(mut g) = self.shared_on_message.lock() { *g = handler; }
        }

    fn set_on_connect(&mut self, handler: Option<Arc<OnConnectHandler>>) { self.on_connect = handler; }
}

impl BingleApiImpl {
    /// Public entry from the networking layer for inbound messages.
    /// If message contains a responseTag, it is treated as a response and routed to waiter; otherwise it is dispatched to on_message.
    pub fn handle_incoming_network_message(&self, sender: UserId, sender_handle: Handle, message: JsonValue) {
        // Check for responseTag
        let tag_opt = message.get("responseTag").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());
        if let Some(tag) = tag_opt {
            let pair_opt = {
                let map = match self.pending_responses.lock() { Ok(m) => m, Err(_) => { eprintln!("pending map lock poisoned"); return; } };
                map.get(&tag).cloned()
            };
            match pair_opt {
                None => {
                    eprintln!("Received tagged response for unknown tag {}. Discarding.", tag);
                }
                Some(pair) => {
                    let (lock, cvar) = (&pair.0, &pair.1);
                    let mut guard = match lock.lock() { Ok(g) => g, Err(_) => { eprintln!("pending lock poisoned"); return; } };
                    guard.responded = true;
                    guard.response = Some(message);
                    cvar.notify_all();
                }
            }
        } else {
            if let Some(cb) = &self.on_message {
                cb(sender, sender_handle, message);
            }
        }
    }
}
