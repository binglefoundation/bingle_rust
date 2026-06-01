use crate::dtls::dtls_trait::{Dtls, HandleMessage, HandleNewSession, HandlePeerCertificate, Result};

pub mod openssl_impl {
    use super::*;
    use crate::themes;

    use crate::dtls::network_mux_trait::NetworkMux;
    // OpenSSL DTLS imports used by handshake, context setup, and UDP stream adapters
    #[allow(unused_imports)]
    use openssl::ssl::{HandshakeError, SslAcceptor, SslAcceptorBuilder, SslConnector, SslConnectorBuilder, SslContext, SslContextBuilder, SslFiletype, SslMethod, SslOptions, SslVerifyMode};
    use std::collections::HashMap;
    use std::pin::Pin;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
    use tokio_openssl::SslStream as TokioSslStream;

    #[derive(Clone)]
    enum PeerWriterKind {
        /// Writes directly to the SSL stream from the calling thread using a blocking
        /// `block_on` call on the provided Tokio runtime.  Used during the initial
        /// handshake phase before a dedicated writer thread has been spawned.
        Direct {
            runtime: Arc<tokio::runtime::Runtime>,
            stream_arc: Arc<Mutex<TokioSslStream<CommonNetworkMuxConn>>>,
        },
        /// Sends payloads over an mpsc channel to a dedicated writer thread that owns
        /// the SSL stream write half.  Used after the handshake is complete so that
        /// the calling thread is never blocked waiting for I/O.
        Channel {
            tx: mpsc::Sender<Vec<u8>>,
        },
    }

    #[derive(Clone)]
    struct PeerWriter {
        inner: Arc<Mutex<PeerWriterKind>>,
    }

    impl PeerWriter {
        fn from_direct(runtime: Arc<tokio::runtime::Runtime>, stream_arc: Arc<Mutex<TokioSslStream<CommonNetworkMuxConn>>>) -> Self {
            Self {
                inner: Arc::new(Mutex::new(PeerWriterKind::Direct { runtime, stream_arc })),
            }
        }

        fn from_channel(tx: mpsc::Sender<Vec<u8>>) -> Self {
            Self {
                inner: Arc::new(Mutex::new(PeerWriterKind::Channel { tx })),
            }
        }

        fn switch_to_channel(&self, tx: mpsc::Sender<Vec<u8>>) -> Result<()> {
            let mut guard = self.inner.lock().map_err(|_| "peer writer lock poisoned".to_string())?;
            *guard = PeerWriterKind::Channel { tx };
            Ok(())
        }

        fn send(&self, payload: &[u8]) -> Result<()> {
            enum SendPath {
                Direct {
                    runtime: Arc<tokio::runtime::Runtime>,
                    stream_arc: Arc<Mutex<TokioSslStream<CommonNetworkMuxConn>>>,
                },
                Channel {
                    tx: mpsc::Sender<Vec<u8>>,
                },
            }

            let send_path = {
                let guard = self.inner.lock().map_err(|_| "peer writer lock poisoned".to_string())?;
                match &*guard {
                    PeerWriterKind::Direct { runtime, stream_arc } => SendPath::Direct {
                        runtime: runtime.clone(),
                        stream_arc: stream_arc.clone(),
                    },
                    PeerWriterKind::Channel { tx } => SendPath::Channel { tx: tx.clone() },
                }
            };

            let res = match send_path {
                SendPath::Direct { runtime, stream_arc } => {
                    tracing::debug!("[DtlsOpenSsl:::PeerWriter][send] Direct {} bytes to peer", payload.len());

                    let mut guard = stream_arc.lock().map_err(|_| "writer stream poisoned".to_string())?;
                    runtime
                        .block_on(async { guard.write_all(payload).await })
                        .map_err(|e| format!("send writer stream dtls write failed: {}", e))
                }
                SendPath::Channel { tx } => {
                    tracing::debug!("[DtlsOpenSsl:::PeerWriter][send] Channel {} bytes to peer", payload.len());
                    tx
                        .send(payload.to_vec())
                        .map_err(|e| format!("peer writer channel send failed: {}", e))
                },
            };

            tracing::debug!("[DtlsOpenSsl:::PeerWriter][send] {} bytes done", payload.len());
            res
        }
    }

    /// Spawns a dedicated writer thread that owns an exclusive `WriteHalf` so it never
    /// contends with the reader thread on a shared mutex.
    fn spawn_stream_writer_task_split(
        runtime: Arc<tokio::runtime::Runtime>,
        mut write_half: tokio::io::WriteHalf<TokioSslStream<CommonNetworkMuxConn>>,
        peer_label: String,
    ) -> Result<mpsc::Sender<Vec<u8>>> {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::Builder::new()
            .name(format!("dtls-writer-{}", peer_label))
            .spawn(move || {
                while let Ok(payload) = rx.recv() {
                    if let Err(e) = runtime.block_on(async { write_half.write_all(&payload).await }) {
                        tracing::warn!("[DtlsOpenSsl:::writer] {} write failed: {}", peer_label, e);
                        break;
                    }
                }
            })
            .map_err(|e| format!("failed to spawn split writer task: {}", e))?;
        Ok(tx)
    }
    #[allow(unused_imports)]
    use openssl::pkey::PKey;
    #[allow(unused_imports)]
    use openssl::x509::store::X509StoreBuilder;
    #[allow(unused_imports)]
    use openssl::x509::X509;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum PeerCmd {
        Send(Vec<u8>),
        Stop,
    }

