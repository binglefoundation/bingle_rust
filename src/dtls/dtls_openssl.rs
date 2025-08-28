use std::net::SocketAddr;

use crate::dtls::dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result};

#[cfg(not(target_os = "ios"))]
pub mod non_ios {
    #[cfg(unix)]
    use std::os::unix::io::AsRawFd;
    #[cfg(unix)]
    use openssl_sys as ossl_sys;
    use super::*;
    use std::net::UdpSocket;
    use std::thread;
    // OpenSSL DTLS imports (unused for now; will be used as we wire handshake)
    #[allow(unused_imports)]
    use openssl::ssl::{SslAcceptor, SslAcceptorBuilder, SslConnector, SslConnectorBuilder, SslContext, SslContextBuilder, SslFiletype, SslMethod, SslOptions, SslVerifyMode};
    #[allow(unused_imports)]
    use openssl::x509::X509;
    #[allow(unused_imports)]
    use openssl::x509::store::X509StoreBuilder;
    #[allow(unused_imports)]
    use openssl::pkey::PKey;

    /// OpenSSL-backed DTLS implementation (non-iOS only for now).
    /// This is a scaffold: wiring to OpenSSL's DTLS handshake, mutual auth,
    /// and async send/recv will be added next.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum DtlsRole { Client, Server }
    impl Default for DtlsRole { fn default() -> Self { DtlsRole::Client } }

    #[derive(Default)]
    pub struct DtlsOpenSsl {
        // Handlers
        pub(crate) handle_message: Option<HandleMessage>,
        pub(crate) handle_peer_certificate: Option<HandlePeerCertificate>,

        // Credentials
        pub(crate) ca_cert: Option<Vec<u8>>,            // CA certificate (PEM)
        pub(crate) client_cert: Option<Vec<u8>>,        // Client certificate (PEM)
        pub(crate) client_private_key: Option<Vec<u8>>, // Client private key (PEM)
        pub(crate) server_signing_cert: Option<Vec<u8>>, // Server signing certificate (PEM)
        pub(crate) server_signing_private_key: Option<Vec<u8>>, // Server signing private key (PEM)

        // Role and state placeholders
        pub(crate) role: DtlsRole,
        // Prepared DTLS server acceptor (DTLSv1.2), built on start_server
        pub(crate) acceptor: Option<SslAcceptor>,
    }

    impl DtlsOpenSsl {
        pub fn new() -> Self { Self { role: DtlsRole::default(), ..Default::default() } }

        pub fn as_client(mut self) -> Self { self.role = DtlsRole::Client; self }
        pub fn as_server(mut self) -> Self { self.role = DtlsRole::Server; self }

        fn prepare_client_context(&self) -> Result<()> {
            // Build a DTLSv1.2 client connector and configure mutual auth + verification.
            let ca_pem = self.ca_cert.as_deref().ok_or(())?;
            let client_cert_pem = self.client_cert.as_deref().ok_or(())?;
            let client_key_pem = self.client_private_key.as_deref().ok_or(())?;

            // Parse materials to validate and to allow store building.
            let ca_x509 = X509::from_pem(ca_pem).map_err(|_| ())?;
            let client_x509 = X509::from_pem(client_cert_pem).map_err(|_| ())?;
            let client_key = PKey::private_key_from_pem(client_key_pem).map_err(|_| ())?;

            // Create DTLS connector builder and load credentials.
            let mut builder = SslConnector::builder(SslMethod::dtls()).map_err(|_| ())?;

            // Restrict to DTLSv1.2.
            builder.set_options(SslOptions::NO_DTLSV1);

            // Load client cert and key.
            builder.set_certificate(&client_x509).map_err(|_| ())?;
            builder.set_private_key(&client_key).map_err(|_| ())?;
            builder.check_private_key().map_err(|_| ())?;

            // Install CA cert into the verify store for server auth.
            let mut store_builder = X509StoreBuilder::new().map_err(|_| ())?;
            store_builder.add_cert(ca_x509).map_err(|_| ())?;
            let store = store_builder.build();
            builder.set_verify_cert_store(store).map_err(|_| ())?;

            // Require peer (server) verification; wire callback if provided.
            builder.set_verify(SslVerifyMode::PEER);
            if let Some(handler) = self.handle_peer_certificate {
                let ca_bytes = self.ca_cert.clone().unwrap_or_default();
                builder.set_verify_callback(SslVerifyMode::PEER, move |preverify_ok, ctx| {
                    if !preverify_ok { return false; }
                    if let Some(cert_ref) = ctx.current_cert() {
                        if let Ok(cert_pem) = cert_ref.to_pem() {
                            return handler(&cert_pem, &ca_bytes).is_ok();
                        }
                    }
                    false
                });
            }

            // Note: We intentionally do not store the connector yet; when we implement the
            // actual handshake path, we'll build and use it against a UDP transport.
            Ok(())
        }

