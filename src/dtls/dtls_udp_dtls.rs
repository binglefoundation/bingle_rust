use std::net::SocketAddr;

use crate::dtls::dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result};

#[allow(unused_imports)]
use udp_dtls as _udp_dtls_crate;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DtlsRole {
    Client,
    Server,
}

impl Default for DtlsRole {
    fn default() -> Self { DtlsRole::Client }
}

/// Minimal UDP-DTLS-backed implementation scaffold. Handshake is intended to be performed lazily on first send.
#[derive(Default)]
pub struct DtlsUdpDtls {
    // Handlers
    handle_message: Option<HandleMessage>,
    handle_peer_certificate: Option<HandlePeerCertificate>,

    // Credentials
    ca_cert: Option<Vec<u8>>,            // CA certificate (PEM)
    client_cert: Option<Vec<u8>>,        // Client certificate (PEM)
    client_private_key: Option<Vec<u8>>, // Client private key (PEM)
    server_signing_cert: Option<Vec<u8>>, // Server signing certificate (PEM)
    server_signing_private_key: Option<Vec<u8>>, // Server signing private key (PEM)

    // Runtime and role/state
    role: DtlsRole,
    runtime: Option<tokio::runtime::Runtime>,
    server_bind: Option<SocketAddr>,
}

impl DtlsUdpDtls {
    pub fn new() -> Self {
        // Build an internal multi-threaded runtime to own background DTLS tasks.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .ok();
        Self {
            handle_message: None,
            handle_peer_certificate: None,
            ca_cert: None,
            client_cert: None,
            client_private_key: None,
            server_signing_cert: None,
            server_signing_private_key: None,
            role: DtlsRole::default(),
            runtime,
            server_bind: None,
        }
    }

    /// Switch this instance to act as a DTLS client.
    pub fn as_client(mut self) -> Self {
        self.role = DtlsRole::Client;
        self
    }

    /// Switch this instance to act as a DTLS server.
    pub fn as_server(mut self) -> Self {
        self.role = DtlsRole::Server;
        self
    }

