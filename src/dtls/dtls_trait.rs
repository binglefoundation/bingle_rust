pub mod dtls;

use std::net::SocketAddr;

/**
 * Handle incoming messages
 * @param from_address the address of the peer
 * @param data the data received
 */
pub type HandleMessage = fn(from_address: &SocketAddr, data: &[u8]);

/**
 * Handle certificates presented by the peer for verification
 * @param certificate the certificate presented by the peer in PEM format
 * @param ca_certificate the CA certificate used to verify the peer certificate in PEM format
 * @return the issuer of the peer certificate
 */
pub type HandlePeerCertificate = fn(certificate: &[u8], ca_certificate: &[u8]) -> Result<String>;

trait Dtls {
    /**
     * Send data to the peer
     * If no connection exists to `to`, a new connection will be established.
     * @param to the address of the peer
     * @param data the data to send
     * @return true if the data was sent, false otherwise
     */
    fn send(&self, to: SocketAddr, data: &[u8]) -> Result<()>;

    /**
     * Get a message handler function
     * @return the message handler function
     */
    fn get_handle_message(&self) -> Option<HandleMessage>;

    /**
     * Set a message handler function
     * @param handler the message handler function
     */
    fn set_handle_message(&mut self, handler: Option<HandleMessage>);

    /**
     * Fluently set a message handler function
     * @param handler the message handler function
     * @return a new instance of the DTLS trait with the message handler function set
     */
    fn with_handle_message(self, handler: HandleMessage) -> Self
    where
        Self: Sized;

    // PEM bytes accessors: CA certificate
    fn get_ca_cert(&self) -> Option<&[u8]>;
    fn set_ca_cert(&mut self, pem: Option<Vec<u8>>);
    fn with_ca_cert(self, pem: Vec<u8>) -> Self
    where
        Self: Sized;

    // PEM bytes accessors: client certificate
    fn get_client_cert(&self) -> Option<&[u8]>;
    fn set_client_cert(&mut self, pem: Option<Vec<u8>>);
    fn with_client_cert(self, pem: Vec<u8>) -> Self
    where
        Self: Sized;

    // PEM bytes accessors: client private key
    fn get_client_private_key(&self) -> Option<&[u8]>;
    fn set_client_private_key(&mut self, pem: Option<Vec<u8>>);
    fn with_client_private_key(self, pem: Vec<u8>) -> Self
    where
        Self: Sized;

    // PEM bytes accessors: server signing certificate
    fn get_server_signing_cert(&self) -> Option<&[u8]>;
    fn set_server_signing_cert(&mut self, pem: Option<Vec<u8>>);
    fn with_server_signing_cert(self, pem: Vec<u8>) -> Self
    where
        Self: Sized;

    // PEM bytes accessors: server signing private key
    fn get_server_signing_private_key(&self) -> Option<&[u8]>;
    fn set_server_signing_private_key(&mut self, pem: Option<Vec<u8>>);
    fn with_server_signing_private_key(self, pem: Vec<u8>) -> Self
    where
        Self: Sized;
}