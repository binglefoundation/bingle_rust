use std::net::SocketAddr;
use std::sync::Arc;

use crate::api::bingle_api::StartOptions;
use crate::dtls::{Dtls, DtlsOpenSsl, NetworkMux, UdpNetworkMux};
use crate::messages::{from_json_str, route, DefaultPrintingHandler};
use crate::messages::handlers::MessageHandler;

/// Minimal Engine implementation that wires UDP mux + DTLS and routes inbound JSON messages.
pub struct Engine {
    options: Option<StartOptions>,
    mux: Option<Arc<UdpNetworkMux>>, // concrete to access start/stop helpers
    dtls: Option<DtlsOpenSsl>,
}

impl Engine {
    pub fn new() -> Self {
        Self { options: None, mux: None, dtls: None }
    }

    /// Start the engine using the provided StartOptions.
    /// For now, only the static endpoint path is implemented.
    pub fn start(&mut self, options: StartOptions) -> Result<(), String> {
        // Keep a copy of options
        self.options = Some(options.clone());

        let Some(static_addr) = options.static_ip else {
            return Err("NotImplemented: Engine without staticEndpoint is not yet implemented".into());
        };

        // Create a UDP NetworkMux bound to the requested address (port may be 0 for OS-assigned)
        let mux = Arc::new(UdpNetworkMux::bind(static_addr).map_err(|_| "Failed to bind UDP mux")?);
        // Determine the concrete local address after bind (handles port 0)
        let local_addr: SocketAddr = mux.local_addr().map_err(|_| "Failed to get local addr")?;

        // Create a DTLS instance and install a message handler that decodes JSON and routes it.
        let mut dtls = DtlsOpenSsl::new();
        dtls.set_handle_message(Some(Arc::new(|server, from, data| Self::handle_dtls_message(server, from, data))));

        // Start DTLS accept loop with the mux and the concrete local address
        dtls.start(local_addr, Some(mux.clone() as Arc<dyn NetworkMux + Send + Sync>))
            .map_err(|_| "Failed to start DTLS")?;

        // Start the UDP mux background loop
        mux.start().map_err(|_| "Failed to start UDP mux")?;

        self.mux = Some(mux);
        self.dtls = Some(dtls);
        Ok(())
    }

    /// Stop the engine and background tasks if started.
    pub fn stop(&mut self) {
        if let Some(dtls) = &mut self.dtls {
            let _ = dtls.stop();
        }
        if let Some(mux) = &self.mux {
            mux.stop();
        }
        self.dtls = None;
        self.mux = None;
    }

    /// DTLS message handler: try to interpret payload as UTF-8 JSON and route.
    fn handle_dtls_message(_server: &dyn Dtls, _from: &SocketAddr, data: &[u8]) {
        // Best-effort decode; print unimplemented on failure via default handler
        let handler = DefaultPrintingHandler;
        match std::str::from_utf8(data) {
            Ok(s) => {
                match from_json_str(s) {
                    Ok(msg) => route(&handler, &msg),
                    Err(_) => {
                        // Not valid JSON per our schema; treat as plaintext with raw bytes
                        // For now, just print
                        handler.on_unimplemented(&crate::messages::Message::Unknown(serde_json::Value::String(s.to_string())));
                    }
                }
            }
            Err(_) => {
                // Not UTF-8; ignore or log
                handler.on_unimplemented(&crate::messages::Message::Unknown(serde_json::Value::Null));
            }
        }
    }
}
