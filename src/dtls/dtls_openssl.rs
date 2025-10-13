use std::net::SocketAddr;

use crate::dtls::dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result};

#[cfg(not(target_os = "ios"))]
pub mod non_ios {
    use super::*;
    use crate::dtls::network_mux_trait::NetworkMux;
    use std::thread;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;
    // OpenSSL DTLS imports used by handshake, context setup, and UDP stream adapters
    #[allow(unused_imports)]
    use openssl::ssl::{SslAcceptor, SslAcceptorBuilder, SslConnector, SslConnectorBuilder, SslContext, SslContextBuilder, SslFiletype, SslMethod, SslOptions, SslVerifyMode, SslStream};

    type ServerWriter = Arc<dyn Fn(&[u8]) -> Result<()> + Send + Sync>;
    type ServerWriters = Arc<Mutex<HashMap<SocketAddr, ServerWriter>>>;
    #[allow(unused_imports)]
    use openssl::x509::X509;
    #[allow(unused_imports)]
    use openssl::x509::store::X509StoreBuilder;
    #[allow(unused_imports)]
    use openssl::pkey::PKey;

    type EndpointIssuers = Arc<Mutex<HashMap<SocketAddr, String>>>;

    // Common helpers and adapters shared by client/server paths to reduce duplication.
    #[inline]
    fn build_ca_store(ca_pem: &[u8]) -> Result<openssl::x509::store::X509Store> {
        let ca_x509 = X509::from_pem(ca_pem).map_err(|e| format!("CA PEM parse failed: {}", e))?;
        let mut store_builder = X509StoreBuilder::new().map_err(|e| format!("build X509 store failed: {}", e))?;
        store_builder.add_cert(ca_x509).map_err(|e| format!("add CA to store failed: {}", e))?;
        Ok(store_builder.build())
    }

    #[inline]
    fn configure_dtls12_connector(builder: &mut SslConnectorBuilder) -> Result<()> {
        builder.set_options(SslOptions::NO_DTLSV1);
        builder
            .set_min_proto_version(Some(openssl::ssl::SslVersion::DTLS1_2))
            .map_err(|e| format!("client: set_min_proto_version failed: {}", e))?;
        builder
            .set_max_proto_version(Some(openssl::ssl::SslVersion::DTLS1_2))
            .map_err(|e| format!("client: set_max_proto_version failed: {}", e))?;
        builder.set_read_ahead(true);
        Ok(())
    }

    #[inline]
    fn configure_dtls12_acceptor(builder: &mut SslAcceptorBuilder) -> Result<()> {
        builder.set_options(SslOptions::NO_DTLSV1);
        builder
            .set_min_proto_version(Some(openssl::ssl::SslVersion::DTLS1_2))
            .map_err(|e| format!("server: set_min_proto_version failed: {}", e))?;
        builder
            .set_max_proto_version(Some(openssl::ssl::SslVersion::DTLS1_2))
            .map_err(|e| format!("server: set_max_proto_version failed: {}", e))?;
        builder.set_read_ahead(true);
        Ok(())
    }

    #[inline]
    fn enable_null_encryption_for_connector(builder: &mut SslConnectorBuilder) -> Result<()> {
        builder.set_security_level(0);
        builder
            .set_cipher_list("eNULL")
            .map_err(|e| format!("openssl: set cipher list eNULL failed: {}", e))?;
        Ok(())
    }

    #[inline]
    fn enable_null_encryption_for_acceptor(builder: &mut SslAcceptorBuilder) -> Result<()> {
        builder.set_security_level(0);
        builder
            .set_cipher_list("eNULL")
            .map_err(|e| format!("server: set cipher list eNULL failed: {}", e))?;
        Ok(())
    }

    #[inline]
    fn set_verify_with_handler_for_connector(
        builder: &mut SslConnectorBuilder,
        handler: Option<HandlePeerCertificate>,
        ca_bytes: Vec<u8>,
    ) {
        if let Some(h) = handler {
            builder.set_verify_callback(SslVerifyMode::PEER, move |preverify_ok, ctx| {
                if !preverify_ok { return false; }
                if let Some(cert_ref) = ctx.current_cert() {
                    if let Ok(cert_pem) = cert_ref.to_pem() {
                        return h(&cert_pem, &ca_bytes).is_ok();
                    }
                }
                false
            });
        } else {
            builder.set_verify(SslVerifyMode::NONE);
        }
    }

