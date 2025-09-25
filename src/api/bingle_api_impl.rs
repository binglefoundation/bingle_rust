use std::net::SocketAddr;
use std::sync::Arc;

use serde_json::Value as JsonValue;

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
}

impl Default for BingleApiImpl {
    fn default() -> Self {
        Self { dtls: None, on_message: None, on_connect: None, started_options: None }
    }
}

impl BingleApiImpl {
    pub fn new() -> Self { Self::default() }

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

    fn send_over_dtls(&self, addr: SocketAddr, issuer: &str, message: JsonValue) -> bool {
        let bytes = match serde_json::to_vec(&message) {
            Ok(b) => b,
            Err(_) => return false,
        };
        if let Some(dtls) = &self.dtls {
            dtls.send(addr, issuer, &bytes).is_ok()
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
            let ok = self.send_over_dtls(addr, _user_id.as_str(), message);
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
        _network_source_key: &NetworkSourceKey,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, String> {
        Err("not implemented".to_string())
    }

    fn set_on_message(&mut self, handler: Option<Arc<OnMessageHandler>>) { self.on_message = handler; }

    fn set_on_connect(&mut self, handler: Option<Arc<OnConnectHandler>>) { self.on_connect = handler; }
}