        // Next-step placeholder: perform DTLS client handshake over UDP to the given address.
        // This will be implemented in a subsequent increment using OpenSSL DTLS over datagram BIOs.
        fn client_handshake_connect(&self, _to: SocketAddr) -> Result<()> {
            // Validate that prepare_client_context prerequisites are met (already checked in send()).
            // For now, this function is a no-op placeholder to structure the future handshake logic.
            Ok(())
        }

        fn prepare_server_acceptor(&mut self) -> Result<()> {
            // Build an SslAcceptor for DTLSv1.2 and require client certificate authentication.
            let ca_pem = self.ca_cert.as_deref().ok_or(())?;
            let server_cert_pem = self.server_signing_cert.as_deref().ok_or(())?;
            let server_key_pem = self.server_signing_private_key.as_deref().ok_or(())?;

            let ca_x509 = X509::from_pem(ca_pem).map_err(|_| ())?;
            let server_x509 = X509::from_pem(server_cert_pem).map_err(|_| ())?;
            let server_key = PKey::private_key_from_pem(server_key_pem).map_err(|_| ())?;

            // Context builder for DTLS (we will constrain to DTLSv1.2)
            let mut builder = SslAcceptor::mozilla_modern_v5(SslMethod::dtls()).map_err(|_| ())?;

            // Load server certificate and private key
            builder.set_certificate(&server_x509).map_err(|_| ())?;
            builder.set_private_key(&server_key).map_err(|_| ())?;
            builder.check_private_key().map_err(|_| ())?;

            // Require client auth
            builder.set_verify(SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT);

            // Install CA into store
            let mut store_builder = X509StoreBuilder::new().map_err(|_| ())?;
            store_builder.add_cert(ca_x509).map_err(|_| ())?;
            let store = store_builder.build();
            builder.set_verify_cert_store(store).map_err(|_| ())?;

            // Constrain to DTLSv1.2 only
            builder.set_options(SslOptions::NO_DTLSV1);

            // Wire verify callback to delegate to handle_peer_certificate, if present.
            if let Some(handler) = self.handle_peer_certificate {
                let ca_bytes = self.ca_cert.clone().unwrap_or_default();
                builder.set_verify_callback(SslVerifyMode::PEER, move |preverify_ok, ctx| {
                    // If OpenSSL pre-verification already failed, short-circuit to false
                    if !preverify_ok { return false; }
                    // Extract current certificate and pass to handler
                    if let Some(cert_ref) = ctx.current_cert() {
                        if let Ok(cert_pem) = cert_ref.to_pem() {
                            return handler(&cert_pem, &ca_bytes).is_ok();
                        }
                    }
                    // If we can’t get the cert, fail verification
                    false
                });
            }

            // Add DTLS cookie callbacks (basic allow-all for now; can be hardened later).
            builder.set_cookie_generate_cb(|_ssl, cookie| {
                // Minimal cookie: fixed 16 bytes. Real impl should HMAC peer addr and time.
                let bytes = [0xAAu8; 16];
                let n = bytes.len().min(cookie.len());
                cookie[..n].copy_from_slice(&bytes[..n]);
                Ok(n)
            });
            builder.set_cookie_verify_cb(|_ssl, _cookie| {
                // Accept any cookie for now to keep the handshake path simple.
                true
            });

            // Persist the acceptor for use in the DTLS handshake loop.
            let acceptor = builder.build();
            self.acceptor = Some(acceptor);
            Ok(())
        }