    #[inline]
    fn set_verify_with_handler_for_acceptor(
        builder: &mut SslAcceptorBuilder,
        handler: Option<HandlePeerCertificate>,
        ca_bytes: Vec<u8>,
    ) {
        // Default to NONE for tests
        builder.set_verify(SslVerifyMode::NONE);
        if let Some(h) = handler {
            builder.set_verify_callback(SslVerifyMode::PEER, move |preverify_ok, ctx| {
                if !preverify_ok { return false; }
                if let Some(cert_ref) = ctx.current_cert() {
                    if let Ok(cert_pem) = cert_ref.to_pem() {
                        return h(&cert_pem, &ca_bytes).is_ok();
                    }
                }
                false
            });
        }
    }

    // Shared NetworkMux-backed Read/Write adapter. Optional pre-buffer for server side to replay first datagram.
    struct CommonNetworkMuxConn {
        mux: std::sync::Arc<crate::dtls::UdpNetworkMux>,
        peer: std::net::SocketAddr,
        pre: Vec<u8>,
        off: usize,
    }
    impl std::io::Read for CommonNetworkMuxConn {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            use std::time::Duration;
            if self.off < self.pre.len() {
                let remaining = &self.pre[self.off..];
                let n = remaining.len().min(buf.len());
                buf[..n].copy_from_slice(&remaining[..n]);
                self.off += n;
                return Ok(n);
            }
            loop {
                match self.mux.dtls_recv_from_peer(self.peer, buf) {
                    Ok(n2) => {
                        #[cfg(debug_assertions)]
                        {
                            if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(&buf[..n2]) {
                                eprintln!("[dtls muxconn][recv][{}] {}", self.peer, json);
                            } else {
                                eprintln!("[dtls muxconn][recv][{}] <parse error> ({} bytes)", self.peer, n2);
                            }
                        }
                        return Ok(n2);
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::WouldBlock {
                            std::thread::sleep(Duration::from_millis(1));
                            continue;
                        }
                        return Err(e);
                    }
                }
            }
        }
    }
    impl std::io::Write for CommonNetworkMuxConn {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            #[cfg(debug_assertions)]
            {
                if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(buf) {
                    eprintln!("[dtls muxconn][send][{}] {}", self.peer, json);
                } else {
                    eprintln!("[dtls muxconn][send][{}] <parse error> ({} bytes)", self.peer, buf.len());
                }
            }
            match self.mux.write(self.peer, buf) { Ok(()) => Ok(buf.len()), Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, format!("mux write failed: {}", e))) }
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }

    // Minimal adapter that implements Dtls::send by delegating to the writers map.
    // Used by the client-side background reader to allow handlers to reply using the same DTLS stream.
    struct WriterAdapter(ServerWriters);

    impl Dtls for WriterAdapter {
        fn start(&mut self, _mux: Arc<crate::dtls::UdpNetworkMux>) -> Result<()> { Ok(()) }
        fn stop(&mut self) -> Result<()> { Ok(()) }
        fn send(&self, to: SocketAddr, data: &[u8]) -> Result<()> {
            match self.0.lock() {
                Ok(map) => {
                    if let Some(w) = map.get(&to) { w(data) } else { Err("no writer for peer".to_string()) }
                }
                Err(_) => Err("writers lock poisoned".to_string()),
            }
        }
        fn get_handle_message(&self) -> Option<HandleMessage> { None }
        fn set_handle_message(&mut self, _handler: Option<HandleMessage>) { }
        fn with_handle_message(self, _handler: HandleMessage) -> Self { self }
        fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> { None }
        fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) { }
        fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self { self }
        fn get_ca_cert(&self) -> Option<&[u8]> { None }
        fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) { }
        fn with_ca_cert(self, _pem: Vec<u8>) -> Self { self }
        fn get_client_cert(&self) -> Option<&[u8]> { None }
        fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) { }
        fn with_client_cert(self, _pem: Vec<u8>) -> Self { self }
        fn get_client_private_key(&self) -> Option<&[u8]> { None }
        fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) { }
        fn with_client_private_key(self, _pem: Vec<u8>) -> Self { self }
        fn get_server_signing_cert(&self) -> Option<&[u8]> { None }
        fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) { }
        fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self { self }
        fn get_server_signing_private_key(&self) -> Option<&[u8]> { None }
        fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) { }
        fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self { self }
    }

    /// OpenSSL-backed DTLS implementation (non-iOS).

    #[derive(Default)]
    pub struct DtlsOpenSsl {
        // Optional NetworkMux used for STUN/TURN or raw UDP writes
        pub(crate) network_mux: Option<std::sync::Arc<dyn crate::dtls::NetworkMux + Send + Sync>>,
        // If we created a UDP mux internally on start(None), keep a typed handle to manage its lifecycle
        pub(crate) owned_udp_mux: Option<std::sync::Arc<crate::dtls::UdpNetworkMux>>,
        // Client/General send path requires a started mux; store it when start() is called
        pub(crate) client_mux: Option<std::sync::Arc<crate::dtls::UdpNetworkMux>>,
        // Handlers
        pub(crate) handle_message: Option<HandleMessage>,
        pub(crate) handle_peer_certificate: Option<HandlePeerCertificate>,

        // Credentials
        pub(crate) ca_cert: Option<Vec<u8>>,            // CA certificate (PEM)
        pub(crate) client_cert: Option<Vec<u8>>,        // Client certificate (PEM)
        pub(crate) client_private_key: Option<Vec<u8>>, // Client private key (PEM)
        pub(crate) server_signing_cert: Option<Vec<u8>>, // Server signing certificate (PEM)
        pub(crate) server_signing_private_key: Option<Vec<u8>>, // Server signing private key (PEM)
        // Debug: if true, configure OpenSSL to use NULL (eNULL) cipher suites for no-encryption handshakes.
        pub(crate) null_encryption: bool,

        // State placeholders
        // Prepared DTLS server acceptor (DTLSv1.2), built on start()
        pub(crate) acceptor: Option<SslAcceptor>,
        pub(crate) server_writers: Option<ServerWriters>,
        // Map from endpoint to verified issuer (CN)
        pub(crate) endpoint_issuers: Option<EndpointIssuers>,
        // Lifecycle control for accept loop
        pub(crate) stop_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
        pub(crate) server_thread: Option<std::thread::JoinHandle<()>>,
    }

    impl DtlsOpenSsl {
        pub fn new() -> Self {
            // Ensure test logs print immediately without buffering
            #[allow(unused)]
            {
                crate::util::printing::enable_immediate_prints();
            }
            use std::sync::{Arc, Mutex};
            use std::collections::HashMap;
            let writers: ServerWriters = Arc::new(Mutex::new(HashMap::new()));
            let issuers: EndpointIssuers = Arc::new(Mutex::new(HashMap::new()));
            Self { server_writers: Some(writers), endpoint_issuers: Some(issuers), ..Default::default() }
        }

        /// Enable NULL (no-encryption) ciphers for debugging. Strongly discouraged for production use.
        pub fn with_null_encryption(mut self) -> Self { self.null_encryption = true; self }
        /// Set NULL (no-encryption) ciphers on/off for debugging.
        pub fn set_null_encryption(&mut self, enabled: bool) { self.null_encryption = enabled; }

        // Removed client/server role distinction; context builders are used by send() as-needed
        fn prepare_client_context(&self) -> Result<SslConnector> {
            // Build a DTLSv1.2 client connector and configure mutual auth + verification.
            let mut builder = SslConnector::builder(SslMethod::dtls()).map_err(|e| format!("openssl: build dtls connector: {}", e))?;

            // Restrict to DTLSv1.2 and enable read_ahead.
            configure_dtls12_connector(&mut builder)?;

            // Debug option: allow NULL encryption by lowering security level and selecting eNULL ciphers.
            if self.null_encryption {
                // OpenSSL 3 defaults to security level >=1 which forbids NULL; drop to 0.
                enable_null_encryption_for_connector(&mut builder)?;
            }

            // Optionally load client cert and key if provided.
            if let (Some(cert_pem), Some(key_pem)) = (self.client_cert.as_deref(), self.client_private_key.as_deref()) {
                let client_x509 = X509::from_pem(cert_pem).map_err(|e| format!("client cert PEM parse failed: {}", e))?;
                let client_key = PKey::private_key_from_pem(key_pem).map_err(|e| format!("client private key PEM parse failed: {}", e))?;
                builder.set_certificate(&client_x509).map_err(|e| format!("set client certificate failed: {}", e))?;
                builder.set_private_key(&client_key).map_err(|e| format!("set client private key failed: {}", e))?;
                builder.check_private_key().map_err(|e| format!("client private key check failed: {}", e))?;
            }

            // Optionally install CA cert into the verify store for server auth.
            if let Some(ca_pem) = self.ca_cert.as_deref() {
                let store = build_ca_store(ca_pem)?;
                builder.set_verify_cert_store(store).map_err(|e| format!("client: set verify cert store failed: {}", e))?;
            }

            // Wire client-side verify callback to delegate to handle_peer_certificate, if present.
            let handler = self.handle_peer_certificate;
            let ca_bytes = self.ca_cert.clone().unwrap_or_default();
            set_verify_with_handler_for_connector(&mut builder, handler, ca_bytes);

            // Build and return the configured connector
            Ok(builder.build())
        }


        fn prepare_server_acceptor(&mut self) -> Result<()> {
            // Build an SslAcceptor for DTLSv1.2. For tests we disable client authentication by default; a custom
            // verify callback can be supplied via handle_peer_certificate to enforce checks if desired.
            let ca_pem = self.ca_cert.as_deref().ok_or_else(|| "missing ca_cert".to_string())?;
            let server_cert_pem = self.server_signing_cert.as_deref().ok_or_else(|| "missing server_signing_cert".to_string())?;
            let server_key_pem = self.server_signing_private_key.as_deref().ok_or_else(|| "missing server_signing_private_key".to_string())?;

            let server_x509 = X509::from_pem(server_cert_pem).map_err(|e| format!("server: server cert PEM parse failed: {}", e))?;
            let server_key = PKey::private_key_from_pem(server_key_pem).map_err(|e| format!("server: server private key PEM parse failed: {}", e))?;

            // Context builder for DTLS (we will constrain to DTLSv1.2)
            let mut builder = SslAcceptor::mozilla_modern_v5(SslMethod::dtls()).map_err(|e| format!("server: build SslAcceptor failed: {}", e))?;

            // Load server certificate and private key
            builder.set_certificate(&server_x509).map_err(|e| format!("server: set certificate failed: {}", e))?;
            builder.set_private_key(&server_key).map_err(|e| format!("server: set private key failed: {}", e))?;
            builder.check_private_key().map_err(|e| format!("server: private key check failed: {}", e))?;

            // Install CA into store
            let store = build_ca_store(ca_pem)?;
            builder.set_verify_cert_store(store).map_err(|e| format!("server: set verify cert store failed: {}", e))?;

            // Constrain to DTLSv1.2 only
            configure_dtls12_acceptor(&mut builder)?;

            // Debug option: allow NULL encryption by lowering security level and selecting eNULL ciphers.
            if self.null_encryption {
                enable_null_encryption_for_acceptor(&mut builder)?;
            }

            // Wire verify callback to delegate to handle_peer_certificate, if present.
            let handler = self.handle_peer_certificate;
            let ca_bytes = self.ca_cert.clone().unwrap_or_default();
            set_verify_with_handler_for_acceptor(&mut builder, handler, ca_bytes);

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

        // Start DTLS accept loop using a pre-bound NetworkMux (no internal bind)
        pub fn start_accept_with_mux(&mut self, mux: std::sync::Arc<crate::dtls::UdpNetworkMux>) -> Result<()> {
            // Validate server creds
            if self.server_signing_cert.is_none() || self.server_signing_private_key.is_none() || self.ca_cert.is_none() {
                return Err("missing server credentials or CA".to_string());
            }
            // Require a peer certificate handler to be set; starting without one should fail per API contract.
            if self.handle_peer_certificate.is_none() {
                return Err("handle_peer_certificate not set".to_string());
            }
            // Prepare and persist acceptor (validates PEMs, configures DTLSv1.2; server-side client verification
            // is disabled by default but can be implemented via verify callback).
            self.prepare_server_acceptor()?;
            let acceptor = self.acceptor.take().map(std::sync::Arc::new);
            // Record the provided mux for later client send operations
            self.client_mux = Some(mux.clone());

            Ok(())
        }
    }

    impl Dtls for DtlsOpenSsl {
            fn start(&mut self, mux: std::sync::Arc<crate::dtls::UdpNetworkMux>) -> Result<()> {
                self.start_accept_with_mux(mux)
            }
            fn stop(&mut self) -> Result<()> {
                if let Some(flag) = self.stop_flag.take() {
                    use std::sync::atomic::Ordering;
                    flag.store(true, Ordering::SeqCst);
                }
                let _ = self.server_thread.take();
                // Stop any internally owned UDP mux
                if let Some(mux) = &self.owned_udp_mux {
                    mux.stop();
                }
                self.owned_udp_mux = None;
                Ok(())
            }
        fn send(&self, to: SocketAddr, data: &[u8]) -> Result<()> {
            use std::io::{Read, Write};
            use std::time::Duration;

            eprintln!("[dtls send] send to {} ({} bytes)", to, data.len());
            #[allow(unused)] { crate::util::logging::log_line(&format!("[client] send to {} ({} bytes)", to, data.len())) };


            Ok(())
        }

        fn get_handle_message(&self) -> Option<HandleMessage> { self.handle_message.clone() }
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