    impl std::fmt::Display for PeerCmd {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                PeerCmd::Send(payload) => {
                    if let Ok(text) = std::str::from_utf8(payload) {
                        if text.chars().all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t') {
                            return write!(f, "Send(\"{}\")", text);
                        }
                    }
                    let preview: String = payload.iter().take(8).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
                    write!(f, "Send({} bytes, [{} ...])", payload.len(), preview)
                }
                PeerCmd::Stop => write!(f, "Stop"),
            }
        }
    }

    #[derive(Clone)]
    pub struct PeerHandle {
        tx: mpsc::Sender<PeerCmd>,
    }

    impl PeerHandle {
        pub fn send(&self, cmd: PeerCmd) -> Result<()> {
            self.tx.send(cmd).map_err(|e| format!("peer command send failed: {}", e))
        }
    }

    pub fn spawn_peer_worker<F>(peer_label: &str, handle: &str, mut on_command: F) -> Result<PeerHandle>
    where
        F: FnMut(PeerCmd) -> bool + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<PeerCmd>();
        let worker_name = format!("dtls-peer-{}", peer_label);
        let handle_tag = handle.to_string();
        std::thread::Builder::new()
            .name(worker_name)
            .spawn(move || {
                while let Ok(cmd) = rx.recv() {
                    let cmd2 = cmd.clone();
                    tracing::debug!("[DtlsOpenSsl:{}] received peer command: {}", handle_tag, cmd2);
                    if !on_command(cmd) {
                        tracing::warn!("[DtlsOpenSsl:{}] peer command handler returned false; terminating worker", handle_tag);
                        break;
                    }
                    tracing::debug!("[DtlsOpenSsl:{}] peer command handler {} returned true; continuing", handle_tag, cmd2);
                }
            })
            .map_err(|e| format!("failed to spawn peer worker: {}", e))?;
        Ok(PeerHandle { tx })
    }

    // Combined per-endpoint state: writer + verified issuer string + per-peer async queue
    const ASYNC_PEER_QUEUE_CAPACITY: usize = 8192;

    #[derive(Clone)]
    struct PeerState {
        writer: Option<PeerWriter>,
        issuer: String,
        async_queue: Arc<AsyncPeerQueue>,
        peer_handle: Option<PeerHandle>,
        is_connecting_peer: bool,
        is_announced_client_cert_peer: bool,
        handshake_logged: bool,
        generation: u64,
    }
    type PeerStates = Arc<Mutex<HashMap<crate::api::bingle_api::NetworkEndpointKey, PeerState>>>;

    fn next_peer_generation(current_generation: u64) -> u64 {
        let mut next_generation = current_generation.wrapping_add(1);
        if next_generation == 0 {
            next_generation = 1;
        }
        next_generation
    }

    fn new_peer_state(generation: u64) -> PeerState {
        PeerState {
            writer: None,
            issuer: String::new(),
            async_queue: Arc::new(AsyncPeerQueue::new(ASYNC_PEER_QUEUE_CAPACITY)),
            peer_handle: None,
            is_connecting_peer: false,
            is_announced_client_cert_peer: false,
            handshake_logged: false,
            generation,
        }
    }

    fn get_or_create_peer_state<'a>(
        map: &'a mut HashMap<crate::api::bingle_api::NetworkEndpointKey, PeerState>,
        key: &crate::api::bingle_api::NetworkEndpointKey,
    ) -> &'a mut PeerState {
        map.entry(key.clone()).or_insert_with(|| new_peer_state(0))
    }

    fn take_peer_state_if_owner(
        map: &mut HashMap<crate::api::bingle_api::NetworkEndpointKey, PeerState>,
        key: &crate::api::bingle_api::NetworkEndpointKey,
        owner_generation: u64,
    ) -> Option<PeerState> {
        let current_generation = map.get(key).map(|ps| ps.generation);
        if current_generation == Some(owner_generation) {
            map.remove(key)
        } else {
            None
        }
    }

    fn close_peer_state(peer_state: PeerState) {
        if let Some(peer_handle) = peer_state.peer_handle {
            tracing::debug!("[DtlsOpenSsl][close_peer_state] send stop to peer_handle");
            let _ = peer_handle.send(PeerCmd::Stop);
        }
        else {
            tracing::debug!("[DtlsOpenSsl][close_peer_state] peer_handle is None, skipping stop command");
        }
        peer_state.async_queue.close();
    }

    // Internal control message prefix used to announce our own certificate to the peer at the
    // application-data layer when the server's CertificateRequest CA list would otherwise prevent
    // the client from sending its certificate. This message is intercepted by the DTLS layer and
    // never delivered to the user's handle_message callback.
    const CERT_ANNOUNCE_PREFIX: &[u8] = b"DTLS-CERT-ANNOUNCE:";

    /// Accept-path background thread entry point.
    ///
    /// Drives the DTLS server-side handshake to completion on the raw (unsplit) stream,
    /// extracts cipher/certificate information from `ssl()` while the full stream is still
    /// owned, records the peer issuer in `peers`, installs a real `PeerWriter` (replacing
    /// the placeholder installed before this thread was spawned), splits the stream via
    /// `tokio::io::split`, and then runs `run_read_loop_split` + `spawn_stream_writer_task_split`
    /// for the post-handshake data phase — no shared mutex needed.
    ///
    /// Returns early (logging a warning) if the handshake fails or any critical step errors.
    #[allow(clippy::too_many_arguments)]
    fn run_accept_stream(
        mut stream: TokioSslStream<CommonNetworkMuxConn>,
        dtls_async_runtime: Arc<tokio::runtime::Runtime>,
        from: NetworkEndpoint,
        peers: PeerStates,
        handle_message: Option<HandleMessage>,
        peer_cert_handler: Option<HandlePeerCertificate>,
        owner_generation: u64,
        handle: String,
    ) {
        let _span = tracing::info_span!("BingleApi", handle = %handle);
        let _guard = _span.enter();
        tracing::info!(
            "[DTLS][DtlsOpenSsl:::accept] run_accept_stream starting for {} (generation={})",
            from,
            owner_generation
        );

        // Step 1: drive the DTLS accept-side handshake to completion.
        let hs_result = dtls_async_runtime.block_on(async {
            match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                std::future::poll_fn(|cx| {
                    let pinned = std::pin::Pin::new(&mut stream);
                    pinned.poll_do_handshake(cx)
                }),
            ).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(format!("DTLS accept handshake error: {}", e)),
                Err(_) => Err("DTLS accept handshake timeout".to_string()),
            }
        });

        if let Err(e) = hs_result {
            tracing::warn!("[DtlsOpenSsl:::accept] handshake failed for {}: {}", from, e);
            if let Ok(mut m) = peers.lock() {
                let key = from.get_key().expect("direct endpoint key");
                if let Some(peer_state) = take_peer_state_if_owner(&mut m, &key, owner_generation) {
                    close_peer_state(peer_state);
                }
            }
            return;
        }

        tracing::info!("[DtlsOpenSsl:::accept] handshake complete for {}", from);

        // Step 2: log selected cipher suite.
        {
            let ssl = stream.ssl();
            let selected = ssl.current_cipher().map(|c| c.name().to_string()).unwrap_or_else(|| "none".to_string());
            let our_ciphers = "DEFAULT:!aNULL:!eNULL:!LOW:!EXPORT:!MD5:!SDK:!ADH:!DSS:!PSK:!SRP:!RC4";
            tracing::info!("[DTLS][handshake {}] completed (accept). Selected: {}. Our available: {}", from, selected, our_ciphers);
            if let Ok(mut m) = peers.lock() {
                let key = from.get_key().expect("direct endpoint key");
                if let Some(ps) = m.get_mut(&key) {
                    ps.handshake_logged = true;
                }
            }
        }

        // Step 3: extract peer certificate from ssl() and record issuer while we still own the stream.
        {
            if let Some(cert) = stream.ssl().peer_certificate() {
                match cert.to_pem() {
                    Ok(cert_pem) => {
                        let ca_bytes_opt: Option<Vec<u8>> = {
                            if let Some(chain) = stream.ssl().peer_cert_chain() {
                                let len = chain.len();
                                if len >= 1 { chain.get(len - 1).and_then(|last| last.to_pem().ok()) } else { None }
                            } else { None }
                        };
                        if let Some(h) = peer_cert_handler {
                            if let Some(ca_vec) = ca_bytes_opt.as_ref() {
                                tracing::debug!(
                                    "[DtlsOpenSsl:::accept][peer_cert_handler][post-handshake][{}] cert_len={} ca_len={}",
                                    from, cert_pem.len(), ca_vec.len()
                                );
                                match h(&cert_pem, ca_vec) {
                                    Ok(mut s) if !s.is_empty() => {
                                        s = s.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
                                        let key = from.get_key().expect("direct endpoint key");
                                        let _ = peers.lock().map(|mut m| {
                                            let ps = get_or_create_peer_state(&mut m, &key);
                                            ps.issuer = s.clone();
                                        });
                                    }
                                    _ => {
                                        tracing::warn!(
                                            "[DtlsOpenSsl:::accept][peer_cert_handler][post-handshake][{}] validation failed; closing",
                                            from
                                        );
                                        if let Ok(mut m) = peers.lock() {
                                            let key = from.get_key().expect("direct endpoint key");
                                            if let Some(peer_state) = take_peer_state_if_owner(&mut m, &key, owner_generation) {
                                                close_peer_state(peer_state);
                                            }
                                        }
                                        return;
                                    }
                                }
                            } else {
                                tracing::warn!(
                                    "[DtlsOpenSsl:::accept][peer_cert_handler][post-handshake][{}] no peer CA chain; closing",
                                    from
                                );
                                if let Ok(mut m) = peers.lock() {
                                    let key = from.get_key().expect("direct endpoint key");
                                    if let Some(peer_state) = take_peer_state_if_owner(&mut m, &key, owner_generation) {
                                        close_peer_state(peer_state);
                                    }
                                }
                                return;
                            }
                        } else {
                            // No handler: record raw PEM as issuer
                            let s: String = String::from_utf8_lossy(&cert_pem).into();
                            let key = from.get_key().expect("direct endpoint key");
                            let _ = peers.lock().map(|mut m| {
                                let ps = get_or_create_peer_state(&mut m, &key);
                                ps.issuer = s;
                            });
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[DtlsOpenSsl:::accept][post-handshake][{}] cert.to_pem failed: {}", from, e);
                    }
                }
            }
            // If no peer certificate: issuer stays empty; the read loop will gate app data delivery.
        }

        // Step 4: split the stream so reader and writer never contend on a shared mutex.
        let (read_half, write_half) = tokio::io::split(stream);

        // Step 5: spawn dedicated writer thread (exclusive WriteHalf ownership, no mutex).
        let peer_label = format!("accept-{}", from.get_key().expect("direct endpoint key"));
        let writer_tx = match spawn_stream_writer_task_split(
            dtls_async_runtime.clone(),
            write_half,
            peer_label,
        ) {
            Ok(tx) => tx,
            Err(e) => {
                tracing::warn!("[DtlsOpenSsl:::accept] failed to spawn split writer for {}: {}", from, e);
                if let Ok(mut m) = peers.lock() {
                    let key = from.get_key().expect("direct endpoint key");
                    if let Some(peer_state) = take_peer_state_if_owner(&mut m, &key, owner_generation) {
                        close_peer_state(peer_state);
                    }
                }
                return;
            }
        };

        // Step 6: install the real writer into peer state (replacing the placeholder).
        if let Ok(mut m) = peers.lock() {
            let key = from.get_key().expect("direct endpoint key");
            if let Some(ps) = m.get_mut(&key) {
                if ps.generation == owner_generation {
                    ps.writer = Some(PeerWriter::from_channel(writer_tx));
                    tracing::debug!("[DtlsOpenSsl:::accept] installed real split writer for {}", from);
                } else {
                    tracing::warn!(
                        "[DtlsOpenSsl:::accept] generation mismatch after handshake for {} (expected {} got {}); closing",
                        from, owner_generation, ps.generation
                    );
                    return;
                }
            }
        }

        // Step 7: run the post-handshake read loop (exclusive ReadHalf ownership, no mutex).
        run_read_loop_split(
            read_half,
            dtls_async_runtime,
            &from,
            peers,
            handle_message,
            peer_cert_handler,
            owner_generation,
            "::accept",
        );
    }

    /// Post-handshake reader loop for the client (outbound) path.
    ///
    /// Takes exclusive ownership of a `ReadHalf` so it never contends with the
    /// writer thread on a shared mutex. The handshake is already complete and the
    /// peer issuer is already recorded in `peers` before this is called, so no
    /// mid-loop SSL inspection is required.
    fn run_read_loop_split(
        mut read_half: tokio::io::ReadHalf<TokioSslStream<CommonNetworkMuxConn>>,
        dtls_async_runtime: Arc<tokio::runtime::Runtime>,
        from: &NetworkEndpoint,
        peers: PeerStates,
        handle_message: Option<HandleMessage>,
        peer_cert_handler: Option<HandlePeerCertificate>,
        owner_generation: u64,
        log_tag: &str,
    ) {
        tracing::info!(
            "[DTLS][DtlsOpenSsl:{}] run_read_loop_split starting for {} (generation={})",
            log_tag,
            from,
            owner_generation
        );
        let key_from = from.get_key().expect("direct endpoint key");
        let get_issuer = || -> Option<String> {
            let key = from.get_key().expect("direct endpoint key");
            match peers.lock() {
                Ok(m) => m.get(&key).map(|ps| ps.issuer.clone()),
                Err(_) => None,
            }
        };
        let mut buf = [0u8; 2048];
        let mut logged_wouldblock = false;
        loop {
            let read_res = dtls_async_runtime.block_on(async {
                match tokio::time::timeout(
                    std::time::Duration::from_millis(20),
                    tokio::io::AsyncReadExt::read(&mut read_half, &mut buf),
                ).await {
                    Ok(res) => res.map_err(|e| e.to_string()),
                    Err(_) => Err("read timeout".to_string()),
                }
            });
            let n = match read_res {
                Ok(0) => {
                    info_theme!(themes::DTLS, "[DtlsOpenSsl:{}][read-loop {}] EOF/peer closed", log_tag, from);
                    break;
                }
                Ok(n) => {
                    logged_wouldblock = false;
                    n
                }
                Err(ref e) if e == "read timeout" || e.contains("would block") || e.contains("WouldBlock") => {
                    if !logged_wouldblock {
                        info_theme!(themes::DTLS, "[DtlsOpenSsl:{}][read-loop {}] WouldBlock/timeout (no datagram yet)", log_tag, from);
                        logged_wouldblock = true;
                    }
                    continue;
                }
                Err(e) => {
                    info_theme!(themes::DTLS, "[DtlsOpenSsl:{}][read-loop {}] read error: {}", log_tag, from, e);
                    break;
                }
            };
            if n == 0 { break; }
            // Gate application delivery on issuer being set (only when peer_cert_handler is configured)
            let issuer_opt = get_issuer();
            if peer_cert_handler.is_some() && issuer_opt.as_deref().unwrap_or("").is_empty() {
                tracing::warn!("[DtlsOpenSsl:{}][read-loop from {}] dropping application data until peer certificate validated", log_tag, from);
                continue;
            }
            tracing::debug!("[DtlsOpenSsl:{}][read-loop {}] application data {} bytes", log_tag, from, n);
            if let Some(h) = &handle_message {
                let issuer = issuer_opt.unwrap_or_default();
                let adapter = PeerAdapter(peers.clone());
                (h)(&adapter as &dyn Dtls, from, &issuer, &buf[..n]);
            }
        }
        // Cleanup
        if let Ok(mut m) = peers.lock() {
            let current_generation = m.get(&key_from).map(|ps| ps.generation);
            if let Some(peer_state) = take_peer_state_if_owner(&mut m, &key_from, owner_generation) {
                tracing::info!(
                    "[DtlsOpenSsl:{}][read-loop {}] peer disconnected, owner generation matched ({}), call close_peer_state",
                    log_tag,
                    from,
                    owner_generation
                );
                close_peer_state(peer_state);
            } else {
                tracing::info!(
                    "[DtlsOpenSsl:{}][read-loop {}] skip cleanup due to ownership mismatch (owner generation={}, current generation={:?})",
                    log_tag,
                    from,
                    owner_generation,
                    current_generation
                );
            }
        }
        tracing::info!("[DtlsOpenSsl:{}][read-loop {}] exit and cleanup", log_tag, from);
    }
    #[inline]
    fn build_ca_store(ca_pem: &[u8]) -> Result<openssl::x509::store::X509Store> {
        let ca_x509 = X509::from_pem(ca_pem).map_err(|e| format!("CA PEM parse failed: {}", e))?;
        let mut store_builder = X509StoreBuilder::new().map_err(|e| format!("build X509 store failed: {}", e))?;
        store_builder.add_cert(ca_x509).map_err(|e| format!("add CA to store failed: {}", e))?;
        Ok(store_builder.build())
    }

    #[inline]
    fn keylog_callback(role: &'static str, handle: String) -> impl Fn(&openssl::ssl::SslRef, &str) + 'static {
        move |_ssl, line| {
            let _span = tracing::info_span!("BingleApi", handle = %handle);
            let _guard = _span.enter();
            let s = format!("[OpenSSL][keylog][{}] {}", role, line);
            tracing::info!("{}", s);
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("target/sslkeylog.log") {
                use std::io::Write as _;
                let _ = writeln!(f, "{}", line);
            }
        }
    }

    // trace at ERROR level when using these
    #[inline]
    pub fn configure_dtls12_connector(builder: &mut SslConnectorBuilder, handle: String, dangerous_debug: bool) -> Result<()> {
        if dangerous_debug {
            // Emit TLS secrets for external analyzers (e.g., Wireshark) using the NSS Key Log Format.
            builder.set_keylog_callback(keylog_callback("client", handle));
            // Lower security level to avoid strict policy rejections in test envs
            builder.set_security_level(0);
        }
        builder.set_options(SslOptions::NO_DTLSV1 | SslOptions::NO_COMPRESSION | SslOptions::NO_RENEGOTIATION);
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
    pub fn configure_dtls12_acceptor(builder: &mut SslAcceptorBuilder, handle: String, dangerous_debug: bool) -> Result<()> {
        if dangerous_debug {
            // Lower security level to avoid strict policy rejections in test envs
            builder.set_security_level(0);
            // Emit TLS secrets for external analyzers (e.g., Wireshark) using the NSS Key Log Format.
            builder.set_keylog_callback(keylog_callback("server", handle));
        }
        builder.set_options(SslOptions::NO_DTLSV1 | SslOptions::NO_COMPRESSION | SslOptions::NO_RENEGOTIATION);
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
        handler: HandlePeerCertificate,
        _ca_bytes: Vec<u8>,
        handle: String,
    ) {
        // Use a verify callback to delegate acceptance to the provided handler.
        // We ignore built-in chain/hostname checks and only fail if the handler returns Err.
        let h = handler;
        builder.set_verify_callback(SslVerifyMode::PEER, move |preverify_ok, x509_ctx| {
            let _span = tracing::info_span!("BingleApi", handle = %handle);
            let _guard = _span.enter();
            // Debug: print parameters received by the verify callback (client)
            tracing::debug!(
                "[DtlsOpenSsl::][verify][client] callback: preverify_ok={} depth={} error={:?} has_cert={} chain_len={}",
                preverify_ok,
                x509_ctx.error_depth(),
                x509_ctx.error(),
                x509_ctx.current_cert().is_some(),
                x509_ctx.chain().map(|c| c.len()).unwrap_or(0)
            );
            // Only evaluate the leaf certificate (depth 0)
            if x509_ctx.error_depth() != 0 {
                tracing::debug!("[DtlsOpenSsl::][verify][client] callback: Skipping non-leaf certificate verification at depth {}", x509_ctx.error_depth());
                return true;
            }
            // Determine peer CA certificate to pass to handler:
            // Extract the issuer from the presented chain (prefer last element when len>=2).
            // Never fall back to locally configured CA; if absent, reject the handshake per policy.
            let mut peer_ca_pem: Option<Vec<u8>> = None;
            if let Some(chain) = x509_ctx.chain() {
                let len = chain.len();
                if len >= 2 {
                    // Prefer the last certificate in the presented chain as the issuing CA
                    if let Some(last) = chain.get(len - 1) {
                        if let Ok(pem) = last.to_pem() { peer_ca_pem = Some(pem); }
                    }
                }
            }
            if peer_ca_pem.is_none() {
                tracing::warn!("[DtlsOpenSsl::][verify][client] no peer CA certificate in presented chain; rejecting per policy");
                return false;
            }

            let cert_verify_status = if let Some(cert) = x509_ctx.current_cert() {
                match cert.to_pem() {
                    Ok(pem) => {
                        let ca_vec = peer_ca_pem.unwrap();
                        match h(&pem, &ca_vec) {
                            Ok(_issuer) => true,
                            Err(e) => {
                                tracing::warn!("[DtlsOpenSsl::][verify][client] handler rejected server cert: {}", e);
                                return false
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[DtlsOpenSsl::][verify][client] to_pem failed: {}", e);
                        false
                    }
                }
            } else {
                // No certificate presented by server; reject.
                tracing::warn!("[DtlsOpenSsl::][verify][client] no server certificate presented");
                false
            };

            tracing::info!("[DtlsOpenSsl::][verify][client] callback: Finished processing server certificate verification, result {}", cert_verify_status);
            cert_verify_status
        });
    }

    #[inline]
    fn set_verify_with_handler_for_acceptor(
        builder: &mut SslAcceptorBuilder,
        handler: HandlePeerCertificate,
        _ca_bytes: Vec<u8>,
        handle: String,
    ) {
        // Use a verify callback driven by the provided handler to decide whether to accept the client.
        // Request a client certificate; fail the handshake if the handler returns Err.
        let h = handler;
        builder.set_verify_callback(SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT, move |preverify_ok, x509_ctx| {
            let _span = tracing::info_span!("BingleApi", handle = %handle);
            let _guard = _span.enter();
            // Debug: print parameters received by the verify callback (server)
            tracing::debug!(
                "[DtlsOpenSsl::][verify][server] callback: preverify_ok={} depth={} error={:?} has_cert={} chain_len={}",
                preverify_ok,
                x509_ctx.error_depth(),
                x509_ctx.error(),
                x509_ctx.current_cert().is_some(),
                x509_ctx.chain().map(|c| c.len()).unwrap_or(0)
            );
            // Only evaluate the leaf certificate (depth 0)
            if x509_ctx.error_depth() != 0 {
                return true;
            }
            // Try to extract peer CA cert from the presented chain (prefer last element if len>=2)
            let mut peer_ca_pem: Option<Vec<u8>> = None;
            if let Some(chain) = x509_ctx.chain() {
                let len = chain.len();
                if len >= 2 {
                    if let Some(last) = chain.get(len - 1) {
                        if let Ok(pem) = last.to_pem() { peer_ca_pem = Some(pem); }
                    }
                }
            }
            if peer_ca_pem.is_none() {
                tracing::warn!("[DtlsOpenSsl::][verify][server] no peer CA certificate in presented chain; rejecting per policy");
                return false;
            }
            if let Some(cert) = x509_ctx.current_cert() {
                match cert.to_pem() {
                    Ok(pem) => {
                        let ca_vec = peer_ca_pem.unwrap();
                        match h(&pem, &ca_vec) {
                            Ok(_issuer) => true,
                            Err(e) => {
                                tracing::warn!("[DtlsOpenSsl::][verify][server] handler rejected client cert: {}", e);
                                false
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[DtlsOpenSsl::][verify][server] to_pem failed: {}", e);
                        false
                    }
                }
            } else {
                // No certificate from client (but we required one): reject.
                tracing::warn!("[DtlsOpenSsl::][verify][server] no client certificate presented");
                false
            }
        });
    }

    use crate::api::network_endpoint::NetworkEndpoint;
    // Per-peer datagram queue with async wakeup semantics.
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    pub(crate) struct AsyncPeerQueue {
        sender: tokio::sync::mpsc::Sender<Vec<u8>>,
        receiver: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Vec<u8>>>,
        closed: AtomicBool,
    }

    impl AsyncPeerQueue {
        pub(crate) fn new(capacity: usize) -> Self {
            let (sender, receiver) = tokio::sync::mpsc::channel(capacity);
            Self {
                sender,
                receiver: tokio::sync::Mutex::new(receiver),
                closed: AtomicBool::new(false),
            }
        }

        pub(crate) fn try_push(&self, data: Vec<u8>) -> std::io::Result<()> {
            use std::io::{Error, ErrorKind};

            if self.closed.load(AtomicOrdering::SeqCst) {
                return Err(Error::new(ErrorKind::BrokenPipe, "queue closed"));
            }

            self.sender
                .try_send(data)
                .map_err(|err| match err {
                    tokio::sync::mpsc::error::TrySendError::Full(_) => {
                        Error::new(ErrorKind::WouldBlock, "async peer queue full")
                    }
                    tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                        Error::new(ErrorKind::BrokenPipe, "async peer queue closed")
                    }
                })
        }

        pub(crate) fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<Vec<u8>>> {
            if self.closed.load(AtomicOrdering::SeqCst) {
                if let Ok(mut receiver) = self.receiver.try_lock() {
                    receiver.close();
                    return receiver.poll_recv(cx);
                }
                return Poll::Ready(None);
            }

            let mut receiver = match self.receiver.try_lock() {
                Ok(guard) => guard,
                Err(_) => return Poll::Pending,
            };
            receiver.poll_recv(cx)
        }

        pub(crate) fn close(&self) {
            self.closed.store(true, AtomicOrdering::SeqCst);
            if let Ok(mut receiver) = self.receiver.try_lock() {
                receiver.close();
            }
        }

        pub(crate) fn is_closed(&self) -> bool {
            self.closed.load(AtomicOrdering::SeqCst)
        }

    }

    // Shared NetworkMux-backed Read/Write adapter using the per-peer async queue provided by the DTLS layer.
    pub(crate) struct CommonNetworkMuxConn {
        mux: std::sync::Arc<crate::dtls::UdpNetworkMux>,
        peer: crate::api::bingle_api::NetworkEndpoint,
        async_queue: Arc<AsyncPeerQueue>,
        read_remainder: Vec<u8>,
    }

    impl std::io::Write for CommonNetworkMuxConn {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            tracing::warn!("[dtls muxconn][Write:write] legacy call {} bytes", buf.len());
            // Prefer direct inet address for logging; otherwise, show relay target if available.
            let peer_str = if let Some(addr) = self.peer.inet_socket_address() {
                addr.to_string()
            } else if let Some(raddr) = self.peer.relay_address() {
                format!("relay:{} ch={:?}", raddr, self.peer.relay_channel())
            } else {
                "<no-dest>".to_string()
            };
            #[cfg(debug_assertions)]
            {
                let from_ip = self.mux.local_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".to_string());
                if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(buf) {
                    tracing::warn!("[dtls muxconn][send][{} -> {}] {}", from_ip, peer_str, json);
                } else {
                    tracing::warn!("[dtls muxconn][send][{} -> {}] <parse error> ({} bytes)", from_ip, peer_str, buf.len());
                }
            }
            // Delegate addressing decision to the UDP mux: it will send direct or wrap as TURN depending on fields set.
            match self.mux.write(&self.peer, buf) {
                Ok(()) => Ok(buf.len()),
                Err(e) => {
                    let from_ip = self.mux.local_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".to_string());
                    tracing::warn!("[dtls muxconn][send][{} -> {}] mux write failed: {}", from_ip, peer_str, e);
                    Err(std::io::Error::new(std::io::ErrorKind::Other, format!("mux write failed: {}", e)))
                }
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            tracing::info!("[dtls muxconn][Write:flush]")   ;
            Ok(())
        }
    }

    impl AsyncRead for CommonNetworkMuxConn {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            let remaining = buf.remaining();
            if remaining == 0 {
                return Poll::Ready(Ok(()));
            }

            if !self.read_remainder.is_empty() {
                let to_copy = std::cmp::min(remaining, self.read_remainder.len());
                buf.put_slice(&self.read_remainder[..to_copy]);
                self.read_remainder.drain(..to_copy);
                return Poll::Ready(Ok(()));
            }

            let recv_result = self.async_queue.poll_recv(cx);

            match recv_result {
                Poll::Ready(Some(packet)) => {
                    let to_copy = std::cmp::min(remaining, packet.len());
                    buf.put_slice(&packet[..to_copy]);
                    if to_copy < packet.len() {
                        self.read_remainder.extend_from_slice(&packet[to_copy..]);
                    }
                    Poll::Ready(Ok(()))
                }
                Poll::Ready(None) => Poll::Ready(Ok(())),
                Poll::Pending => {
                    if self.async_queue.is_closed() {
                        Poll::Ready(Ok(()))
                    } else {
                        Poll::Pending
                    }
                }
            }
        }
    }

    impl AsyncWrite for CommonNetworkMuxConn {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            tracing::info!("[dtls muxconn][Poll_write] poll_write");
            Poll::Ready(std::io::Write::write(&mut *self, buf))
        }

        fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            tracing::info!("[dtls muxconn][Poll_flush] poll_flush");
            Poll::Ready(std::io::Write::flush(&mut *self))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            tracing::info!("[dtls muxconn][Poll_shutdown] poll_shutdown");
            Poll::Ready(Ok(()))
        }
    }

    // Minimal adapter that implements Dtls::send by delegating to the unified peer_states map.
    // Used by the background reader to allow handlers to reply using the same DTLS stream.
    struct PeerAdapter(PeerStates);

    // Do we need this, can we get a reference to API?
    impl Dtls for PeerAdapter {
        fn start(&mut self, _mux: Arc<crate::dtls::UdpNetworkMux>) -> Result<()> { Ok(()) }
        fn stop(&mut self) -> Result<()> { Ok(()) }
        fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
        fn with_app_layer_only_verification(self, _enabled: bool) -> Self { self }
        fn set_dangerous_debug(&mut self, _enabled: bool) {}
        fn with_dangerous_debug(self, _enabled: bool) -> Self where Self: Sized { self }
        fn set_null_encryption(&mut self, _enabled: bool) {}
        fn with_null_encryption(self, _enabled: bool) -> Self where Self: Sized { self }
        fn send(&self, to: &crate::api::bingle_api::NetworkEndpoint, data: &[u8]) -> Result<()> {
            tracing::debug!("[Dtls for PeerAdapter][send] {}", to);
            let key = to
                .get_key()
                .expect("PeerAdapter::send requires a NetworkEndpointKey (inet_socket_address or relay_id)");
            match self.0.lock() {
                Ok(map) => {
                    if let Some(ps) = map.get(&key) {
                        if let Some(w) = &ps.writer { w.send(data) } else { Err("no writer for peer".to_string()) }
                    } else { Err("no writer for peer".to_string()) }
                }
                Err(_) => Err("peers lock poisoned".to_string()),
            }
        }
        fn get_handle_message(&self) -> Option<HandleMessage> { None }
        fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
        fn with_handle_message(self, _handler: HandleMessage) -> Self { self }
        fn set_handle_new_session(&mut self, _handler: Option<HandleNewSession>) {}
        fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> { None }
        fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) {}
        fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self { self }
        fn get_ca_cert(&self) -> Option<&[u8]> { None }
        fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}
        fn with_ca_cert(self, _pem: Vec<u8>) -> Self { self }
        fn get_client_cert(&self) -> Option<&[u8]> { None }
        fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}
        fn with_client_cert(self, _pem: Vec<u8>) -> Self { self }
        fn get_client_private_key(&self) -> Option<&[u8]> { None }
        fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}
        fn with_client_private_key(self, _pem: Vec<u8>) -> Self { self }
        fn get_server_signing_cert(&self) -> Option<&[u8]> { None }
        fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}
        fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self { self }
        fn get_server_signing_private_key(&self) -> Option<&[u8]> { None }
        fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}
        fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self { self }
    }

    /// OpenSSL-backed DTLS implementation (non-iOS).

    pub struct DtlsOpenSsl {
        // Optional NetworkMux used for STUN/TURN or raw UDP writes
        #[allow(dead_code)]
        pub(crate) network_mux: Option<std::sync::Arc<dyn crate::dtls::NetworkMux + Send + Sync>>,
        // If we created a UDP mux internally on start(None), keep a typed handle to manage its lifecycle
        pub(crate) owned_udp_mux: Option<std::sync::Arc<crate::dtls::UdpNetworkMux>>,
        // Client/General send path requires a started mux; store it when start() is called
        pub(crate) client_mux: Option<std::sync::Arc<crate::dtls::UdpNetworkMux>>,
        // Handlers
        pub(crate) handle_message: Option<HandleMessage>,
        pub(crate) handle_new_session: Option<HandleNewSession>,
        pub(crate) handle_peer_certificate: Option<HandlePeerCertificate>,

        // Credentials
        pub(crate) ca_cert: Option<Vec<u8>>,            // CA certificate (PEM)
        pub(crate) client_cert: Option<Vec<u8>>,        // Client certificate (PEM)
        pub(crate) client_private_key: Option<Vec<u8>>, // Client private key (PEM)
        pub(crate) server_signing_cert: Option<Vec<u8>>, // Server signing certificate (PEM)
        pub(crate) server_signing_private_key: Option<Vec<u8>>, // Server signing private key (PEM)
        // Debug: if true, configure OpenSSL to use NULL (eNULL) cipher suites for no-encryption handshakes.
        pub(crate) null_encryption: bool,
        pub(crate) app_layer_only_verification: bool,
        pub(crate) dangerous_debug: bool,

        // State placeholders
        // Prepared DTLS server acceptor (DTLSv1.2), built on start()
        pub(crate) acceptor: Option<SslAcceptor>,
        // Combined peer state map: writer and issuer per endpoint
        peer_states: PeerStates,
        // Lifecycle control for accept loop or background tasks
        pub(crate) stop_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
        pub(crate) server_thread: Option<std::thread::JoinHandle<()>>,
        pub(crate) dtls_async_runtime: Arc<tokio::runtime::Runtime>,
        pub(crate) handle: String,
        pub(crate) span: tracing::Span,
    }

    impl DtlsOpenSsl {
        pub fn new(handle: String) -> Self {
            // Ensure test logs print immediately without buffering
            #[allow(unused)]
            {
                crate::util::printing::enable_immediate_prints();
            }
            use std::collections::HashMap;
            use std::sync::{Arc, Mutex};
            let peers: PeerStates = Arc::new(Mutex::new(HashMap::new()));
            let dtls_async_runtime = Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build dtls async runtime"),
            );
            let span = tracing::info_span!("BingleApi", handle = %handle);
            // Build struct with explicit defaults
            Self {
                network_mux: None,
                owned_udp_mux: None,
                client_mux: None,
                handle_message: None,
                handle_new_session: None,
                handle_peer_certificate: None,
                ca_cert: None,
                client_cert: None,
                client_private_key: None,
                server_signing_cert: None,
                server_signing_private_key: None,
                null_encryption: false,
                app_layer_only_verification: false,
                dangerous_debug: false,
                acceptor: None,
                peer_states: peers,
                stop_flag: None,
                server_thread: None,
                dtls_async_runtime,
                handle,
                span,
            }
        }

        /// Enable NULL (no-encryption) ciphers for debugging. Strongly discouraged for production use.
        pub fn with_null_encryption(mut self) -> Self {
            if self.dangerous_debug {
                self.null_encryption = true;
            } else {
                tracing::error!("[DtlsOpenSsl] Attempted to enable null encryption without dangerous_debug; ignoring");
            }
            self
        }
        /// Set NULL (no-encryption) ciphers on/off for debugging.
        pub fn set_null_encryption(&mut self, enabled: bool) {
            if self.dangerous_debug {
                self.null_encryption = enabled;
            } else if enabled {
                tracing::error!("[DtlsOpenSsl] Attempted to enable null encryption without dangerous_debug; ignoring");
            }
        }

        // Removed client/server role distinction; context builders are used by send() as-needed
        fn prepare_client_context(&self) -> Result<SslConnector> {
            // Build a DTLSv1.2 client connector and configure mutual auth + verification.
            let mut builder = SslConnector::builder(SslMethod::dtls()).map_err(|e| format!("openssl: build dtls connector: {}", e))?;

            // Restrict to DTLSv1.2 and enable read_ahead.
            configure_dtls12_connector(&mut builder, self.handle.clone(), self.dangerous_debug)?;

            // Debug option: allow NULL encryption by lowering security level and selecting eNULL ciphers.
            if self.null_encryption && self.dangerous_debug {
                // OpenSSL 3 defaults to security level >=1 which forbids NULL; drop to 0.
                enable_null_encryption_for_connector(&mut builder)?;
            }

            // Optionally load client cert and key if provided, and install a selection callback so the
            // client will always present its certificate even if the server's acceptable CA list is empty.
            if let (Some(cert_pem), Some(key_pem)) = (self.client_cert.as_deref(), self.client_private_key.as_deref()) {
                let client_x509 = X509::from_pem(cert_pem).map_err(|e| format!("client cert PEM parse failed: {}", e))?;
                let client_key = PKey::private_key_from_pem(key_pem).map_err(|e| format!("client private key PEM parse failed: {}", e))?;
                builder.set_certificate(&client_x509).map_err(|e| format!("set client certificate failed: {}", e))?;
                builder.set_private_key(&client_key).map_err(|e| format!("set client private key failed: {}", e))?;
                builder.check_private_key().map_err(|e| format!("client private key check failed: {}", e))?;

                // Include our CA certificate in the client certificate chain so the server can extract it.
                if let Some(ca_pem) = self.ca_cert.as_deref() {
                    if let Ok(ca_x509) = X509::from_pem(ca_pem) {
                        // Best effort; ignore error to avoid panics
                        let _ = builder.add_extra_chain_cert(ca_x509);
                    }
                }

                // Note: openssl crate version in this repo does not expose a client cert selection callback.
                // We ensure the client certificate is always installed on the connection via the connector context
                // and also directly on the Ssl instance before handshake (see send()).
            }

            // Configure verification per mode
            if self.app_layer_only_verification {
                // Disable built-in certificate verification; validate at application layer instead.
                builder.set_verify(SslVerifyMode::NONE);
            } else {
                // Enforce handshake-time verification via handler; require a handler to be set.
                let ca = self.ca_cert.clone().unwrap_or_default();
                let h = self.handle_peer_certificate.ok_or_else(|| "missing peer certificate handler for client handshake verification".to_string())?;
                set_verify_with_handler_for_connector(&mut builder, h, ca, self.handle.clone());
            }

            // Build and return the configured connector
            Ok(builder.build())
        }


        fn prepare_server_acceptor(&mut self) -> Result<()> {
            // Build an SslAcceptor for DTLSv1.2. For tests we disable client authentication by default; a custom
            // verify callback can be supplied via handle_peer_certificate to enforce checks if desired.
            // Gate this on dangerous_debug option
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

            // Include our CA certificate in the server's certificate chain so clients can extract it
            if let Ok(ca_x509) = X509::from_pem(ca_pem) {
                // Best-effort; ignore errors
                let _ = builder.add_extra_chain_cert(ca_x509);
            }

            // Install verify cert store on the server and advertise acceptable CA list so clients
            // know which certificate to present. The acceptable CA is our virtual CA (VIRTUAL_CA).
            //let store = build_ca_store(ca_pem)?;
            //builder.set_verify_cert_store(store).map_err(|e| format!("server: set verify cert store failed: {}", e))?;
            // Build and set the acceptable CA names list sent in CertificateRequest
            // Intentionally do not advertise a client CA list or request client certificates during handshake.
            // Client identity is validated at the application layer via peer_certificate_handler and DTLS-CERT-ANNOUNCE.
            let _ = ca_pem; // suppress unused warning

            // Constrain to DTLSv1.2 only
            configure_dtls12_acceptor(&mut builder, self.handle.clone(), self.dangerous_debug)?;

            // Debug option: allow NULL encryption by lowering security level and selecting eNULL ciphers.
            if self.null_encryption && self.dangerous_debug {
                enable_null_encryption_for_acceptor(&mut builder)?;
            }

            if self.app_layer_only_verification && self.dangerous_debug {
                // Disable built-in certificate verification; validate at application layer instead.
                builder.set_verify(SslVerifyMode::NONE);
            } else {
                // Prefer handshake-time verification via handler when provided; otherwise, do not require it.
                let ca = self.ca_cert.clone().unwrap_or_default();
                if let Some(h) = self.handle_peer_certificate {
                    // Install verify callback that delegates to the handler and requires a client certificate
                    set_verify_with_handler_for_acceptor(&mut builder, h, ca.clone(), self.handle.clone());
                } else {
                    // No handler provided: do not enforce handshake-time verification on the server side.
                    builder.set_verify(SslVerifyMode::NONE);
                }
                // Also advertise an acceptable CA list and set verify store so clients know which cert to send
                if let Ok(ca_x509) = openssl::x509::X509::from_pem(&ca) {
                    // Set verify cert store
                    if let Ok(store) = build_ca_store(&ca) {
                        let _ = builder.set_verify_cert_store(store);
                    }
                    // Build acceptable CA names list
                    let mut names = openssl::stack::Stack::new().unwrap();
                    if let Ok(name) = ca_x509.subject_name().to_owned() {
                        let _ = names.push(name);
                    }
                    builder.set_client_ca_list(names);
                    // Some clients also look for explicit CA certs in the list
                    let _ = builder.add_client_ca(&ca_x509);
                }
            }

            // Emit TLS secrets for external analyzers (e.g., Wireshark) using the NSS Key Log Format.
            // builder.set_keylog_callback(keylog_callback("server"));

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

        /// Central entry point for all inbound DTLS packets received via the NetworkMux.
        ///
        /// This function routes inbound datagrams to the correct per-peer SslStream and manages
        /// DTLS session lifecycles based on packet type and current peer state:
        ///
        /// ### Packet Type Handling
        /// - **ClientHello**: 
        ///   - If an established stream already exists, it is dropped to allow a reconnect (Client Restart).
        ///   - If no stream exists, it triggers the creation of a new `SslStream` (unless suppressed).
        /// - **Non-Handshake/Other**: 
        ///   - These are enqueued for the existing `SslStream`.
        ///   - If no stream exists yet, they will still trigger stream creation, which is necessary
        ///     to handle cases where the initial `ClientHello` might have been processed or 
        ///     retransmitted.
        ///
        /// ### State-Based Processing
        /// 1. **No Existing State**: Creates a new async queue and `PeerState`, enqueues the 
        ///    packet, and initiates a new `SslStream` in `accept_state`.
        ///
        /// 2. **Existing Established Stream**:
        ///    - If a `ClientHello` arrives, the old session is terminated and a new one starts.
        ///    - Otherwise, the packet is simply enqueued for the existing session.
        ///
        /// 3. **Outbound Connect in Progress (`is_connecting_peer`)**:
        ///    - Implements **Simultaneous Connect Tie-Breaking** to avoid deadlocks where both
        ///      sides are trying to connect and neither is accepting.
        ///    - Compares local and remote socket addresses (port then IP).
        ///    - The "lower" address is the designated client: it suppresses the inbound 
        ///      accept stream, waiting for its own outbound connect to succeed.
        ///    - The "higher" address is the designated server: it aborts its outbound 
        ///      connect and allows the inbound accept stream to proceed.
        ///
        /// 4. **Queue Management**: All inbound datagrams are pushed to the peer's `AsyncPeerQueue`,
        ///    which is read by the `CommonNetworkMuxConn` inside the `SslStream`.
        ///
        /// 5. **Inbound Stream Lifecycle**: If a new stream is needed, it creates an `SslStream` in 
        ///    `accept_state` and spawns a background `run_read_loop` to handle the handshake and 
        ///    subsequent application data.
        fn handle_dtls_accept_packet(
            mux: Arc<crate::dtls::UdpNetworkMux>,
            acceptor: Arc<SslAcceptor>,
            peers: PeerStates,
            dtls_async_runtime: Arc<tokio::runtime::Runtime>,
            handle_message: Option<HandleMessage>,
            handle_new_session: Option<HandleNewSession>,
            peer_cert_handler: Option<HandlePeerCertificate>,
            from: &NetworkEndpoint,
            data: &[u8],
            handle: String,
        ) {
            let tracing_span = tracing::info_span!("BingleApi", handle = %handle);
            let _tracing_span_guard = tracing_span.enter();
            // Check for looping back, verboten here
            let my_ip = mux.local_addr().expect("We must have a local address when accepting inbound");
            if !from.is_relay() && my_ip == from.inet_socket_address().unwrap() {
                tracing::error!("[DtlsOpenSsl:::accept][inbound][{} -> {:?}] <loopback> ({} bytes), ignored", from, my_ip, data.len());
                #[allow(unused)] {}
                return;
            }

            // Debug log inbound DTLS packet at the DTLS layer
            if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(data) {
                tracing::debug!("[DtlsOpenSsl:::accept][inbound][{} -> {:?}] {}", from, my_ip, json);
                #[allow(unused)] {}
            } else {
                tracing::warn!("[DtlsOpenSsl:::accept][inbound][{} -> {:?}] <parse error> ({} bytes)", from, my_ip, data.len());
                #[allow(unused)] {}
            }
            // Find or create the async queue and worker handle for this peer.
            let (async_q_arc, peer_handle) = {
                let key = from.get_key().expect("direct endpoint key");
                let mut pm = peers.lock().unwrap();

                // Detect ClientHello on existing established peer to handle client restarts
                let is_client_hello = data.len() >= 14
                    && data[0] == 0x16       // Handshake
                    && data[3] == 0          // Epoch high byte
                    && data[4] == 0          // Epoch low byte
                    && data[13] == 0x01;     // ClientHello

                if is_client_hello {
                    if pm.get(&key).map(|ps| !ps.is_connecting_peer && ps.writer.is_some()).unwrap_or(false) {
                        if let Some(ps) = pm.remove(&key) {
                            let next_generation = next_peer_generation(ps.generation);
                            tracing::info!("[DtlsOpenSsl:::accept] ClientHello on existing established peer for {} - dropping old peer state to allow reconnect, next_generation={}", from, next_generation);
                            close_peer_state(ps);
                            pm.insert(key.clone(), new_peer_state(next_generation));
                            if let Some(handler) = handle_new_session.as_ref() {
                                handler(from);
                            }
                        }
                    }
                }

                let ps = get_or_create_peer_state(&mut pm, &key);
                (ps.async_queue.clone(), ps.peer_handle.clone())
            };

            if peer_handle.is_none() {
                let peers_for_worker = peers.clone();
                let key_for_worker = from.get_key().expect("direct endpoint key");
                let key_for_worker_lookup = key_for_worker.clone();
                let key_for_worker_log = key_for_worker.clone();
                let peer_label = format!("accept-{}", key_for_worker);
                let new_peer_handle = spawn_peer_worker(&peer_label, &handle, move |cmd| {
                    match cmd {
                        PeerCmd::Send(payload) => {
                            tracing::debug!("[DtlsOpenSsl:::accept] send to worker for {}: {} bytes, locking writer", key_for_worker_log, payload.len());
                            let maybe_writer = if let Ok(map) = peers_for_worker.lock() {
                                tracing::debug!("[DtlsOpenSsl:::accept] lock acquired for {}", key_for_worker_log);
                                map.get(&key_for_worker_lookup).and_then(|ps| ps.writer.clone())
                            } else {
                                tracing::error!("[DtlsOpenSsl:::accept] lock acquisition failed for {}", key_for_worker_log);
                                None
                            };
                            if let Some(writer) = maybe_writer {
                                if let Err(err) = writer.send(&payload) {
                                    tracing::warn!("[DtlsOpenSsl:::accept] peer worker write failed: {}", err);
                                }
                            } else {
                                tracing::warn!("[DtlsOpenSsl:::accept] peer worker send dropped: writer not ready for {}", key_for_worker_log);
                            }
                            true
                        }
                        PeerCmd::Stop => false,
                    }
                }).expect("accept peer worker should spawn");

                if let Ok(mut map) = peers.lock() {
                    if let Some(ps) = map.get_mut(&key_for_worker) {
                        ps.peer_handle = Some(new_peer_handle.clone());
                    }
                }
            }

            tracing::debug!("[DtlsOpenSsl:::accept] enqueue datagram via async queue [{} -> {:?}] ({} bytes)", from, my_ip, data.len());
            #[allow(unused)] {}
            if let Err(async_err) = async_q_arc.try_push(data.to_vec()) {
                if async_err.kind() != std::io::ErrorKind::WouldBlock {
                    tracing::warn!("[DtlsOpenSsl:::accept] async inbound enqueue failed for {}: {}", from, async_err);
                }
            }

            let create_stream = {
                let key = from.get_key().expect("direct endpoint key");
                let pm = peers.lock().unwrap();
                let have_writer = pm.get(&key).map(|ps| ps.writer.is_some()).unwrap_or(false);
                !have_writer // && !suppressed
            };
            if create_stream {
                tracing::debug!("[DtlsOpenSsl:::accept] creating new SslStream (accept_state) for [{} -> {:?}]", from, my_ip);
                #[allow(unused)] {}
                let mut ssl = openssl::ssl::Ssl::new(acceptor.context()).expect("ssl new");
                ssl.set_accept_state();
                let conn = CommonNetworkMuxConn {
                    mux: mux.clone(),
                    peer: from.clone(),
                    async_queue: async_q_arc.clone(),
                    read_remainder: Vec::new(),
                };
                let ssl_stream = TokioSslStream::new(ssl, conn).expect("ssl stream new");

                // Install a placeholder writer immediately so that subsequent inbound packets
                // from this peer see `have_writer == true` and are enqueued rather than
                // triggering a duplicate stream creation.  The real writer is installed by
                // `run_accept_stream` once the handshake completes.
                let (placeholder_tx, _placeholder_rx) = mpsc::channel::<Vec<u8>>();
                let placeholder_writer = PeerWriter::from_channel(placeholder_tx);
                let mut accept_generation = 0u64;

                if let Ok(mut m) = peers.lock() {
                    let key = from.get_key().expect("direct endpoint key");
                    let ps = get_or_create_peer_state(&mut m, &key);
                    accept_generation = next_peer_generation(ps.generation);
                    ps.generation = accept_generation;
                    tracing::debug!("[DtlsOpenSsl:::accept] create_stream, is_connecting_peer={}", ps.is_connecting_peer);
                    ps.writer = Some(placeholder_writer);
                    ps.handshake_logged = false;
                    tracing::debug!("[DtlsOpenSsl:::accept] assigned generation {} for {}", accept_generation, from);
                    tracing::debug!("[DtlsOpenSsl:::accept] installed placeholder writer for {}", from);
                }

                // Spawn a background thread that drives the DTLS accept handshake, extracts
                // SSL state, splits the stream, and runs the post-handshake read loop.
                let peers2 = peers.clone();
                let handle_message2 = handle_message.clone();
                let from2 = from.clone();
                let handle2 = handle.clone();
                std::thread::spawn(move || {
                    run_accept_stream(
                        ssl_stream,
                        dtls_async_runtime,
                        from2,
                        peers2,
                        handle_message2,
                        peer_cert_handler,
                        accept_generation,
                        handle2,
                    );
                });
            }
        }

        pub fn start_accept_with_mux(&mut self, mux: std::sync::Arc<crate::dtls::UdpNetworkMux>) -> Result<()> {
            use std::sync::Arc;
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
            let peers: PeerStates = self.peer_states.clone();
            let dtls_async_runtime = self.dtls_async_runtime.clone();
            let handle_message = self.handle_message.clone();
            let handle_new_session = self.handle_new_session.clone();
            let peer_cert_handler = self.handle_peer_certificate;
            let handle = self.handle.clone();
            // Connecting peer suppression now tracked per-peer in PeerState.is_connecting_peer

            // Install a DTLS packet handler that queues datagrams and sets up per-peer SslStreams on first packet.
            mux.clone().set_handle_dtls_arc(Some(Arc::new(move |_source, from, data| {
                Self::handle_dtls_accept_packet(
                    mux.clone(),
                    acceptor.clone(),
                    peers.clone(),
                    dtls_async_runtime.clone(),
                    handle_message.clone(),
                    handle_new_session.clone(),
                    peer_cert_handler,
                    from,
                    data,
                    handle.clone(),
                );
            })));

            Ok(())
        }
    }

    impl Drop for DtlsOpenSsl {
        fn drop(&mut self) {
            let _ = self.stop();
        }
    }

    impl Dtls for DtlsOpenSsl {
        fn start(&mut self, mux: std::sync::Arc<crate::dtls::UdpNetworkMux>) -> Result<()> {
            self.start_accept_with_mux(mux)
        }

        fn stop(&mut self) -> Result<()> {
            let _guard = self.span.enter();
            tracing::debug!("[DtlsOpenSsl:::stop]");
            if let Some(flag) = self.stop_flag.take() {
                use std::sync::atomic::Ordering;
                flag.store(true, Ordering::SeqCst);
            }
            else {
                tracing::warn!("[DtlsOpenSsl:::stop] already stopped");
            }

            // Clear peer states and close their async queues to signal EOF to background reader threads
            if let Ok(mut map) = self.peer_states.lock() {
                for (_, ps) in map.drain() {
                    tracing::info!("[DtlsOpenSsl:::stop] close peer state");
                    close_peer_state(ps);
                }
            }

            let _ = self.server_thread.take();
            // Stop any internally owned UDP mux
            if let Some(mux) = &self.owned_udp_mux {
                mux.stop();
            }
            else {
                tracing::warn!("[DtlsOpenSsl:::stop] no owned mux");
            }
            self.owned_udp_mux = None;
            tracing::debug!("[DtlsOpenSsl:::stop] done");
            Ok(())
        }

        fn send(&self, to: &crate::api::bingle_api::NetworkEndpoint, data: &[u8]) -> Result<()> {
            let _guard = self.span.enter();
            // We require a running UDP mux to perform client handshake and writes
            let mux = self.client_mux.as_ref().ok_or_else(|| "client mux not started".to_string())?.clone();

            tracing::info!("[DtlsOpenSsl:::send] {:?} -> {} ({} bytes)", mux.local_addr(), to, data.len());
            // Validate destination key: allow either direct inet address or relay channel+address
            let endpoint: &NetworkEndpoint = if to.inet_socket_address().is_some() {
                to
            } else if to.relay_channel().is_some() && to.relay_address().is_some() && to.relay_id().is_some() {
                to
            } else {
                panic!("DtlsOpenSsl::send: invalid NetworkSourceKey: need inet_socket_address or (relay_channel + relay_address + relay_id)");
            };

            let key_to = to.get_key().expect("direct endpoint key");

            // If there is an existing peer worker for `to`, enqueue directly through channel.
            let peer_handle_to_use = {
                let peers = &self.peer_states;
                tracing::info!("[DtlsOpenSsl:::send] Locking peers");
                if let Ok(map) = peers.lock() {
                    tracing::info!("[DtlsOpenSsl:::send] locked peers");
                    if let Some(ps) = map.get(&key_to) {
                        ps.peer_handle.clone()
                    } else {
                        None
                    }
                } else {
                    tracing::error!("[DtlsOpenSsl:::send] peers lock poisoned");
                    None
                }
            };

            if let Some(peer_handle) = peer_handle_to_use {
                tracing::info!("[DtlsOpenSsl:::send] enqueueing send via existing peer worker for {} ({} bytes)", to, data.len());
                return peer_handle.send(PeerCmd::Send(data.to_vec()));
            }

            // 3) Otherwise, create a new outbound DTLS connection and persist it for reuse.
            tracing::info!("[DtlsOpenSsl:::send] creating new outbound DTLS connection to {}", to);
            // Build DTLSv1.2 client connector
            let connector = self.prepare_client_context()?;
            let mut ssl = connector.configure().map_err(|e| e.to_string())?
                .into_ssl("localhost").map_err(|e| e.to_string())?;
            ssl.set_connect_state();

            // Create SslStream and publish writer/worker in peer_states BEFORE the handshake.
            let (stream_arc, peer_handle, owner_generation) = {
                let peers = &self.peer_states;
                let mut map = peers.lock().map_err(|_| "peers lock poisoned".to_string())?;

                let ps = get_or_create_peer_state(&mut map, &key_to);
                let owner_generation = next_peer_generation(ps.generation);
                ps.generation = owner_generation;
                let async_q_arc = ps.async_queue.clone();

                let conn = CommonNetworkMuxConn {
                    mux: mux.clone(),
                    peer: endpoint.clone(),
                    async_queue: async_q_arc.clone(),
                    read_remainder: Vec::new(),
                };
                let stream = TokioSslStream::new(ssl, conn).map_err(|e| e.to_string())?;
                let s_arc = Arc::new(Mutex::new(stream));
                let writer = PeerWriter::from_direct(self.dtls_async_runtime.clone(), s_arc.clone());
                let worker_writer = writer.clone();
                let peer_label = format!("send-{}", key_to);
                let key_to2 = key_to.clone();
                let peer_handle = spawn_peer_worker(&peer_label, &self.handle, move |cmd| {
                    match cmd {
                        PeerCmd::Send(payload) => {
                            tracing::debug!("[DtlsOpenSsl:::send] PeerCmd::Send to worker for {}: {} bytes, locking writer", key_to2, payload.len());
                            if let Err(err) = worker_writer.send(&payload) {
                                tracing::warn!("[DtlsOpenSsl:::send] peer worker write failed: {}", err);
                                return true;
                            }
                            tracing::debug!("[DtlsOpenSsl:::send] PeerCmd::Send to worker for {}: done", key_to2);
                            true
                        }
                        PeerCmd::Stop => false,
                    }
                })?;

                tracing::debug!("[DtlsOpenSsl:::send] on {:?}, initialize/update peer state with stream and is_connecting_peer=true for {}", mux.local_addr(), to);
                ps.writer = Some(writer);
                ps.issuer.clear();
                ps.peer_handle = Some(peer_handle.clone());
                ps.is_connecting_peer = true;
                ps.is_announced_client_cert_peer = false;
                ps.handshake_logged = false;
                tracing::debug!("[DtlsOpenSsl:::send] assigned generation {} for {}", owner_generation, to);

                (s_arc, peer_handle, owner_generation)
            };

            let mut stream = stream_arc.lock().map_err(|_| "newly created stream lock poisoned".to_string())?;

            tracing::info!("[DtlsOpenSsl:::send] starting DTLS connect/handshake to {} with 10s deadline", to);
            use std::time::Instant;
            let start = Instant::now();
            let runtime = self.dtls_async_runtime.clone();

            let io_result = runtime.block_on(async {
                tokio::time::timeout(std::time::Duration::from_millis(10_000), async {
                    stream.write_all(data).await.map_err(|e| e.to_string())
                })
                .await
            });

            match io_result {
                Ok(Ok(())) => {
                    let ms = start.elapsed().as_millis();
                    let ssl = stream.ssl();
                    let selected = ssl.current_cipher().map(|c| c.name().to_string()).unwrap_or_else(|| "none".to_string());
                    let our_ciphers = "DEFAULT:!aNULL:!eNULL:!LOW:!EXPORT:!MD5:!SDK:!ADH:!DSS:!PSK:!SRP:!RC4";
                    if let Ok(mut m) = self.peer_states.lock() {
                        if let Some(ps) = m.get_mut(&key_to) {
                            if ps.generation != owner_generation {
                                tracing::debug!(
                                    "[DtlsOpenSsl:::send] skip handshake_logged update for {} due to generation mismatch (owner={}, current={})",
                                    to,
                                    owner_generation,
                                    ps.generation
                                );
                            } else {
                                ps.handshake_logged = true;
                            }
                        }
                    }
                    tracing::info!("[DTLS][handshake {}] completed (client). Selected: {}. Our available: {}", to, selected, our_ciphers);
                    tracing::info!("[DtlsOpenSsl:::send] first packet sent and handshake completed to {} in {}ms", to, ms);
                }
                Ok(Err(err)) => {
                    if let Ok(mut m) = self.peer_states.lock() {
                        tracing::debug!(
                            "[DtlsOpenSsl:::send] removing peer state for {} (write failure, owner generation={})",
                            to,
                            owner_generation
                        );
                        if let Some(peer_state) = take_peer_state_if_owner(&mut m, &key_to, owner_generation) {
                            close_peer_state(peer_state);
                        } else {
                            tracing::debug!(
                                "[DtlsOpenSsl:::send] skip write-failure cleanup for {} due to generation mismatch",
                                to
                            );
                        }
                    }
                    tracing::warn!("[DtlsOpenSsl:::send] connect/write FAILURE to {}: {}", to, err);
                    return Err(format!("dtls client write failed: {}", err));
                }
                Err(_) => {
                    if let Ok(mut m) = self.peer_states.lock() {
                        tracing::debug!(
                            "[DtlsOpenSsl:::send] removing peer state for {} (timeout, owner generation={})",
                            to,
                            owner_generation
                        );
                        if let Some(peer_state) = take_peer_state_if_owner(&mut m, &key_to, owner_generation) {
                            close_peer_state(peer_state);
                        } else {
                            tracing::debug!(
                                "[DtlsOpenSsl:::send] skip timeout cleanup for {} due to generation mismatch",
                                to
                            );
                        }
                    }
                    tracing::warn!("[DtlsOpenSsl:::send] connect/write timeout to {} after {}ms", to, start.elapsed().as_millis());
                    return Err("dtls client connect timeout".to_string());
                }
            }
            // Connected new DTLS stream
            tracing::info!("[DtlsOpenSsl:::send] connected new DTLS stream to {}", to);
            #[allow(unused)] {}

            // Immediately invoke peer certificate handler on the client side with the server's certificate
            // (while we still hold the `stream` guard, before splitting the stream).
            if let Some(h) = self.handle_peer_certificate {
                if let Some(cert) = stream.ssl().peer_certificate() {
                    match cert.to_pem() {
                        Ok(cert_pem) => {
                            // Extract peer CA from the presented chain (prefer last element)
                            let mut peer_ca_pem: Option<Vec<u8>> = None;
                            if let Some(chain) = stream.ssl().peer_cert_chain() {
                                let len = chain.len();
                                if len >= 1 {
                                    if let Some(last) = chain.get(len - 1) {
                                        if let Ok(pem) = last.to_pem() { peer_ca_pem = Some(pem); }
                                    }
                                }
                            }
                            let ca_len = peer_ca_pem.as_ref().map(|v| v.len()).unwrap_or(0);
                            tracing::debug!("[DtlsOpenSsl::][peer_cert_handler][client/post-connect][{:?} -> {}] cert_len={} ca_len={} (from peer chain)", mux.local_addr(), to, cert_pem.len(), ca_len);
                            #[allow(unused)] {}
                            if let Some(ca) = peer_ca_pem.as_ref() {
                                match h(&cert_pem, ca) {
                                    Ok(issuer) if !issuer.is_empty() => {
                                        // Convert issuer (subject CN) to id by trimming the trailing ISSUER_SUFFIX
                                        let id = issuer.trim_end_matches(crate::protocol::ISSUER_SUFFIX).to_string();
                                        tracing::debug!("[DtlsOpenSsl::][peer_cert_handler][client/post-connect][{}] issuer={} id={}", to, issuer, id);
                                        let peers = &self.peer_states;
                                        let _ = peers.lock().map(|mut m| {
                                            if let Some(ps) = m.get_mut(&key_to) {
                                                if ps.generation == owner_generation {
                                                    tracing::debug!("[DtlsOpenSsl:::send] initialize is_connecting_peer=false for {}", to);
                                                    ps.issuer = id.clone();
                                                }
                                            }
                                        });
                                    }
                                    Ok(_) => {
                                        tracing::warn!("[DtlsOpenSsl::][peer_cert_handler][client/post-connect][{}] empty issuer returned; will defer app data delivery until validated", to);
                                        #[allow(unused)] {}
                                    }
                                    Err(e) => {
                                        tracing::error!("[DtlsOpenSsl::][peer_cert_handler][client/post-connect][{}] handler error: {}", to, e);
                                        #[allow(unused)] {}
                                    }
                                }
                            } else {
                                tracing::error!("[DtlsOpenSsl::][peer_cert_handler][client/post-connect][{}] no peer CA certificate in chain; skipping handler invocation", to);
                                #[allow(unused)] {}
                            }
                        }
                        Err(e) => {
                            tracing::error!("[DtlsOpenSsl::][peer_cert_handler][client/post-connect][{}] to_pem failed: {}", to, e);
                            #[allow(unused)] {}
                        }
                    }
                } else {
                    tracing::error!("[DtlsOpenSsl::][peer_cert_handler][client/post-connect][{}] no server certificate available", to);
                    #[allow(unused)] {}
                    return Err("no peer certificate presented".to_string());
                }
            }

            // Announce client certificate to the server while we still hold the stream guard.
            // This must happen before we split the stream, so that the guard is the only writer.
            if let Some(cert_pem) = self.client_cert.as_ref() {
                let mut should_send = false;
                {
                    let peers = &self.peer_states;
                    let _ = peers.lock().map(|mut m| {
                        if let Some(ps) = m.get_mut(&key_to) {
                            if !ps.is_announced_client_cert_peer {
                                ps.is_announced_client_cert_peer = true;
                                should_send = true;
                            }
                        }
                    });
                }
                if should_send {
                    let ca_pem = self.ca_cert.as_deref().unwrap_or(&[]);
                    let mut msg = Vec::with_capacity(CERT_ANNOUNCE_PREFIX.len() + cert_pem.len() + 1 + ca_pem.len());
                    msg.extend_from_slice(CERT_ANNOUNCE_PREFIX);
                    msg.extend_from_slice(cert_pem);
                    // Separate with a newline if the cert block doesn't already end with one
                    if !cert_pem.last().map(|b| *b == b'\n').unwrap_or(false) { msg.push(b'\n'); }
                    msg.extend_from_slice(ca_pem);
                    tracing::debug!("[DtlsOpenSsl:::send] announcing client cert to {} (cert_len={} ca_len={})", to, cert_pem.len(), ca_pem.len());
                    #[allow(unused)] {}
                    let _ = stream.write_all(&msg);
                }
            }

            // Release the guard.  Before we can split the stream we must drop all
            // other clones of `stream_arc` — the `PeerWriter::Direct` variant (held by
            // both `ps.writer` and the peer-worker closure via `worker_writer`) still
            // references it.  Switching to a temporary dead channel first drops those
            // references atomically through the shared `Arc<Mutex<PeerWriterKind>>`.
            drop(stream);
            if let Ok(m) = self.peer_states.lock() {
                if let Some(ps) = m.get(&key_to) {
                    if let Some(w) = &ps.writer {
                        let (dead_tx, _dead_rx) = mpsc::channel::<Vec<u8>>();
                        let _ = w.switch_to_channel(dead_tx);
                    }
                }
            }

            // Now the only remaining owner is `stream_arc` itself; unwrap and split.
            let inner_stream = Arc::try_unwrap(stream_arc)
                .map_err(|_| "stream_arc still has other owners after clearing Direct variant; cannot split".to_string())?
                .into_inner()
                .map_err(|_| "stream mutex poisoned".to_string())?;
            let (read_half, write_half) = tokio::io::split(inner_stream);

            // Spawn a dedicated writer thread that owns the write half exclusively — no mutex needed.
            let writer_tx = spawn_stream_writer_task_split(
                self.dtls_async_runtime.clone(),
                write_half,
                format!("send-{}", key_to),
            )?;

            // Switch the PeerWriter to the real channel (replacing the temporary dead channel).
            if let Ok(mut m) = self.peer_states.lock() {
                if let Some(ps) = m.get_mut(&key_to) {
                    if ps.generation != owner_generation {
                        tracing::debug!(
                            "[DtlsOpenSsl:::send] skip writer channel install for {} due to generation mismatch (owner={}, current={})",
                            to,
                            owner_generation,
                            ps.generation
                        );
                    } else if let Some(w) = &ps.writer {
                        let _ = w.switch_to_channel(writer_tx.clone());
                    }
                }
            }

            // Also install/update the writer for this peer in the unified peer_states map.
            {
                let peers = &self.peer_states;
                let _ = peers.lock().map(|mut m| {
                    if let Some(ps) = m.get_mut(&key_to) {
                        if ps.generation == owner_generation {
                            tracing::debug!("[DtlsOpenSsl:::send] change is_connecting_peer to false for {} (post-connect update)", to);
                            ps.peer_handle = Some(peer_handle.clone());
                            ps.is_connecting_peer = false;
                            ps.handshake_logged = true;
                        } else {
                            tracing::debug!(
                                "[DtlsOpenSsl:::send] skip post-connect state update for {} due to generation mismatch (owner={}, current={})",
                                to,
                                owner_generation,
                                ps.generation
                            );
                        }
                    }
                });
            }

            // Spawn a background reader loop owning the read half exclusively — no mutex contention.
            {
                let handle_message2 = self.handle_message.clone();
                let peer_cert_handler2 = self.handle_peer_certificate;
                let from2: NetworkEndpoint = endpoint.clone();
                let peers2: PeerStates = self.peer_states.clone();
                std::thread::spawn(move || {
                    run_read_loop_split(
                        read_half,
                        runtime,
                        &from2,
                        peers2.clone(),
                        handle_message2.clone(),
                        peer_cert_handler2,
                        owner_generation,
                        "::send",
                    );
                });
            }
            Ok(())
        }

        fn get_handle_message(&self) -> Option<HandleMessage> { self.handle_message.clone() }
        fn set_handle_message(&mut self, handler: Option<HandleMessage>) { self.handle_message = handler; }
        fn with_handle_message(mut self, handler: HandleMessage) -> Self {
            self.handle_message = Some(handler);
            self
        }

        fn set_handle_new_session(&mut self, handler: Option<HandleNewSession>) {
            self.handle_new_session = handler;
        }

        fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> { self.handle_peer_certificate }
        fn set_handle_peer_certificate(&mut self, handler: Option<HandlePeerCertificate>) { self.handle_peer_certificate = handler; }
        fn with_handle_peer_certificate(mut self, handler: HandlePeerCertificate) -> Self {
            self.handle_peer_certificate = Some(handler);
            self
        }

        fn get_ca_cert(&self) -> Option<&[u8]> { self.ca_cert.as_deref() }
        fn set_ca_cert(&mut self, pem: Option<Vec<u8>>) { self.ca_cert = pem; }
        fn with_ca_cert(mut self, pem: Vec<u8>) -> Self {
            self.ca_cert = Some(pem);
            self
        }

        fn get_client_cert(&self) -> Option<&[u8]> { self.client_cert.as_deref() }
        fn set_client_cert(&mut self, pem: Option<Vec<u8>>) { self.client_cert = pem; }
        fn with_client_cert(mut self, pem: Vec<u8>) -> Self {
            self.client_cert = Some(pem);
            self
        }

        fn get_client_private_key(&self) -> Option<&[u8]> { self.client_private_key.as_deref() }
        fn set_client_private_key(&mut self, pem: Option<Vec<u8>>) { self.client_private_key = pem; }
        fn with_client_private_key(mut self, pem: Vec<u8>) -> Self {
            self.client_private_key = Some(pem);
            self
        }

        fn get_server_signing_cert(&self) -> Option<&[u8]> { self.server_signing_cert.as_deref() }
        fn set_server_signing_cert(&mut self, pem: Option<Vec<u8>>) { self.server_signing_cert = pem; }
        fn with_server_signing_cert(mut self, pem: Vec<u8>) -> Self {
            self.server_signing_cert = Some(pem);
            self
        }

        fn get_server_signing_private_key(&self) -> Option<&[u8]> { self.server_signing_private_key.as_deref() }
        fn set_server_signing_private_key(&mut self, pem: Option<Vec<u8>>) { self.server_signing_private_key = pem; }
        fn with_server_signing_private_key(mut self, pem: Vec<u8>) -> Self {
            self.server_signing_private_key = Some(pem);
            self
        }

        // Toggle application-layer-only verification mode
        fn set_app_layer_only_verification(&mut self, enabled: bool) {
            if self.dangerous_debug {
                self.app_layer_only_verification = enabled;
            } else if enabled {
                tracing::error!("[DtlsOpenSsl] Attempted to enable app-layer-only verification without dangerous_debug; ignoring");
            }
        }

        fn with_app_layer_only_verification(mut self, enabled: bool) -> Self {
            if self.dangerous_debug {
                self.app_layer_only_verification = enabled;
            } else if enabled {
                tracing::error!("[DtlsOpenSsl] Attempted to enable app-layer-only verification without dangerous_debug; ignoring");
            }
            self
        }

        fn set_dangerous_debug(&mut self, enabled: bool) {
            if enabled {
                tracing::error!("[DtlsOpenSsl] DANGEROUS DEBUG MODE ENABLED - SECURITY IS COMPROMISED");
            }
            self.dangerous_debug = enabled;
        }

        fn with_dangerous_debug(mut self, enabled: bool) -> Self {
            if enabled {
                tracing::error!("[DtlsOpenSsl] DANGEROUS DEBUG MODE ENABLED - SECURITY IS COMPROMISED");
            }
            self.dangerous_debug = enabled;
            self
        }

        fn set_null_encryption(&mut self, enabled: bool) {
            if self.dangerous_debug {
                self.null_encryption = enabled;
            } else if enabled {
                tracing::error!("[DtlsOpenSsl] Attempted to enable null encryption without dangerous_debug; ignoring");
            }
        }

        fn with_null_encryption(mut self, enabled: bool) -> Self {
            if self.dangerous_debug {
                self.null_encryption = enabled;
            } else if enabled {
                tracing::error!("[DtlsOpenSsl] Attempted to enable null encryption without dangerous_debug; ignoring");
            }
            self
        }
    }
}



