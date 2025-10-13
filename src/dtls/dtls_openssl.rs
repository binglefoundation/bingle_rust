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
    /// Provides:
    /// - A server accept loop (Unix) performing DTLSv1.2 handshakes over UDP and spawning per-peer workers.
    /// - A client path that performs a DTLSv1.2 handshake on first send to a peer.
    /// Notes/limits: synchronous, test-oriented API; hostname verification disabled on client; client verification
    /// is disabled by default on server but can be provided via handle_peer_certificate; no plaintext UDP fallback.

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
            builder.set_options(SslOptions::NO_DTLSV1);
            // Pin protocol to DTLS 1.2 explicitly to avoid DTLS 1.3 negotiation mismatches with server
            builder.set_min_proto_version(Some(openssl::ssl::SslVersion::DTLS1_2)).map_err(|e| format!("client: set_min_proto_version failed: {}", e))?;
            builder.set_max_proto_version(Some(openssl::ssl::SslVersion::DTLS1_2)).map_err(|e| format!("client: set_max_proto_version failed: {}", e))?;
            builder.set_read_ahead(true);

            // Debug option: allow NULL encryption by lowering security level and selecting eNULL ciphers.
            if self.null_encryption {
                // OpenSSL 3 defaults to security level >=1 which forbids NULL; drop to 0.
                builder.set_security_level(0);
                builder.set_cipher_list("eNULL").map_err(|e| format!("openssl: set cipher list eNULL failed: {}", e))?;
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
                let ca_x509 = X509::from_pem(ca_pem).map_err(|e| format!("client: CA PEM parse failed: {}", e))?;
                let mut store_builder = X509StoreBuilder::new().map_err(|e| format!("client: build X509 store failed: {}", e))?;
                store_builder.add_cert(ca_x509).map_err(|e| format!("client: add CA to store failed: {}", e))?;
                let store = store_builder.build();
                builder.set_verify_cert_store(store).map_err(|e| format!("client: set verify cert store failed: {}", e))?;
            }

            // Wire client-side verify callback to delegate to handle_peer_certificate, if present.
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
            } else {
                // For tests, disable peer verification to simplify handshake.
                builder.set_verify(SslVerifyMode::NONE);
            }

            // Build and return the configured connector
            Ok(builder.build())
        }


        fn prepare_server_acceptor(&mut self) -> Result<()> {
            // Build an SslAcceptor for DTLSv1.2. For tests we disable client authentication by default; a custom
            // verify callback can be supplied via handle_peer_certificate to enforce checks if desired.
            let ca_pem = self.ca_cert.as_deref().ok_or_else(|| "missing ca_cert".to_string())?;
            let server_cert_pem = self.server_signing_cert.as_deref().ok_or_else(|| "missing server_signing_cert".to_string())?;
            let server_key_pem = self.server_signing_private_key.as_deref().ok_or_else(|| "missing server_signing_private_key".to_string())?;

            let ca_x509 = X509::from_pem(ca_pem).map_err(|e| format!("server: CA PEM parse failed: {}", e))?;
            let server_x509 = X509::from_pem(server_cert_pem).map_err(|e| format!("server: server cert PEM parse failed: {}", e))?;
            let server_key = PKey::private_key_from_pem(server_key_pem).map_err(|e| format!("server: server private key PEM parse failed: {}", e))?;

            // Context builder for DTLS (we will constrain to DTLSv1.2)
            let mut builder = SslAcceptor::mozilla_modern_v5(SslMethod::dtls()).map_err(|e| format!("server: build SslAcceptor failed: {}", e))?;

            // Load server certificate and private key
            builder.set_certificate(&server_x509).map_err(|e| format!("server: set certificate failed: {}", e))?;
            builder.set_private_key(&server_key).map_err(|e| format!("server: set private key failed: {}", e))?;
            builder.check_private_key().map_err(|e| format!("server: private key check failed: {}", e))?;

            // For tests, do not require client authentication to keep handshake simple
            builder.set_verify(SslVerifyMode::NONE);

            // Install CA into store
            let mut store_builder = X509StoreBuilder::new().map_err(|e| format!("server: build X509 store failed: {}", e))?;
            store_builder.add_cert(ca_x509).map_err(|e| format!("server: add CA to store failed: {}", e))?;
            let store = store_builder.build();
            builder.set_verify_cert_store(store).map_err(|e| format!("server: set verify cert store failed: {}", e))?;

            // Constrain to DTLSv1.2 only
            builder.set_options(SslOptions::NO_DTLSV1);
            // Pin protocol explicitly to DTLS 1.2 to match client
            builder.set_min_proto_version(Some(openssl::ssl::SslVersion::DTLS1_2)).map_err(|e| format!("server: set_min_proto_version failed: {}", e))?;
            builder.set_max_proto_version(Some(openssl::ssl::SslVersion::DTLS1_2)).map_err(|e| format!("server: set_max_proto_version failed: {}", e))?;
            builder.set_read_ahead(true);

            // Debug option: allow NULL encryption by lowering security level and selecting eNULL ciphers.
            if self.null_encryption {
                builder.set_security_level(0);
                builder.set_cipher_list("eNULL").map_err(|e| format!("server: set cipher list eNULL failed: {}", e))?;
            }

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

            // Build a sender instance that can be passed into the handler and reuse per-peer writers.
            let mut sender_inner = DtlsOpenSsl::new();
            sender_inner.ca_cert = self.ca_cert.clone();
            sender_inner.client_cert = self.client_cert.clone();
            sender_inner.client_private_key = self.client_private_key.clone();
            sender_inner.null_encryption = self.null_encryption;
            let writers: ServerWriters = Arc::new(Mutex::new(HashMap::new()));
            // Share writers with both the per-handler sender and this server instance so lookups/registrations are consistent.
            sender_inner.server_writers = Some(writers.clone());
            self.server_writers = Some(writers.clone());
            let sender = std::sync::Arc::new(sender_inner);

            // Initialize stop flag and spawn the DTLS accept thread
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.stop_flag = Some(stop.clone());
            let handler = self.handle_message.clone();
            let peer_cert_handler = self.handle_peer_certificate;
            let ca_bytes_for_handler = std::sync::Arc::new(self.ca_cert.clone().unwrap_or_default());
            let endpoint_issuers = sender.endpoint_issuers.as_ref().cloned();
            let stop_clone = stop.clone();
            let handle = thread::spawn(move || {

                // Run the DTLS accept loop (unix-only). If it exits or fails, the server thread finishes; no plaintext fallback.
                #[cfg(unix)]
                fn run_dtls_accept_loop(mux: std::sync::Arc<crate::dtls::UdpNetworkMux>, acceptor: Option<std::sync::Arc<SslAcceptor>>, handler: Option<HandleMessage>, sender: std::sync::Arc<DtlsOpenSsl>, writers: ServerWriters, stop: std::sync::Arc<std::sync::atomic::AtomicBool>, endpoint_issuers: Option<EndpointIssuers>, peer_cert_handler: Option<HandlePeerCertificate>, ca_bytes_for_handler: std::sync::Arc<Vec<u8>>) -> core::result::Result<(), ()> {
                    use std::io::{Read, Write};
                    use std::time::Duration;

                    // Adapter that exposes a NetworkMux as a Read/Write stream expected by openssl::ssl APIs,
                    // with support for a prefetched first datagram to avoid losing the initial ClientHello (reads filtered to a peer).
                    struct NetworkMuxConn {
                        mux: std::sync::Arc<crate::dtls::UdpNetworkMux>,
                        pre: Vec<u8>,
                        off: usize,
                        peer: std::net::SocketAddr,
                    }
                    impl Read for NetworkMuxConn {
                        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                            use std::time::Duration;
                            // Serve any prefetched bytes first (from initial recv_from in accept loop)
                            if self.off < self.pre.len() {
                                let remaining = &self.pre[self.off..];
                                let n = remaining.len().min(buf.len());
                                buf[..n].copy_from_slice(&remaining[..n]);
                                self.off += n;
                                return Ok(n);
                            }
                            // Only consume datagrams from the designated peer using mux's DTLS queue
                            let mut tmp = vec![0u8; buf.len().max(2048)];
                            loop {
                                match self.mux.dtls_recv_from_peer(self.peer, &mut tmp) {
                                    Ok(n2) => {
                                        #[cfg(debug_assertions)]
                                        {
                                            if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(&tmp[..n2]) {
                                                eprintln!("[dtls][recv][server {}] {}", self.peer, json);
                                            } else {
                                                eprintln!("[dtls][recv][server {}] <parse error> ({} bytes)", self.peer, n2);
                                            }
                                        }
                                        let ncopy = n2.min(buf.len());
                                        buf[..ncopy].copy_from_slice(&tmp[..ncopy]);
                                        return Ok(ncopy);
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
                    impl Write for NetworkMuxConn {
                        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                            #[cfg(debug_assertions)]
                            {
                                if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(buf) {
                                    eprintln!("[dtls][send][server {}] {}", self.peer, json);
                                } else {
                                    eprintln!("[dtls][send][server {}] <parse error> ({} bytes)", self.peer, buf.len());
                                }
                            }
                            match self.mux.write(self.peer, buf) { Ok(()) => Ok(buf.len()), Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, format!("mux write failed: {}", e))) }
                        }
                        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
                    }

                    // Avoid unused warning for writers in some builds
                    let _ = &writers;

                    let acceptor = match acceptor { Some(a) => a, None => return Err(()) };
                    // Use the provided, already-bound mux
                    let mux = mux;

                    loop {
                        use std::sync::atomic::Ordering;
                        if stop.load(Ordering::Relaxed) { break Ok(()); }
                        // Peek at the next datagram to decide if this is a new peer or an existing one.
                        let mut probe = [0u8; 2048];
                        let (_n_peek, from) = match mux.dtls_peek_from(&mut probe) {
                            Ok((n, from)) => (n, from),
                            Err(_) => continue,
                        };
                        // If this peer already has a registered writer/stream, do not consume this packet here;
                        // let the per-client worker's reader handle it.
                        match writers.lock() {
                            Ok(map) => {
                                let present = map.contains_key(&from);
                                eprintln!("[server][writers] contains_key({}) = {}", from, present);
                                #[allow(unused)] { crate::util::logging::log_line(&format!("[server][writers] contains_key({}) = {}", from, present)); }
                                if present {
                                    // Brief nap to avoid a tight spin while the per-client thread drains data.
                                    std::thread::sleep(Duration::from_millis(1));
                                    continue;
                                }
                            }
                            Err(e) => {
                                eprintln!("[server][writers] ERROR: failed to acquire lock in contains_key for {}: {}", from, e);
                                #[allow(unused)] { crate::util::logging::log_line(&format!("[server][writers] ERROR: failed to acquire lock in contains_key for {}: {}", from, e)); }
                                // Backoff to avoid tight spin if lock is poisoned
                                std::thread::sleep(Duration::from_millis(1));
                                continue;
                            }
                        }

                        // This appears to be a new peer: consume the packet now and spawn a worker to handle handshake.
                        let (n, from) = match mux.dtls_recv_from(&mut probe) {
                            Ok((n, from)) => (n, from),
                            Err(_) => continue,
                        };
                        eprintln!("[server] probe from {} ({} bytes)", from, n);
                        #[allow(unused)] { crate::util::logging::log_line(&format!("[server] probe from {} ({} bytes)", from, n)); }
                        #[cfg(debug_assertions)]
                        {
                            if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(&probe[..n]) {
                                eprintln!("[dtls][recv][server {}] {}", from, json);
                                #[allow(unused)] { crate::util::logging::log_line(&format!("[dtls][recv][server {}] {}", from, json)); }
                            } else {
                                eprintln!("[dtls][recv][server {}] <parse error> ({} bytes)", from, n);
                                #[allow(unused)] { crate::util::logging::log_line(&format!("[dtls][recv][server {}] <parse error> ({} bytes)", from, n)); }
                            }
                        }

                        // Pre-register a placeholder writer to mark this peer as in-progress to avoid racing consumption.
                        {
                            match writers.lock() {
                                Ok(mut map) => {
                                    let placeholder: ServerWriter = std::sync::Arc::new(|_payload: &[u8]| -> Result<()> { Err("writer not ready".to_string()) });
                                    let replaced = map.insert(from, placeholder).is_some();
                                    eprintln!("[server][writers] insert placeholder for {} (replaced={})", from, replaced);
                                    #[allow(unused)] { crate::util::logging::log_line(&format!("[server][writers] insert placeholder for {} (replaced={})", from, replaced)); }
                                }
                                Err(e) => {
                                    eprintln!("[server][writers] ERROR: failed to acquire lock in insert placeholder for {}: {}", from, e);
                                    #[allow(unused)] { crate::util::logging::log_line(&format!("[server][writers] ERROR: failed to acquire lock in insert placeholder for {}: {}", from, e)); }
                                }
                            }
                        }

                        // Spawn a per-client worker thread to handle handshake and I/O, keeping the accept loop free.
                        let acc2 = acceptor.clone();
                        let handler2 = handler.clone();
                        let sender_clone = sender.clone();
                        let writers_clone = writers.clone();
                        let prebuf = probe[..n].to_vec();
                        let mux2 = mux.clone();
                        let endpoint_issuers2 = endpoint_issuers.clone();
                        let peer_cert_handler2 = peer_cert_handler;
                        let ca_bytes2 = ca_bytes_for_handler.clone();
                        std::thread::spawn(move || {
                            let _ = mux2.set_read_timeout(Some(Duration::from_millis(1500)));

                            // Attempt DTLS server handshake using NetworkMuxConn filtered to this peer.
                            let stream = match acc2.accept(NetworkMuxConn { mux: mux2, pre: prebuf, off: 0, peer: from }) {
                                Ok(s) => s,
                                Err(_) => {
                                    // cleanup placeholder on handshake failure
                                    if let Ok(mut map) = writers_clone.lock() { let _ = map.remove(&from); }
                                    // also clear any issuer mapping
                                    if let Some(ep) = &endpoint_issuers2 { if let Ok(mut m) = ep.lock() { let _ = m.remove(&from); } }
                                    return;
                                },
                            };

                            // After handshake, extract peer certificate and derive issuer mapping
                            if let (Some(h), Some(ep)) = (peer_cert_handler2, &endpoint_issuers2) {
                                if let Some(cert) = stream.ssl().peer_certificate() {
                                    if let Ok(pem) = cert.to_pem() {
                                        if let Ok(issuer) = h(&pem, &ca_bytes2[..]) {
                                            let _ = ep.lock().map(|mut m| { m.insert(from, issuer); });
                                        } else {
                                            // Verification failed: close connection and cleanup
                                            if let Ok(mut map) = writers_clone.lock() { let _ = map.remove(&from); }
                                            if let Ok(mut m) = ep.lock() { let _ = m.remove(&from); }
                                            return;
                                        }
                                    }
                                }
                            }

                            let shared = std::sync::Arc::new(std::sync::Mutex::new(stream));

                            // Register writer for this peer so handler can call sender.send(to, data)
                            {
                                match writers_clone.lock() {
                                    Ok(mut map) => {
                                        let stream_arc = shared.clone();
                                        let ep_map = endpoint_issuers2.clone();
                                        let writer: ServerWriter = std::sync::Arc::new(move |payload: &[u8]| -> Result<()> {
                                            let mut s = match stream_arc.lock() { Ok(g) => g, Err(e) => return Err(format!("stream lock poisoned: {}", e)) };
                                            use std::io::Write;
                                            if let Err(e) = s.write_all(payload) {
                                                if let Some(ep) = &ep_map { if let Ok(mut m) = ep.lock() { let _ = m.remove(&from); } }
                                                return Err(format!("dtls writer write_all failed: {}", e));
                                            }
                                            let _ = s.flush();
                                            Ok(())
                                        });
                                        let replaced = map.insert(from, writer).is_some();
                                        eprintln!("[server][writers] writer registered for {} (replaced={})", from, replaced);
                                        #[allow(unused)] { crate::util::logging::log_line(&format!("[server][writers] writer registered for {} (replaced={})", from, replaced)); }
                                    }
                                    Err(e) => {
                                        eprintln!("[server][writers] ERROR: failed to acquire lock in insert writer for {}: {}", from, e);
                                        #[allow(unused)] { crate::util::logging::log_line(&format!("[server][writers] ERROR: failed to acquire lock in insert writer for {}: {}", from, e)); }
                                    }
                                }
                            }

                            // Per-client read loop (runs on this worker thread).
                            if let Some(h) = handler2 {
                                loop {
                                    use std::io::Read;
                                    let mut app = [0u8; 2048];
                                    let n = {
                                        let mut s = match shared.lock() { Ok(g) => g, Err(_) => break };
                                        match s.read(&mut app) {
                                            Ok(n) => n,
                                            Err(e) => {
                                                eprintln!("[server] read error from {}: {} (continuing)", from, e);
                                                if let Some(ep) = &endpoint_issuers2 { if let Ok(mut m) = ep.lock() { let _ = m.remove(&from); } }
                                                continue;
                                            }
                                        }
                                    };
                                    if n == 0 { break; }
                                    eprintln!("[server] application data from {} ({} bytes)", from, n);
                                    #[allow(unused)] { crate::util::logging::log_line(&format!("[server] application data from {} ({} bytes)", from, n)); }
                                    let issuer_str = if let Some(ep) = &endpoint_issuers2 {
                                        match ep.lock() {
                                            Ok(m) => m.get(&from).cloned().unwrap_or_default(),
                                            Err(_) => String::new(),
                                        }
                                    } else { String::new() };
                                    h(&*sender_clone, &from, &issuer_str, &app[..n]);
                                }
                            }
                        });

                        // Continue to listen for additional clients after spawning worker.
                        continue;
                    }
                }
                #[cfg(not(unix))]
                fn run_dtls_accept_loop(_mux: std::sync::Arc<crate::dtls::UdpNetworkMux>, _acceptor: Option<std::sync::Arc<SslAcceptor>>, _handler: Option<HandleMessage>, _sender: std::sync::Arc<DtlsOpenSsl>, _writers: ServerWriters, _stop: std::sync::Arc<std::sync::atomic::AtomicBool>, _endpoint_issuers: Option<EndpointIssuers>, _peer_cert_handler: Option<HandlePeerCertificate>, _ca_bytes_for_handler: std::sync::Arc<Vec<u8>>) -> core::result::Result<(), ()> {
                    Err(())
                }

                eprintln!("[server] starting accept loop");
                #[allow(unused)] { crate::util::logging::log_line("[server] starting accept loop"); }
                let _ = run_dtls_accept_loop(mux, acceptor, handler, sender, writers, stop_clone, endpoint_issuers, peer_cert_handler, ca_bytes_for_handler);
                // No plaintext UDP fallback; server thread exits after DTLS accept loop completes or fails.
            });
            self.server_thread = Some(handle);
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

            // If this instance has writers, we may reuse them.
            // Server instances (acceptor.is_some()) always reuse. Client instances reuse only after a successful handshake
            // has recorded an issuer for this peer (to avoid hitting a placeholder during initial send).
            if let Some(writers) = &self.server_writers {
                let mut reuse_ok = false;
                if self.acceptor.is_some() {
                    reuse_ok = true; // server side
                } else if let Some(ep) = &self.endpoint_issuers {
                    if let Ok(m) = ep.lock() { reuse_ok = m.contains_key(&to); }
                }
                if reuse_ok {
                    match writers.lock() {
                        Ok(map) => {
                            if let Some(writer) = map.get(&to) {
                                eprintln!("[dtls send][writers] Got writer for {}", to);
                                #[allow(unused)] { crate::util::logging::log_line(&format!("[client][writers] Got writer for {}", to)) };
                                return writer(data);
                            }
                        }
                        Err(e) => {
                            eprintln!("[dtls send][writers] ERROR: failed to acquire lock in get writer for {}: {}", to, e);
                            #[allow(unused)] { crate::util::logging::log_line(&format!("[client][writers] ERROR: failed to acquire lock in get writer for {}: {}", to, e)); }
                        }
                    }
                } else {
                    eprintln!("[dtls send][writers] Client path: not reusing writer for {} (no issuer yet)", to);
                    #[allow(unused)] { crate::util::logging::log_line(&format!("[client][writers] Client path: not reusing writer for {} (no issuer yet)", to)); }
                }
            }

            // Build a DTLSv1.2 connector via the shared helper
            let connector = self.prepare_client_context()?;

            // Create a temporary UDP NetworkMux and adapt it as a Read/Write stream, mirroring start_accept
            struct NetworkMuxConn {
                mux: std::sync::Arc<crate::dtls::UdpNetworkMux>,
                peer: SocketAddr,
            }
            impl Read for NetworkMuxConn {
                fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                    loop {
                        match self.mux.dtls_recv_from_peer(self.peer, buf) {
                            Ok(n2) => {
                                #[cfg(debug_assertions)]
                                {
                                    if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(&buf[..n2]) {
                                        eprintln!("[dtls send][recv][client {}] {}", self.peer, json);
                                        #[allow(unused)] { crate::util::logging::log_line(&format!("[dtls send][recv][client {}] {}", self.peer, json)); }
                                    } else {
                                        eprintln!("[dtls send][recv][client {}] <parse error> ({} bytes)", self.peer, n2);
                                        #[allow(unused)] { crate::util::logging::log_line(&format!("[dtls send][recv][client {}] <parse error> ({} bytes)", self.peer, n2)); }
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
            impl Write for NetworkMuxConn {
                fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                    #[cfg(debug_assertions)]
                    {
                        if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(buf) {
                            eprintln!("[dtls][send][client {}] {}", self.peer, json);
                            #[allow(unused)] { crate::util::logging::log_line(&format!("[dtls][send][client {}] {}", self.peer, json)); }
                        } else {
                            eprintln!("[dtls][send][client {}] <parse error> ({} bytes)", self.peer, buf.len());
                            #[allow(unused)] { crate::util::logging::log_line(&format!("[dtls][send][client {}] <parse error> ({} bytes)", self.peer, buf.len())); }
                        }
                    }
                    match self.mux.write(self.peer, buf) { Ok(()) => Ok(buf.len()), Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, format!("mux write failed: {}", e))) }
                }
                fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
            }

            // Use the mux provided via start(); require that start() has been called
            let mux = match &self.client_mux { Some(m) => m.clone(), None => { return Err("DTLS send requires start(mux) to be called before send".to_string()); } };
            let _ = mux.set_read_timeout(Some(Duration::from_millis(1500)));

            // Pre-register a placeholder writer for this peer so that server responses to our first handshake
            // packet are considered part of the handshake, not a new-connection probe by any accept loop sharing
            // the same writers map.
            let mut inserted_placeholder = false;
            if let Some(writers) = &self.server_writers {
                match writers.lock() {
                    Ok(mut map) => {
                        if !map.contains_key(&to) {
                            let placeholder: ServerWriter = std::sync::Arc::new(|_payload: &[u8]| -> Result<()> { Err("writer not ready".to_string()) });
                            let _prev = map.insert(to, placeholder);
                            inserted_placeholder = true;
                            eprintln!("[client][writers] insert placeholder for {} (replaced=false)", to);
                            #[allow(unused)] { crate::util::logging::log_line(&format!("[client][writers] insert placeholder for {} (replaced=false)", to)); }
                        } else {
                            eprintln!("[client][writers] contains_key({}) = true (no placeholder inserted)", to);
                            #[allow(unused)] { crate::util::logging::log_line(&format!("[client][writers] contains_key({}) = true (no placeholder inserted)", to)); }
                        }
                    }
                    Err(e) => {
                        eprintln!("[client][writers] ERROR: failed to acquire lock in insert placeholder for {}: {}", to, e);
                        #[allow(unused)] { crate::util::logging::log_line(&format!("[client][writers] ERROR: failed to acquire lock in insert placeholder for {}: {}", to, e)); }
                    }
                }
            }

            // Perform client DTLS handshake using configuration with hostname verification disabled
            eprintln!("[client] connecting DTLS to {}", to);
            #[allow(unused)] { crate::util::logging::log_line(&format!("[client] connecting DTLS to {}", to)); }
            let mut conf = connector.configure().map_err(|e| format!("client: connector configure failed: {}", e))?;
            conf.set_verify_hostname(false);
            let stream = match conf.connect("ignored-host", NetworkMuxConn { mux: mux.clone(), peer: to }) {
                Ok(s) => {
                    eprintln!("[client] handshake ok to {}", to);
                    #[allow(unused)] { crate::util::logging::log_line(&format!("[client] handshake ok to {}", to)); }
                    s
                }
                Err(e) => {
                    eprintln!("[client] handshake failed to {}", to);
                    #[allow(unused)] { crate::util::logging::log_line(&format!("[client] handshake failed to {}", to)); }
                    // Cleanup placeholder on failure so future attempts are not blocked/misleading
                    if inserted_placeholder {
                        if let Some(writers) = &self.server_writers {
                            match writers.lock() {
                                Ok(mut map) => {
                                    let _ = map.remove(&to);
                                    eprintln!("[client][writers] removed placeholder for {} after handshake failure", to);
                                    #[allow(unused)] { crate::util::logging::log_line(&format!("[client][writers] removed placeholder for {} after handshake failure", to)); }
                                }
                                Err(e2) => {
                                    eprintln!("[client][writers] ERROR: failed to acquire lock to remove placeholder for {}: {}", to, e2);
                                    #[allow(unused)] { crate::util::logging::log_line(&format!("[client][writers] ERROR: failed to acquire lock to remove placeholder for {}: {}", to, e2)); }
                                }
                            }
                        }
                    }
                    // Stop mux before returning
                    // mux.stop();
                    return Err(format!("client handshake failed to {}", to));
                }
            };

            // After handshake, extract and map server issuer if handler provided
            if let Some(h) = self.handle_peer_certificate {
                if let Some(cert) = stream.ssl().peer_certificate() {
                    if let Ok(pem) = cert.to_pem() {
                        let ca = self.ca_cert.as_deref().unwrap_or(&[]);
                        let issuer = h(&pem, ca).map_err(|e| format!("client: peer certificate verification failed: {}", e))?;
                        if let Some(epmap) = &self.endpoint_issuers { let _ = epmap.lock().map(|mut m| { m.insert(to, issuer); }); }
                    }
                }
            }

            // Wrap the stream to allow the client handler to write back on the same DTLS connection.
            let shared = std::sync::Arc::new(std::sync::Mutex::new(stream));

            // Register a writer for this peer so handler can call self.send(to, data) and reuse the stream.
            if let Some(writers) = &self.server_writers {
                match writers.lock() {
                    Ok(mut map) => {
                        let stream_arc = shared.clone();
                        let endpoint_issuers = self.endpoint_issuers.as_ref().cloned();
                        let to_addr = to.clone();
                        let writer: ServerWriter = std::sync::Arc::new(move |payload: &[u8]| -> Result<()> {
                            let mut s = match stream_arc.lock() { Ok(g) => g, Err(_) => return Err("stream lock poisoned".to_string()) };
                            use std::io::Write;
                            if let Err(e) = s.write_all(payload) {
                                if let Some(ep) = &endpoint_issuers { if let Ok(mut m) = ep.lock() { let _ = m.remove(&to_addr); } }
                                return Err(format!("dtls writer write_all failed: {}", e));
                            }
                            let _ = s.flush();
                            Ok(())
                        });
                        let replaced = map.insert(to, writer).is_some();
                        eprintln!("[client][writers] writer cached for {} (replaced={})", to_addr, replaced);
                        #[allow(unused)] { crate::util::logging::log_line(&format!("[client][writers] writer cached for {} (replaced={})", to_addr, replaced)); }
                    }
                    Err(e) => {
                        eprintln!("[client][writers] ERROR: failed to acquire lock in insert writer for {}: {}", to, e);
                        #[allow(unused)] { crate::util::logging::log_line(&format!("[client][writers] ERROR: failed to acquire lock in insert writer for {}: {}", to, e)); }
                    }
                }
            }
            
            // Send payload using the established stream; keep stream open via writers map.
            let mut s = shared.lock().map_err(|_| "client: stream lock poisoned".to_string())?;
            let _ = s.write_all(data);
            let _ = s.flush();

            // After sending, perform a single best-effort read to deliver immediate application data (e.g., server Ping).
            // This avoids needing a dedicated background reader while still enabling request/response style flows.
            if let Some(h) = &self.handle_message {
                use std::io::Read;
                let mut buf = [0u8; 2048];
                match s.read(&mut buf) {
                    Ok(n) if n > 0 => {
                        let issuer_str = if let Some(ep) = &self.endpoint_issuers {
                            match ep.lock() { Ok(m) => m.get(&to).cloned().unwrap_or_default(), Err(_) => String::new() }
                        } else { String::new() };
                        if let Some(wm) = &self.server_writers {
                            let adapter = WriterAdapter(wm.clone());
                            // Drop the lock on the stream before invoking the handler to allow re-entrant send.
                            drop(s);
                            h(&adapter, &to, &issuer_str, &buf[..n]);
                        }
                    }
                    _ => {}
                }
            }

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
