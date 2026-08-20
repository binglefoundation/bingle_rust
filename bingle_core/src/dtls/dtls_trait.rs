use super::network_mux_udp::UdpNetworkMux;
use crate::api::bingle_api::NetworkEndpoint;
use std::sync::Arc;

/// Local Result type for DTLS operations with string error messages.
pub type Result<T = ()> = core::result::Result<T, String>;

/**
 * Handle incoming messages
 * @param from_address the address of the peer
 * @param data the data received
 * Note: Use an `Arc<dyn Fn>` so implementations can capture per-instance context without globals.
 */
pub type HandleMessage = Arc<dyn Fn(&dyn Dtls, &NetworkEndpoint, &str, &[u8]) + Send + Sync>;

/// Notifies listeners that a peer endpoint has rolled over to a new DTLS session.
pub type HandleNewSession = Arc<dyn Fn(&NetworkEndpoint) + Send + Sync>;

/**
 * Handle certificates presented by the peer for verification
 * @param certificate the certificate presented by the peer in PEM format
 * @param ca_certificate the CA certificate used to verify the peer certificate in PEM format
 * @return the issuer of the peer certificate
 */
pub type HandlePeerCertificate = fn(certificate: &[u8], ca_certificate: &[u8]) -> Result<String>;

/// Public DTLS trait abstraction
pub trait Dtls {
    /**
     * Start the DTLS accept loop using the provided, already-bound UDP mux.
     * Implementations must not bind or start their own mux; a valid mux is required.
     */
    fn start(&self, mux: Arc<UdpNetworkMux>) -> Result<()>;

    /**
     * Stop the DTLS accept loop, waiting for background tasks to exit.
     */
    fn stop(&self) -> Result<()>;

    /**
     * Send data to the peer
     * If no connection exists to `to`, a new connection will be established.
     * @param to the address of the peer
     * @param data the data to send
     * @return Ok(()) if the data was queued/sent, Err(()) otherwise
     */
    fn send(&self, to: &NetworkEndpoint, data: &[u8]) -> Result<()>;

    /**
     * Get a message handler function
     * @return the message handler function
     */
    fn get_handle_message(&self) -> Option<HandleMessage>;

    /**
     * Set a message handler function
     * @param handler the message handler function
     */
    fn set_handle_message(&self, handler: Option<HandleMessage>);

    /**
     * Fluently set a message handler function
     * @param handler the message handler function
     * @return a new instance of the DTLS trait with the message handler function set
     */
    fn with_handle_message(self, handler: HandleMessage) -> Self
    where
        Self: Sized;

    /**
     * Set a callback invoked when DTLS detects a new session for a peer endpoint.
     * @param handler callback invoked with the endpoint whose session rolled over
     */
    fn set_handle_new_session(&self, handler: Option<HandleNewSession>);

    /**
     * Set a callback invoked when this side establishes a *fresh outbound* DTLS
     * session to a peer — i.e. a handshake performed because no live session
     * existed (first contact, or a rebuild after the previous session was
     * forgotten/died). Used to re-run relay listener registration immediately on
     * reconnect instead of waiting for the next periodic tick (issue #50).
     *
     * Unlike `set_handle_new_session`, this carries no reliable-transport side
     * effects (no session-generation bump, no pending-ACK reset): it is a pure
     * notification, so firing it must not disturb the in-flight send that is
     * driving the rebuild.
     *
     * Default: a no-op. This is an optional observer — a `Dtls` impl that never
     * fires it is still correct (the keep-alive path still recovers) — so unlike
     * the mandatory `forget_peer` the default does not panic. Production
     * `DtlsOpenSsl` overrides it; test mocks inherit the no-op.
     */
    fn set_handle_new_outbound_session(&self, _handler: Option<HandleNewSession>) {}

    /**
     * Get the peer certificate handler function
     */
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate>;

    /**
     * Set the peer certificate handler function
     */
    fn set_handle_peer_certificate(&self, handler: Option<HandlePeerCertificate>);