    /// Start a DTLS server bound to the provided address. The real udp-dtls accept loop will be wired here.
    pub fn start_server(&mut self, addr: SocketAddr) -> Result<()> {
        self.server_bind = Some(addr);
        if self.runtime.is_none() {
            // Try to create a runtime if it was not created successfully in new().
            self.runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().ok();
        }
        if let Some(rt) = &self.runtime {
            // Clone necessary state into the task to satisfy 'static requirement.
            let handle_message = self.handle_message;
            let handle_peer_certificate = self.handle_peer_certificate;
            let ca_cert = self.ca_cert.clone();
            let server_signing_cert = self.server_signing_cert.clone();
            let server_signing_private_key = self.server_signing_private_key.clone();
            rt.spawn(Self::start_server_task(
                addr,
                handle_message,
                handle_peer_certificate,
                ca_cert,
                server_signing_cert,
                server_signing_private_key,
            ));
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn set_handle_peer_certificate(&mut self, handler: Option<HandlePeerCertificate>) {
        self.handle_peer_certificate = handler;
    }

    pub fn with_handle_peer_certificate(mut self, handler: HandlePeerCertificate) -> Self {
        self.handle_peer_certificate = Some(handler);
        self
    }
}

impl DtlsUdpDtls {
    async fn client_send_async(&self, _to: SocketAddr, _data: &[u8]) -> Result<()> {
        // iOS plan (udp-dtls):
        // - Create a udp_dtls::Client with provided client cert/key and CA.
        // - Perform DTLS handshake to `_to` using a UDP socket.
        // - Send `_data` over the established DTLS session.
        // Non‑iOS: this implementation is unused; OpenSSL path is provided separately.
        if cfg!(target_os = "ios") {
            // Placeholder: real udp-dtls handshake/send to be implemented next.
            // Ensure creds exist (already validated by caller) and return Ok for now.
            Ok(())
        } else {
            // Not used on non‑iOS targets.
            Ok(())
        }
    }

    async fn start_server_task(
        _bind: SocketAddr,
        _handle_message: Option<HandleMessage>,
        _handle_peer_certificate: Option<HandlePeerCertificate>,
        _ca_cert: Option<Vec<u8>>,
        _server_signing_cert: Option<Vec<u8>>,
        _server_signing_private_key: Option<Vec<u8>>,
    ) -> Result<()> {
        // iOS plan (udp-dtls):
        // - Bind a UDP socket to `_bind`.
        // - Create a udp_dtls::Server/Acceptor configured with `_server_signing_cert`/`_server_signing_private_key` and `_ca_cert`.
        // - Accept DTLS clients, running mutual auth, call `_handle_peer_certificate` as part of validation if provided.
        // - Deliver decrypted datagrams to `_handle_message`.
        // Non‑iOS: this udp-dtls path is unused.
        let _ = (
            _handle_message,
            _handle_peer_certificate,
            _ca_cert,
            _server_signing_cert,
            _server_signing_private_key,
        );
        if cfg!(target_os = "ios") {
            // Placeholder: real udp-dtls accept/recv loop to be implemented next.
            Ok(())
        } else {
            Ok(())
        }
    }
}

impl Dtls for DtlsUdpDtls {
    fn start(&mut self, addr: SocketAddr) -> Result<()> { self.start_server(addr) }
    fn stop(&mut self) -> Result<()> { Ok(()) }
    fn send(&self, to: SocketAddr, data: &[u8]) -> Result<()> {
        // Validate role
        if self.role != DtlsRole::Client {
            return Err(());
        }
        // Fail if certificates required for client auth are missing.
        if self.client_cert.is_none() || self.client_private_key.is_none() || self.ca_cert.is_none() {
            return Err(());
        }
        // Ensure runtime exists to drive async tasks
        if let Some(rt) = &self.runtime {
            // Execute the async send path on the internal runtime
            rt.block_on(self.client_send_async(to, data))
        } else {
            Err(())
        }
    }

    fn get_handle_message(&self) -> Option<HandleMessage> {
        self.handle_message
    }

    fn set_handle_message(&mut self, handler: Option<HandleMessage>) {
        self.handle_message = handler;
    }

    fn with_handle_message(mut self, handler: HandleMessage) -> Self {
        self.handle_message = Some(handler);
        self
    }

    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> {
        self.handle_peer_certificate
    }

    fn set_handle_peer_certificate(&mut self, handler: Option<HandlePeerCertificate>) {
        self.handle_peer_certificate = handler;
    }

    fn with_handle_peer_certificate(mut self, handler: HandlePeerCertificate) -> Self {
        self.handle_peer_certificate = Some(handler);
        self
    }

    fn get_ca_cert(&self) -> Option<&[u8]> {
        self.ca_cert.as_deref()
    }

    fn set_ca_cert(&mut self, pem: Option<Vec<u8>>) {
        self.ca_cert = pem;
    }

    fn with_ca_cert(mut self, pem: Vec<u8>) -> Self {
        self.ca_cert = Some(pem);
        self
    }

    fn get_client_cert(&self) -> Option<&[u8]> {
        self.client_cert.as_deref()
    }

    fn set_client_cert(&mut self, pem: Option<Vec<u8>>) {
        self.client_cert = pem;
    }

    fn with_client_cert(mut self, pem: Vec<u8>) -> Self {
        self.client_cert = Some(pem);
        self
    }

    fn get_client_private_key(&self) -> Option<&[u8]> {
        self.client_private_key.as_deref()
    }

    fn set_client_private_key(&mut self, pem: Option<Vec<u8>>) {
        self.client_private_key = pem;
    }

    fn with_client_private_key(mut self, pem: Vec<u8>) -> Self {
        self.client_private_key = Some(pem);
        self
    }

    fn get_server_signing_cert(&self) -> Option<&[u8]> {
        self.server_signing_cert.as_deref()
    }

    fn set_server_signing_cert(&mut self, pem: Option<Vec<u8>>) {
        self.server_signing_cert = pem;
    }

    fn with_server_signing_cert(mut self, pem: Vec<u8>) -> Self {
        self.server_signing_cert = Some(pem);
        self
    }

    fn get_server_signing_private_key(&self) -> Option<&[u8]> {
        self.server_signing_private_key.as_deref()
    }

    fn set_server_signing_private_key(&mut self, pem: Option<Vec<u8>>) {
        self.server_signing_private_key = pem;
    }

    fn with_server_signing_private_key(mut self, pem: Vec<u8>) -> Self {
        self.server_signing_private_key = Some(pem);
        self
    }
}
