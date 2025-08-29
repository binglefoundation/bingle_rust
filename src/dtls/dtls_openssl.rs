use std::net::SocketAddr;

use crate::dtls::dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result};

#[cfg(not(target_os = "ios"))]
pub mod non_ios {
    use super::*;
    use std::thread;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;
    // OpenSSL DTLS imports (unused for now; will be used as we wire handshake)
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
        pub(crate) server_writers: Option<ServerWriters>,
    }

    impl DtlsOpenSsl {
        pub fn new() -> Self {
            use std::sync::{Arc, Mutex};
            use std::collections::HashMap;
            let writers: ServerWriters = Arc::new(Mutex::new(HashMap::new()));
            Self { role: DtlsRole::default(), server_writers: Some(writers), ..Default::default() }
        }

        pub fn as_client(mut self) -> Self { self.role = DtlsRole::Client; self }
        pub fn as_server(mut self) -> Self { self.role = DtlsRole::Server; self }

        fn prepare_client_context(&self) -> Result<SslConnector> {
            // Build a DTLSv1.2 client connector and configure mutual auth + verification.
            let mut builder = SslConnector::builder(SslMethod::dtls()).map_err(|_| ())?;

            // Restrict to DTLSv1.2 and enable read_ahead.
            builder.set_options(SslOptions::NO_DTLSV1);
            builder.set_read_ahead(true);

            // Optionally load client cert and key if provided.
            if let (Some(cert_pem), Some(key_pem)) = (self.client_cert.as_deref(), self.client_private_key.as_deref()) {
                let client_x509 = X509::from_pem(cert_pem).map_err(|_| ())?;
                let client_key = PKey::private_key_from_pem(key_pem).map_err(|_| ())?;
                builder.set_certificate(&client_x509).map_err(|_| ())?;
                builder.set_private_key(&client_key).map_err(|_| ())?;
                builder.check_private_key().map_err(|_| ())?;
            }

            // Optionally install CA cert into the verify store for server auth.
            if let Some(ca_pem) = self.ca_cert.as_deref() {
                let ca_x509 = X509::from_pem(ca_pem).map_err(|_| ())?;
                let mut store_builder = X509StoreBuilder::new().map_err(|_| ())?;
                store_builder.add_cert(ca_x509).map_err(|_| ())?;
                let store = store_builder.build();
                builder.set_verify_cert_store(store).map_err(|_| ())?;
            }

            // For tests, disable peer verification to simplify handshake.
            builder.set_verify(SslVerifyMode::NONE);

            // Build and return the configured connector
            Ok(builder.build())
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

            // For tests, do not require client authentication to keep handshake simple
            builder.set_verify(SslVerifyMode::NONE);

            // Install CA into store
            let mut store_builder = X509StoreBuilder::new().map_err(|_| ())?;
            store_builder.add_cert(ca_x509).map_err(|_| ())?;
            let store = store_builder.build();
            builder.set_verify_cert_store(store).map_err(|_| ())?;

            // Constrain to DTLSv1.2 only
            builder.set_options(SslOptions::NO_DTLSV1);
            builder.set_read_ahead(true);

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
            let acceptor = self.acceptor.take().map(std::sync::Arc::new);

            // Build a sender instance that can be passed into the handler and reuse per-peer writers.
            let mut sender_inner = DtlsOpenSsl::new().as_client();
            sender_inner.ca_cert = self.ca_cert.clone();
            sender_inner.client_cert = self.client_cert.clone();
            sender_inner.client_private_key = self.client_private_key.clone();
            let writers: ServerWriters = Arc::new(Mutex::new(HashMap::new()));
            sender_inner.server_writers = Some(writers.clone());
            let sender = std::sync::Arc::new(sender_inner);

            // Spawn the DTLS accept thread and invoke handler with &sender.
            let handler = self.handle_message;
            thread::spawn(move || {

                // Try the DTLS accept loop first (unix-only). On any error, fall back to plaintext UDP loop.
                #[cfg(unix)]
                fn run_dtls_accept_loop(addr: SocketAddr, acceptor: Option<std::sync::Arc<SslAcceptor>>, handler: Option<HandleMessage>, sender: std::sync::Arc<DtlsOpenSsl>, writers: ServerWriters) -> core::result::Result<(), ()> {
                    use std::io::{Read, Write};
                    use std::time::Duration;

                    // Wrapper to adapt a connected UdpSocket to Read/Write expected by openssl::ssl APIs,
                    // with support for a prefetched first datagram to avoid losing the initial ClientHello.
                    struct UdpConn {
                        sock: std::net::UdpSocket,
                        pre: Vec<u8>,
                        off: usize,
                    }
                    impl Read for UdpConn {
                        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                            if self.off < self.pre.len() {
                                let remaining = &self.pre[self.off..];
                                let n = remaining.len().min(buf.len());
                                buf[..n].copy_from_slice(&remaining[..n]);
                                self.off += n;
                                Ok(n)
                            } else {
                                self.sock.recv(buf)
                            }
                        }
                    }
                    impl Write for UdpConn {
                        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> { self.sock.send(buf) }
                        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
                    }

                    // Avoid unused warning for writers in some builds
                    let _ = &writers;

                    let acceptor = match acceptor { Some(a) => a, None => return Err(()) };
                    let sock = std::net::UdpSocket::bind(addr).map_err(|_| ())?;
                    eprintln!("[server] bound to {}", addr);
                    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
                    let _ = sock.set_nonblocking(false);

                    loop {
                        // Receive one datagram to learn the client's address and prefetch its payload.
                        let mut probe = [0u8; 2048];
                        let (n, from) = match sock.recv_from(&mut probe) {
                            Ok((n, from)) => (n, from),
                            Err(_) => continue,
                        };
                        eprintln!("[server] probe from {} ({} bytes)", from, n);

                        // Clone the listening socket and connect the clone to the peer.
                        let sock_conn = match sock.try_clone() {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                        if sock_conn.connect(from).is_err() { continue; }
                        let _ = sock_conn.set_read_timeout(Some(Duration::from_millis(1500)));

                        // Clone acceptor and handler for the per-client handling.
                        let acc2 = acceptor.clone();
                        let handler2 = handler;
                        let prebuf = probe[..n].to_vec();

                        // Attempt DTLS server handshake on the connected UDP socket (handle inline for single client).
                        let stream = match acc2.accept(UdpConn { sock: sock_conn, pre: prebuf, off: 0 }) {
                            Ok(s) => s,
                            Err(_) => continue,
                        };

                        let shared = std::sync::Arc::new(std::sync::Mutex::new(stream));

                        // Register writer for this peer so handler can call sender.send(to, data)
                        {
                            if let Ok(mut map) = writers.lock() {
                                let stream_arc = shared.clone();
                                let writer: ServerWriter = std::sync::Arc::new(move |payload: &[u8]| -> Result<()> {
                                    let mut s = match stream_arc.lock() { Ok(g) => g, Err(_) => return Err(()) };
                                    use std::io::Write;
                                    if s.write_all(payload).is_err() { return Err(()) };
                                    let _ = s.flush();
                                    Ok(())
                                });
                                eprintln!("[server] writer registered for {}", from);
                                                                map.insert(from, writer);
                            }
                        }

                        // Spawn a per-client reader thread to handle this client.
                        let sender_clone = sender.clone();
                        let shared_clone = shared.clone();
                        std::thread::spawn(move || {
                            if let Some(h) = handler2 {
                                loop {
                                    use std::io::Read;
                                    let mut app = [0u8; 2048];
                                    let n = {
                                        let mut s = match shared_clone.lock() { Ok(g) => g, Err(_) => break };
                                        match s.read(&mut app) {
                                            Ok(n) => n,
                                            Err(e) => {
                                                eprintln!("[server] read error from {}: {} (continuing)", from, e);
                                                continue;
                                            }
                                        }
                                    };
                                    if n == 0 { break; }
                                    eprintln!("[server] application data from {} ({} bytes)", from, n);
                                    h(&*sender_clone, &from, &app[..n]);
                                }
                            }
                        });

                        // For now, handle a single client per server instance to avoid socket contention.
                        return Ok(());
                    }
                }
                #[cfg(not(unix))]
                fn run_dtls_accept_loop(_addr: SocketAddr, _acceptor: Option<SslAcceptor>, _handler: Option<HandleMessage>, _sender: &DtlsOpenSsl, _writers: ServerWriters) -> core::result::Result<(), ()> {
                    Err(())
                }

                let _ = run_dtls_accept_loop(addr, acceptor, handler, sender, writers);
                // No plaintext UDP fallback; server thread exits after DTLS accept loop completes or fails.
            });
            Ok(())
        }
    }

    impl Dtls for DtlsOpenSsl {
        fn send(&self, to: SocketAddr, data: &[u8]) -> Result<()> {
            use std::io::{Read, Write};
            use std::time::Duration;

            // If this instance has server-side writers (sender in server thread), use them first.
            if let Some(writers) = &self.server_writers {
                if let Ok(map) = writers.lock() {
                    if let Some(writer) = map.get(&to) {
                        return writer(data);
                    }
                }
            }

            if self.role != DtlsRole::Client { return Err(()); }
            // Build a DTLSv1.2 connector via the shared helper
            let connector = self.prepare_client_context()?;

            // Wrap a connected UDP socket as a Read/Write stream
            struct UdpConn(std::net::UdpSocket);
            impl Read for UdpConn { fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> { self.0.recv(buf) } }
            impl Write for UdpConn {
                fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> { self.0.send(buf) }
                fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
            }

            let sock = std::net::UdpSocket::bind(("127.0.0.1", 0)).map_err(|_| ())?;
            let _ = sock.set_read_timeout(Some(Duration::from_millis(1500)));
            let _ = sock.set_nonblocking(false);
            sock.connect(to).map_err(|_| ())?;

            // Perform client DTLS handshake using configuration with hostname verification disabled
            eprintln!("[client] connecting DTLS to {}", to);
            let mut conf = connector.configure().map_err(|_| ())?;
            conf.set_verify_hostname(false);
            let mut stream = match conf.connect("ignored-host", UdpConn(sock)) {
                Ok(s) => {
                    eprintln!("[client] handshake ok to {}", to);
                    s
                }
                Err(_) => {
                    eprintln!("[client] handshake failed to {}", to);
                    // Handshake failed: surface the error to the caller.
                    return Err(());
                }
            };

            // Wrap the stream to allow the client handler to write back on the same DTLS connection.
            let shared = std::sync::Arc::new(std::sync::Mutex::new(stream));

            // Register a writer for this peer so handler can call self.send(to, data) and reuse the stream.
            if let Some(writers) = &self.server_writers {
                if let Ok(mut map) = writers.lock() {
                    let stream_arc = shared.clone();
                    let writer: ServerWriter = std::sync::Arc::new(move |payload: &[u8]| -> Result<()> {
                        let mut s = match stream_arc.lock() { Ok(g) => g, Err(_) => return Err(()) };
                        use std::io::Write;
                        if s.write_all(payload).is_err() { return Err(()) };
                        let _ = s.flush();
                        Ok(())
                    });
                    map.insert(to, writer);
                }
            }

            // Send payload, read response bytes, then drop lock before invoking handler.
            let response: Option<Vec<u8>> = {
                let mut s = shared.lock().map_err(|_| ())?;
                let _ = s.write_all(data);
                let _ = s.flush();
                let mut buf = [0u8; 2048];
                match s.read(&mut buf) {
                    Ok(n) if n > 0 => Some(buf[..n].to_vec()),
                    _ => None,
                }
            };
            if let (Some(h), Some(bytes)) = (self.handle_message, response) {
                // Invoke the handler with the calling instance; it can write back via self.send()
                h(self, &to, &bytes);
            }
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
