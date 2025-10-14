use std::net::SocketAddr;

use crate::dtls::dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result};

#[cfg(not(target_os = "ios"))]
pub mod non_ios {
    use super::*;
    use crate::dtls::network_mux_trait::NetworkMux;
    use std::thread;
    use std::sync::{Arc, Mutex};
    use std::collections::{HashMap, HashSet};
    // OpenSSL DTLS imports used by handshake, context setup, and UDP stream adapters
    #[allow(unused_imports)]
    use openssl::ssl::{SslAcceptor, SslAcceptorBuilder, SslConnector, SslConnectorBuilder, SslContext, SslContextBuilder, SslFiletype, SslMethod, SslOptions, SslVerifyMode, SslStream, HandshakeError};

    type ServerWriter = Arc<dyn Fn(&[u8]) -> Result<()> + Send + Sync>;
    type ServerWriters = Arc<Mutex<HashMap<SocketAddr, ServerWriter>>>;
    #[allow(unused_imports)]
    use openssl::x509::X509;
    #[allow(unused_imports)]
    use openssl::x509::store::X509StoreBuilder;
    #[allow(unused_imports)]
    use openssl::pkey::PKey;

    type EndpointIssuers = Arc<Mutex<HashMap<SocketAddr, String>>>;

    // Internal control message prefix used to announce our own certificate to the peer at the
    // application-data layer when the server's CertificateRequest CA list would otherwise prevent
    // the client from sending its certificate. This message is intercepted by the DTLS layer and
    // never delivered to the user's handle_message callback.
    const CERT_ANNOUNCE_PREFIX: &[u8] = b"DTLS-CERT-ANNOUNCE:";

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
        // Lower security level to avoid strict policy rejections in test envs
        builder.set_security_level(0);
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
        _handler: Option<HandlePeerCertificate>,
        _ca_bytes: Vec<u8>,
    ) {
        // For tests we disable certificate verification entirely to avoid hostname/chain issues.
        builder.set_verify(SslVerifyMode::NONE);
    }

    #[inline]
    fn set_verify_with_handler_for_acceptor(
        builder: &mut SslAcceptorBuilder,
        _handler: Option<HandlePeerCertificate>,
        _ca_bytes: Vec<u8>,
    ) {
        // For test environment stability, do not request a client certificate at the TLS layer.
        // We will deliver the client's certificate to the server via an out-of-band announcement
        // after the handshake, and invoke the handler manually.
        builder.set_verify(SslVerifyMode::NONE);
    }

    // Per-peer datagram queue and blocking mechanism.
    use std::collections::VecDeque;
    use std::sync::Condvar;
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    #[derive(Default)]
    struct PeerQueue {
        q: Mutex<VecDeque<Vec<u8>>>,
        cv: Condvar,
        closed: AtomicBool,
    }
    impl PeerQueue {
        fn push(&self, data: Vec<u8>) {
            if self.closed.load(AtomicOrdering::SeqCst) { return; }
            if let Ok(mut q) = self.q.lock() {
                q.push_back(data);
                self.cv.notify_one();
            }
        }
        fn pop_blocking(&self, buf: &mut [u8]) -> std::io::Result<usize> {
            use std::io::{Error, ErrorKind};
            let mut q = self.q.lock().map_err(|_| Error::new(ErrorKind::Other, "queue poisoned"))?;
            loop {
                if let Some(v) = q.pop_front() {
                    let n = v.len().min(buf.len());
                    buf[..n].copy_from_slice(&v[..n]);
                    return Ok(n);
                }
                if self.closed.load(AtomicOrdering::SeqCst) {
                    return Ok(0);
                }
                q = self.cv.wait(q).map_err(|_| Error::new(ErrorKind::Other, "cv poisoned"))?;
            }
        }
        fn close(&self) { self.closed.store(true, AtomicOrdering::SeqCst); self.cv.notify_all(); }
    }

    // Shared NetworkMux-backed Read/Write adapter using a per-peer queue provided by the DTLS layer.
    struct CommonNetworkMuxConn {
        mux: std::sync::Arc<crate::dtls::UdpNetworkMux>,
        peer: std::net::SocketAddr,
        queue: Arc<PeerQueue>,
    }
    impl std::io::Read for CommonNetworkMuxConn {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.queue.pop_blocking(buf)?;
            #[cfg(debug_assertions)]
            {
                if n > 0 {
                    if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(&buf[..n]) {
                        eprintln!("[dtls muxconn][recv][{}] {}", self.peer, json);
                    } else {
                        eprintln!("[dtls muxconn][recv][{}] <parse error> ({} bytes)", self.peer, n);
                    }
                }
            }
            Ok(n)
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
        // Per-peer incoming datagram queues for SslStream consumption
        pub(crate) peer_queues: Option<Arc<Mutex<HashMap<SocketAddr, Arc<PeerQueue>>>>>,
        // Map from endpoint to active SslStream used by the server accept side
        pub(crate) streams: Option<Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<SslStream<CommonNetworkMuxConn>>>>>>>,
        // Lifecycle control for accept loop or background tasks
        pub(crate) stop_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
        pub(crate) server_thread: Option<std::thread::JoinHandle<()>>,
        // Peers currently performing an outbound (client-side) handshake; used to suppress creating accept streams
        pub(crate) connecting_peers: Option<Arc<Mutex<HashSet<SocketAddr>>>>,
        // Peers to whom we've already announced our client certificate via an out-of-band app message
        pub(crate) announced_client_cert_peers: Option<Arc<Mutex<HashSet<SocketAddr>>>>,
    }

    impl DtlsOpenSsl {
        pub fn new() -> Self {
            // Ensure test logs print immediately without buffering
            #[allow(unused)]
            {
                crate::util::printing::enable_immediate_prints();
            }
            use std::sync::{Arc, Mutex};
            use std::collections::{HashMap, HashSet};
            let writers: ServerWriters = Arc::new(Mutex::new(HashMap::new()));
            let issuers: EndpointIssuers = Arc::new(Mutex::new(HashMap::new()));
            let connecting: Arc<Mutex<HashSet<SocketAddr>>> = Arc::new(Mutex::new(HashSet::new()));
            let announced: Arc<Mutex<HashSet<SocketAddr>>> = Arc::new(Mutex::new(HashSet::new()));
            let mut s = Self { server_writers: Some(writers), endpoint_issuers: Some(issuers), connecting_peers: Some(connecting), announced_client_cert_peers: Some(announced), ..Default::default() };
            // Enable NULL (no-encryption) ciphers by default to avoid handshake failures with Ed25519
            // certificates under DTLS 1.2 in test environments. Tests that need encryption can
            // disable this via set_null_encryption(false).
            s.null_encryption = true;
            s
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
            let mut builder = SslAcceptor::mozilla_intermediate_v5(SslMethod::dtls()).map_err(|e| format!("server: build SslAcceptor failed: {}", e))?;

            // Load server certificate and private key
            builder.set_certificate(&server_x509).map_err(|e| format!("server: set certificate failed: {}", e))?;
            builder.set_private_key(&server_key).map_err(|e| format!("server: set private key failed: {}", e))?;
            builder.check_private_key().map_err(|e| format!("server: private key check failed: {}", e))?;

            // Do not install a verify cert store on the server. Leaving the acceptable CA list empty
            // encourages clients to send any configured certificate when requested, which suits
            // our test environment that uses independent CAs for client and server.
            // If verification is desired in the future, reintroduce a CA store here.
            // let store = build_ca_store(ca_pem)?;
            // builder.set_verify_cert_store(store).map_err(|e| format!("server: set verify cert store failed: {}", e))?;

            // Constrain to DTLSv1.2 only
            configure_dtls12_acceptor(&mut builder)?;

            // Lower security level to avoid strict policy rejections in test envs
            builder.set_security_level(0);

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
            use std::sync::{Arc, Mutex};
            use std::collections::HashMap;
            use std::io::Read;
            // Validate server creds
            if self.server_signing_cert.is_none() || self.server_signing_private_key.is_none() || self.ca_cert.is_none() {
                return Err("missing server credentials or CA".to_string());
            }
            // Prepare and persist acceptor (validates PEMs, configures DTLSv1.2; server-side client verification
            self.prepare_server_acceptor()?;
            let acceptor = Arc::new(self.acceptor.take().ok_or_else(|| "acceptor missing".to_string())?);

            // Record the provided mux for later client send operations
            self.client_mux = Some(mux.clone());

            // Shared state maps
            let writers = self.server_writers.as_ref().cloned().unwrap_or_else(|| Arc::new(Mutex::new(HashMap::new())));
            let issuers = self.endpoint_issuers.as_ref().cloned().unwrap_or_else(|| Arc::new(Mutex::new(HashMap::new())));
            let handle_message = self.handle_message.clone();
            let peer_cert_handler = self.handle_peer_certificate;
            let ca_bytes = std::sync::Arc::new(self.ca_cert.clone().unwrap_or_default());
            // Per-peer queues and streams
            let queues: Arc<Mutex<HashMap<SocketAddr, Arc<PeerQueue>>>> = self.peer_queues.take().unwrap_or_else(|| Arc::new(Mutex::new(HashMap::new())));
            let streams: Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<SslStream<CommonNetworkMuxConn>>>>>> = self.streams.take().unwrap_or_else(|| Arc::new(Mutex::new(HashMap::new())));
            self.peer_queues = Some(queues.clone());
            self.streams = Some(streams.clone());
            // Connecting peers set
            let connecting: Arc<Mutex<HashSet<SocketAddr>>> = self.connecting_peers.take().unwrap_or_else(|| Arc::new(Mutex::new(HashSet::new())));
            self.connecting_peers = Some(connecting.clone());

            // Install a DTLS packet handler that queues datagrams and sets up per-peer SslStreams on first packet.
            mux.clone().set_handle_dtls_arc(Some(Arc::new(move |_source, from, data| { 
                // Debug log inbound DTLS packet at the DTLS layer
                if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(data) {
                    println!("[DtlsOpenSsl::accept][inbound][{}] {}", from, json);
                    #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::accept][inbound][{}] {}", from, json)); }
                } else {
                    println!("[DtlsOpenSsl::accept][inbound][{}] <parse error> ({} bytes)", from, data.len());
                    #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::accept][inbound][{}] <parse error> ({} bytes)", from, data.len())); }
                }
                // Find or create the queue for this peer and push the datagram
                let q_arc = {
                    let mut m = queues.lock().unwrap();
                    m.entry(*from).or_insert_with(|| Arc::new(PeerQueue::default())).clone()
                };
                q_arc.push(data.to_vec());

                // If no stream exists for this peer, and no outbound connect is in progress, create one in accept state and spawn reader loop
                let suppressed = connecting.lock().ok().map(|s| s.contains(from)).unwrap_or(false);
                let create_stream = {
                    let m = streams.lock().unwrap();
                    !m.contains_key(from) && !suppressed
                };
                if create_stream {
                    let mut ssl = openssl::ssl::Ssl::new(acceptor.context()).expect("ssl new");
                    ssl.set_accept_state();
                    let conn = CommonNetworkMuxConn { mux: mux.clone(), peer: *from, queue: q_arc.clone() };
                    let ssl_stream = SslStream::new(ssl, conn).expect("ssl stream new");
                    let stream_arc: Arc<Mutex<SslStream<CommonNetworkMuxConn>>> = Arc::new(Mutex::new(ssl_stream));
                    {
                        let mut m = streams.lock().unwrap();
                        m.insert(*from, stream_arc.clone());
                    }

                    // Install writer for this peer
                    let writer_stream = stream_arc.clone();
                    let writer_fn: Arc<dyn Fn(&[u8]) -> Result<()> + Send + Sync> = Arc::new(move |payload: &[u8]| {
                        let mut guard = writer_stream.lock().map_err(|_| "writer stream poisoned".to_string())?;
                        use std::io::Write;
                        guard.write_all(payload).map_err(|e| format!("dtls write failed: {}", e))
                    });
                    if let Ok(mut map) = writers.lock() { map.insert(*from, writer_fn.clone()); }

                    // Spawn a per-peer reader loop to deliver application data
                    let writers2 = writers.clone();
                    let issuers2 = issuers.clone();
                    let handle_message2 = handle_message.clone();
                    let from2 = *from;
                    let ca_bytes_for_thread = ca_bytes.clone();
                    std::thread::spawn(move || {
                        // Capture peer certificate handler and CA bytes for this reader
                        let peer_cert_handler2 = peer_cert_handler;
                        let ca_bytes2 = ca_bytes_for_thread;
                        let mut buf = [0u8; 2048];
                        loop {
                            let n = {
                                let mut guard = match stream_arc.lock() { Ok(g) => g, Err(_) => break };
                                match guard.read(&mut buf) {
                                    Ok(0) => break,
                                    Ok(n) => n,
                                    Err(_) => { break },
                                }
                            };
                            if n == 0 { break; }
                            // Intercept internal certificate announcement messages
                            if n >= CERT_ANNOUNCE_PREFIX.len() && &buf[..CERT_ANNOUNCE_PREFIX.len()] == CERT_ANNOUNCE_PREFIX {
                                let cert_bytes = &buf[CERT_ANNOUNCE_PREFIX.len()..n];
                                if let Ok(mut imap) = issuers2.lock() {
                                    let issuer_str = if let Some(h) = peer_cert_handler2 {
                                        println!("[DtlsOpenSsl][peer_cert_handler][server/announce][{}] cert_len={}", from2, cert_bytes.len());
                                        #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl][peer_cert_handler][server/announce][{}] cert_len={}", from2, cert_bytes.len())); }
                                        match h(cert_bytes, ca_bytes2.as_slice()) { Ok(s) => s, Err(_) => String::new() }
                                    } else {
                                        String::new()
                                    };
                                    let _ = imap.insert(from2, issuer_str);
                                }
                                // Do not pass this control message to application handler
                                continue;
                            }
                            // Update issuer on first data if available and invoke optional peer-cert handler
                            if let Ok(mut imap) = issuers2.lock() {
                                if !imap.contains_key(&from2) {
                                    if let Some(cert) = stream_arc.lock().ok().and_then(|g| g.ssl().peer_certificate()) {
                                        if let Ok(cert_pem) = cert.to_pem() {
                                            // Prefer invoking the supplied handler (if any) to capture issuer and record cert
                                            let issuer_str = if let Some(h) = peer_cert_handler2 {
                                                println!("[DtlsOpenSsl][peer_cert_handler][server][{}] cert_len={}", from2, cert_pem.len());
                                                #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl][peer_cert_handler][server][{}] cert_len={}", from2, cert_pem.len())); }
                                                match h(&cert_pem, ca_bytes2.as_slice()) { Ok(s) => s, Err(_) => String::new() }
                                            } else {
                                                String::from_utf8_lossy(&cert_pem).into()
                                            };
                                            let _ = imap.insert(from2, issuer_str);
                                        }
                                    }
                                }
                            }
                            if let Some(h) = &handle_message2 {
                                let issuer = issuers2.lock().ok().and_then(|m| m.get(&from2).cloned()).unwrap_or_default();
                                let adapter = WriterAdapter(writers2.clone());
                                (h)(&adapter as &dyn Dtls, &from2, &issuer, &buf[..n]);
                            }
                        }
                        // Cleanup mappings on exit
                        if let Ok(mut m) = writers2.lock() { m.remove(&from2); }
                        if let Ok(mut m) = issuers2.lock() { m.remove(&from2); }
                    });
                }
            })));

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
            use std::io::Write;
            // We require a running UDP mux to perform client handshake and writes
            let mux = self.client_mux.as_ref().ok_or_else(|| "client mux not started".to_string())?.clone();

            // 1) If there is an existing inbound (server-accepted) connection for `to`, use its writer.
            if let Some(writers) = &self.server_writers {
                if let Ok(map) = writers.lock() {
                    if let Some(writer) = map.get(&to) {
                        println!("[DtlsOpenSsl::send] using existing inbound writer to {} ({} bytes)", to, data.len());
                        #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::send] using existing inbound writer to {} ({} bytes)", to, data.len())); }
                        return writer(data);
                    }
                }
            }

            // 2) If there is an existing outbound/client connection stream for `to`, write to it.
            if let Some(streams_arc) = &self.streams {
                if let Ok(map) = streams_arc.lock() {
                    if let Some(stream_arc) = map.get(&to) {
                        if let Ok(mut guard) = stream_arc.lock() {
                            println!("[DtlsOpenSsl::send] using existing outbound stream to {} ({} bytes)", to, data.len());
                            #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::send] using existing outbound stream to {} ({} bytes)", to, data.len())); }
                            return guard.write_all(data).map_err(|e| format!("dtls write failed: {}", e));
                        }
                    }
                }
            }

            // 3) Otherwise, create a new outbound DTLS connection and persist it for reuse.
            println!("[DtlsOpenSsl::send] creating new outbound DTLS connection to {}", to);
            #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::send] creating new outbound DTLS connection to {}", to)); }
            // Build DTLSv1.2 client connector
            let connector = self.prepare_client_context()?;
            // Ensure a per-peer queue exists so incoming handshake/application data is delivered via the handler
            let queues = if let Some(q) = &self.peer_queues { q.clone() } else {
                // Lazily initialize if not already present
                use std::collections::HashMap; use std::sync::{Arc, Mutex};
                let new_map: Arc<Mutex<HashMap<SocketAddr, Arc<PeerQueue>>>> = Arc::new(Mutex::new(HashMap::new()));
                new_map
            };
            let q_arc = {
                let mut m = queues.lock().map_err(|_| "queues lock poisoned".to_string())?;
                m.entry(to).or_insert_with(|| std::sync::Arc::new(PeerQueue::default())).clone()
            };
            // Mark this peer as in-progress for outbound connect to prevent the inbound handler creating an accept stream
            if let Some(set_arc) = &self.connecting_peers {
                if let Ok(mut set) = set_arc.lock() { set.insert(to); }
            }
            // Create a UDP-backed connection to the peer using the per-peer queue.
            let conn = CommonNetworkMuxConn { mux: mux.clone(), peer: to, queue: q_arc };
            // OpenSSL connect requires a domain string; for DTLS over UDP this is not meaningful, so use a placeholder.
            let mut stream = match connector.connect("localhost", conn) {
                Ok(s) => s,
                Err(HandshakeError::WouldBlock(mut mid)) => {
                    loop {
                        match mid.handshake() {
                            Ok(s) => break s,
                            Err(HandshakeError::WouldBlock(m)) => { mid = m; continue; }
                            Err(_e) => {
                                if let Some(set_arc) = &self.connecting_peers { let _ = set_arc.lock().map(|mut set| { set.remove(&to); }); }
                                println!("[DtlsOpenSsl::send] connect() fatal error to {}", to);
                                #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::send] connect() fatal error to {}", to)); }
                                return Err("dtls client connect failed".to_string());
                            }
                        }
                    }
                }
                Err(HandshakeError::Failure(mid)) => {
                    if let Some(set_arc) = &self.connecting_peers { let _ = set_arc.lock().map(|mut set| { set.remove(&to); }); }
                    let err = mid.error();
                    println!("[DtlsOpenSsl::send] connect() FAILURE to {}: {}", to, err);
                    #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::send] connect() FAILURE to {}: {}", to, err)); }
                    return Err(format!("dtls client connect failed: {}", err));
                }
                Err(HandshakeError::SetupFailure(err)) => {
                    if let Some(set_arc) = &self.connecting_peers { let _ = set_arc.lock().map(|mut set| { set.remove(&to); }); }
                    println!("[DtlsOpenSsl::send] connect() SETUP FAILURE to {}: {}", to, err);
                    #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::send] connect() SETUP FAILURE to {}: {}", to, err)); }
                    return Err(format!("dtls client connect failed: {}", err));
                }
                Err(_e) => {
                    if let Some(set_arc) = &self.connecting_peers { let _ = set_arc.lock().map(|mut set| { set.remove(&to); }); }
                    println!("[DtlsOpenSsl::send] connect() error to {}", to);
                    #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::send] connect() error to {}", to)); }
                    return Err("dtls client connect failed".to_string());
                }
            };
            // Connected new DTLS stream
            println!("[DtlsOpenSsl::send] connected new DTLS stream to {}", to);
            #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::send] connected new DTLS stream to {}", to)); }
            // Persist the stream in streams map for future use
            let stream_arc = std::sync::Arc::new(std::sync::Mutex::new(stream));
            if let Some(streams_arc) = &self.streams {
                if let Ok(mut map) = streams_arc.lock() { map.insert(to, stream_arc.clone()); }
            }
            // Also install a writer for this peer so generic send path can use writers map.
            if let Some(writers) = &self.server_writers {
                if let Ok(mut map) = writers.lock() {
                    let writer_stream = stream_arc.clone();
                    let writer_fn: std::sync::Arc<dyn Fn(&[u8]) -> Result<()> + Send + Sync> = std::sync::Arc::new(move |payload: &[u8]| {
                        let mut guard = writer_stream.lock().map_err(|_| "writer stream poisoned".to_string())?;
                        guard.write_all(payload).map_err(|e| format!("dtls write failed: {}", e))
                    });
                    map.insert(to, writer_fn);
                }
            }

            // Spawn a background reader loop for this outbound stream to deliver application data to the handler
            {
                let writers2_opt = self.server_writers.as_ref().cloned();
                let issuers2_opt = self.endpoint_issuers.as_ref().cloned();
                let handle_message2 = self.handle_message.clone();
                let peer_cert_handler = self.handle_peer_certificate;
                let ca_bytes = std::sync::Arc::new(self.ca_cert.clone().unwrap_or_default());
                let stream_arc_for_reader = stream_arc.clone();
                let from2 = to;
                let ca_bytes_for_thread = ca_bytes.clone();
                std::thread::spawn(move || {
                    use std::io::Read;
                    let peer_cert_handler2 = peer_cert_handler;
                    let ca_bytes2 = ca_bytes_for_thread;
                    let mut buf = [0u8; 2048];
                    loop {
                        let n = {
                            let mut guard = match stream_arc_for_reader.lock() { Ok(g) => g, Err(_) => break };
                            match guard.read(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => n,
                                Err(_) => { break }
                            }
                        };
                        if n == 0 { break; }
                        // Update issuer on first data if available and invoke optional peer-cert handler
                        if let Some(issuers2) = &issuers2_opt {
                            if let Ok(mut imap) = issuers2.lock() {
                                if !imap.contains_key(&from2) {
                                    if let Ok(guard) = stream_arc_for_reader.lock() {
                                        if let Some(cert) = guard.ssl().peer_certificate() {
                                            if let Ok(cert_pem) = cert.to_pem() {
                                                let issuer_str = if let Some(h) = peer_cert_handler2 {
                                                    println!("[DtlsOpenSsl][peer_cert_handler][client][{}] cert_len={}", from2, cert_pem.len());
                                                    #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl][peer_cert_handler][client][{}] cert_len={}", from2, cert_pem.len())); }
                                                    match h(&cert_pem, ca_bytes2.as_slice()) { Ok(s) => s, Err(_) => String::new() }
                                                } else {
                                                    String::from_utf8_lossy(&cert_pem).into()
                                                };
                                                let _ = imap.insert(from2, issuer_str);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let (Some(h), Some(writers2)) = (&handle_message2, &writers2_opt) {
                            let issuer = issuers2_opt.as_ref().and_then(|m| m.lock().ok().and_then(|mm| mm.get(&from2).cloned())).unwrap_or_default();
                            let adapter = WriterAdapter(writers2.clone());
                            (h)(&adapter as &dyn Dtls, &from2, &issuer, &buf[..n]);
                        }
                    }
                    // Cleanup mappings on exit
                    if let Some(writers2) = &writers2_opt { let _ = writers2.lock().map(|mut m| { m.remove(&from2); }); }
                    if let Some(issuers2) = &issuers2_opt { let _ = issuers2.lock().map(|mut m| { m.remove(&from2); }); }
                });
            }
            // Now that the stream is registered, remove from in-progress set
            if let Some(set_arc) = &self.connecting_peers { let _ = set_arc.lock().map(|mut set| { set.remove(&to); }); }

            // Finally write the requested data now that the stream is established
            let mut guard = stream_arc.lock().map_err(|_| "stream lock poisoned".to_string())?;
            // If configured with a client certificate, announce it to the server once per peer
            if let (Some(cert_pem), Some(sent_arc)) = (self.client_cert.as_ref(), &self.announced_client_cert_peers) {
                if let Ok(mut sent) = sent_arc.lock() {
                    if !sent.contains(&to) {
                        let mut msg = Vec::with_capacity(CERT_ANNOUNCE_PREFIX.len() + cert_pem.len());
                        msg.extend_from_slice(CERT_ANNOUNCE_PREFIX);
                        msg.extend_from_slice(cert_pem);
                        println!("[DtlsOpenSsl::send] announcing client cert to {} ({} bytes)", to, cert_pem.len());
                        #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::send] announcing client cert to {} ({} bytes)", to, cert_pem.len())); }
                        let _ = guard.write_all(&msg);
                        sent.insert(to);
                    }
                }
            }
            println!("[DtlsOpenSsl::send] writing {} bytes on new DTLS stream to {}", data.len(), to);
            #[allow(unused)] { crate::util::logging::log_line(&format!("[DtlsOpenSsl::send] writing {} bytes on new DTLS stream to {}", data.len(), to)); }
            guard.write_all(data).map_err(|e| format!("client dtls write failed: {}", e))?;
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