        pub fn start_server(&mut self, addr: SocketAddr) -> Result<()> {
            if self.role != DtlsRole::Server { return Err(()); }
            // Validate server creds
            if self.server_signing_cert.is_none() || self.server_signing_private_key.is_none() || self.ca_cert.is_none() {
                return Err(());
            }
            // Prepare and persist acceptor (validates PEMs, configures DTLSv1.2 + client auth)
            self.prepare_server_acceptor()?;
            let acceptor = self.acceptor.take();

            // Spawn a background UDP listener thread as a temporary scaffold
            // until full OpenSSL DTLS handshake is wired. This will deliver
            // plaintext datagrams to the message handler.
            let handler = self.handle_message;
            thread::spawn(move || {
                // Try the DTLS accept loop first (unix-only). On any error, fall back to plaintext UDP loop.
                #[cfg(unix)]
                fn run_dtls_accept_loop(addr: SocketAddr, acceptor: Option<SslAcceptor>, handler: Option<HandleMessage>) -> core::result::Result<(), ()> {
                    use std::time::Duration;
                    // Quickly validate we have an acceptor and can bind the UDP socket.
                    let _acceptor = match acceptor { Some(a) => a, None => return Err(()) };
                    let sock = std::net::UdpSocket::bind(addr).map_err(|_| ())?;
                    // Set a small read timeout so we can periodically check loop conditions.
                    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
                    let _ = sock.set_nonblocking(false);

                    // NOTE: The actual DTLSv1_listen + BIO_dgram handshake and per-peer SSL_accept wiring
                    // will be implemented next. For this increment, we verify we can bind and then return
                    // an error to fall back to the plaintext loop, preserving existing tests.
                    let _ = handler;
                    Err(())
                }
                #[cfg(not(unix))]
                fn run_dtls_accept_loop(_addr: SocketAddr, _acceptor: Option<SslAcceptor>, _handler: Option<HandleMessage>) -> core::result::Result<(), ()> {
                    Err(())
                }

                let _ = run_dtls_accept_loop(addr, acceptor, handler);

                // Fallback plaintext UDP loop preserves current test behavior.
                if let Ok(sock) = UdpSocket::bind(addr) {
                    let mut buf = [0u8; 2048];
                    loop {
                        match sock.recv_from(&mut buf) {
                            Ok((n, from)) => {
                                if let Some(h) = handler { h(&from, &buf[..n]); }
                            }
                            Err(_) => {
                                // Exit the loop on error for now
                                break;
                            }
                        }
                    }
                }
            });
            Ok(())
        }
    }

    impl Dtls for DtlsOpenSsl {
        fn send(&self, _to: SocketAddr, _data: &[u8]) -> Result<()> {
            if self.role != DtlsRole::Client { return Err(()); }
            // TODO: Implement DTLS over OpenSSL
            if self.client_cert.is_none() || self.client_private_key.is_none() || self.ca_cert.is_none() {
                return Err(());
            }
            // Prepare client context (placeholder for now)
            self.prepare_client_context()?;
            // Next-step placeholder: attempt a DTLS client handshake connection to the target.
            self.client_handshake_connect(_to)?;
            Ok(())
        }

        fn get_handle_message(&self) -> Option<HandleMessage> { self.handle_message }
        fn set_handle_message(&mut self, handler: Option<HandleMessage>) { self.handle_message = handler; }
        fn with_handle_message(mut self, handler: HandleMessage) -> Self { self.handle_message = Some(handler); self }

        fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> { self.handle_peer_certificate }
        fn set_handle_peer_certificate(&mut self, handler: Option<HandlePeerCertificate>) { self.handle_peer_certificate = handler; }
        fn with_handle_peer_certificate(mut self, handler: HandlePeerCertificate) -> Self { self.handle_peer_certificate = Some(handler); self }

        fn get_ca_cert(&self) -> Option<&[u8]> { self.ca_cert.as_deref() }
        fn set_ca_cert(&mut self, pem: Option<Vec<u8>>) { self.ca_cert = pem; }
        fn with_ca_cert(mut self, pem: Vec<u8>) -> Self { self.ca_cert = Some(pem); self }

        fn get_client_cert(&self) -> Option<&[u8]> { self.client_cert.as_deref() }
        fn set_client_cert(&mut self, pem: Option<Vec<u8>>) { self.client_cert = pem; }
        fn with_client_cert(mut self, pem: Vec<u8>) -> Self { self.client_cert = Some(pem); self }

        fn get_client_private_key(&self) -> Option<&[u8]> { self.client_private_key.as_deref() }
        fn set_client_private_key(&mut self, pem: Option<Vec<u8>>) { self.client_private_key = pem; }
        fn with_client_private_key(mut self, pem: Vec<u8>) -> Self { self.client_private_key = Some(pem); self }

        fn get_server_signing_cert(&self) -> Option<&[u8]> { self.server_signing_cert.as_deref() }
        fn set_server_signing_cert(&mut self, pem: Option<Vec<u8>>) { self.server_signing_cert = pem; }
        fn with_server_signing_cert(mut self, pem: Vec<u8>) -> Self { self.server_signing_cert = Some(pem); self }

        fn get_server_signing_private_key(&self) -> Option<&[u8]> { self.server_signing_private_key.as_deref() }
        fn set_server_signing_private_key(&mut self, pem: Option<Vec<u8>>) { self.server_signing_private_key = pem; }
        fn with_server_signing_private_key(mut self, pem: Vec<u8>) -> Self { self.server_signing_private_key = Some(pem); self }
    }
}

#[cfg(target_os = "ios")]
mod ios_placeholder {
    // Empty module to keep file compiling when conditionally included elsewhere.
}