    /**
     * Fluently set the peer certificate handler function
     */
    fn with_handle_peer_certificate(self, handler: HandlePeerCertificate) -> Self
    where
        Self: Sized;

    // PEM bytes accessors: CA certificate
    fn get_ca_cert(&self) -> Option<&[u8]>;
    fn set_ca_cert(&self, pem: Option<Vec<u8>>);
    fn with_ca_cert(self, pem: Vec<u8>) -> Self
    where
        Self: Sized;

    // PEM bytes accessors: client certificate
    fn get_client_cert(&self) -> Option<&[u8]>;
    fn set_client_cert(&self, pem: Option<Vec<u8>>);
    fn with_client_cert(self, pem: Vec<u8>) -> Self
    where
        Self: Sized;

    // PEM bytes accessors: client private key
    fn get_client_private_key(&self) -> Option<&[u8]>;
    fn set_client_private_key(&self, pem: Option<Vec<u8>>);
    fn with_client_private_key(self, pem: Vec<u8>) -> Self
    where
        Self: Sized;

    // PEM bytes accessors: server signing certificate
    fn get_server_signing_cert(&self) -> Option<&[u8]>;
    fn set_server_signing_cert(&self, pem: Option<Vec<u8>>);
    fn with_server_signing_cert(self, pem: Vec<u8>) -> Self
    where
        Self: Sized;

    // PEM bytes accessors: server signing private key
    fn get_server_signing_private_key(&self) -> Option<&[u8]>;
    fn set_server_signing_private_key(&self, pem: Option<Vec<u8>>);
    fn with_server_signing_private_key(self, pem: Vec<u8>) -> Self
    where
        Self: Sized;

    // Verification mode: if true, do not enforce TLS verification during handshake; validate at application layer only.
    fn set_app_layer_only_verification(&self, enabled: bool);
    fn with_app_layer_only_verification(self, enabled: bool) -> Self
    where
        Self: Sized;

    /**
     * Set the dangerous debug mode
     * This enables features that are insecure but useful for debugging, such as NULL encryption and keylogging.
     */
    fn set_dangerous_debug(&self, enabled: bool);

    /**
     * Fluently set the dangerous debug mode
     */
    fn with_dangerous_debug(self, enabled: bool) -> Self
    where
        Self: Sized;

    /**
     * Set the null encryption mode
     * This enables NULL cipher suites for DTLS handshakes (no encryption).
     * Only effective if dangerous_debug is also enabled.
     */
    fn set_null_encryption(&self, enabled: bool);

    /**
     * Fluently set the null encryption mode
     */
    fn with_null_encryption(self, enabled: bool) -> Self
    where
        Self: Sized;

    /**
     * Get the cipher suite negotiated for the DTLS session with the given endpoint.
     * Returns None if the handshake has not completed yet or the endpoint is unknown.
     * This value is derived from the connection and is not transmitted on the wire.
     */
    fn get_cipher_suite(&self, endpoint: &NetworkEndpoint) -> Option<String>;

    /**
     * Forget all peer connections, closing their workers.
     * Used when the public address changes so that fresh DTLS handshakes are performed.
     */
    fn forget_peers(&self);

    /**
     * Forget a single peer connection, closing its worker, so the next send to
     * that endpoint performs a fresh DTLS handshake.
     *
     * Used by the reliable transport when a send exhausts its ACK retries: after
     * a network change, the session to a peer (e.g. a non-home root relay) can be
     * silently dead — local UDP writes still "succeed" but nothing comes back, so
     * the worker stays healthy and every send reuses the dead session forever.
     * Tearing the peer down on that signal lets the next retry re-handshake and
     * recover. See issue #46.
     *
     * A panicking default is provided so test-only `Dtls` mocks that never exercise
     * this path need not implement it; production impls MUST override it.
     */
    fn forget_peer(&self, _endpoint: &NetworkEndpoint) {
        panic!("Dtls::forget_peer must be implemented by production Dtls impls");
    }
}
