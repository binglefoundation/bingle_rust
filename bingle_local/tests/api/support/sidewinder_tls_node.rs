//! A tiny in-process mutual-TLS Sidewinder node for the Mailbox failover tests (#497).
//!
//! Presents an identity certificate bound to a node key, requires a client certificate from one of the
//! authorized identities, and answers just enough of the client API for a Mailbox `post` to finalise:
//! `GET /v2/transactions/params`, `POST /v2/transactions` and `GET /v2/transactions/pending/{id}`
//! (always `final`). It runs a blocking `rustls::ServerConnection` on a std thread, one request per
//! connection, after the sidewinder_ops test node of the same shape. It binds IPv4 or IPv6 loopback,
//! so a test can give a node a live `[::1]` address next to a dead IPv4 one.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use ed25519_dalek::SigningKey;
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use sidewinder_ops::{DiscoveredNode, EndpointRecord};
use sw_identity_tls::server::{IdentityClientVerifier, server_config};
use sw_identity_tls::{StaticMembership, default_provider, generate};

/// The suggested params every node serves: a 32-byte instance id (base64 of zeros), a round, a fee and
/// a validity window.
const PARAMS_JSON: &str = r#"{"instanceId":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","lastRound":42,"minFee":1000,"maxValidityWindow":1000}"#;
/// The transaction id a submit returns, and the one every pending read reports as final.
const TX_ID: &str = "MOCKTX";

/// A loopback mutual-TLS Sidewinder node on an ephemeral port; shuts down on drop.
pub struct TlsSidewinderNode {
    addr: SocketAddr,
    identity: String,
    shutdown: Arc<AtomicBool>,
    connections: Arc<AtomicUsize>,
    submits: Arc<AtomicUsize>,
    handle: Option<JoinHandle<()>>,
}

impl TlsSidewinderNode {
    /// Start a node on IPv4 loopback with `node_key`'s identity, accepting `authorized_clients`.
    pub fn start_v4(node_key: &SigningKey, authorized_clients: Vec<String>) -> Self {
        Self::start_on("127.0.0.1:0", node_key, authorized_clients)
    }

    /// Start a node on IPv6 loopback (`[::1]`). Panics if the host has no IPv6 loopback.
    pub fn start_v6(node_key: &SigningKey, authorized_clients: Vec<String>) -> Self {
        Self::start_on("[::1]:0", node_key, authorized_clients)
    }

    fn start_on(bind: &str, node_key: &SigningKey, authorized_clients: Vec<String>) -> Self {
        let cert = generate(node_key).expect("mint node identity certificate");
        let identity = cert.address.clone();
        let authority = Arc::new(StaticMembership::from_sets(Vec::new(), authorized_clients));
        let verifier = Arc::new(IdentityClientVerifier::new(authority, default_provider()));
        let config: Arc<ServerConfig> =
            Arc::new(server_config(cert, verifier).expect("build node server config"));

        let listener = TcpListener::bind(bind).expect("bind tls sidewinder node");
        listener
            .set_nonblocking(true)
            .expect("set tls sidewinder node nonblocking");
        let addr = listener
            .local_addr()
            .expect("tls sidewinder node local addr");
        let shutdown = Arc::new(AtomicBool::new(false));
        let connections = Arc::new(AtomicUsize::new(0));
        let submits = Arc::new(AtomicUsize::new(0));

        let thread_shutdown = Arc::clone(&shutdown);
        let thread_connections = Arc::clone(&connections);
        let thread_submits = Arc::clone(&submits);
        let handle = std::thread::spawn(move || {
            while !thread_shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        thread_connections.fetch_add(1, Ordering::SeqCst);
                        serve_connection(stream, config.clone(), &thread_submits);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            identity,
            shutdown,
            connections,
            submits,
            handle: Some(handle),
        }
    }

    /// The bound socket address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The Algorand identity the node's certificate is bound to (what a client pins).
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// How many TCP connections the node has accepted.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// How many transactions were submitted to the node.
    pub fn submits(&self) -> usize {
        self.submits.load(Ordering::SeqCst)
    }
}

impl Drop for TlsSidewinderNode {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Complete one mutual-TLS handshake and answer one request. A failed handshake (an unauthorized or
/// absent client certificate) or a read error just drops the connection.
fn serve_connection(tcp: TcpStream, config: Arc<ServerConfig>, submits: &AtomicUsize) {
    tcp.set_nonblocking(false).ok();
    tcp.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let Ok(conn) = ServerConnection::new(config) else {
        return;
    };
    let mut tls = StreamOwned::new(conn, tcp);
    let mut reader = BufReader::new(&mut tls);

    // the handshake is driven by the first read.
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) => break,
            Ok(_) => {
                let header = header.trim_end_matches(['\r', '\n']);
                if header.is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse().unwrap_or(0);
                }
            }
            Err(_) => return,
        }
    }
    let mut body = vec![0u8; content_length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");
    let (status, response_body) = if method == "GET" && path == "/v2/transactions/params" {
        ("200 OK", PARAMS_JSON.to_string())
    } else if method == "POST" && path == "/v2/transactions" {
        submits.fetch_add(1, Ordering::SeqCst);
        ("200 OK", format!(r#"{{"txId":"{TX_ID}"}}"#))
    } else if method == "GET" && path.starts_with("/v2/transactions/pending/") {
        ("200 OK", format!(r#"{{"txId":"{TX_ID}","stage":"final"}}"#))
    } else {
        ("404 Not Found", r#"{"message":"not found"}"#.to_string())
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
        response_body.len()
    );
    let _ = tls.write_all(response.as_bytes());
    let _ = tls.flush();
}

/// A loopback address on `ip` with nothing listening, so a connection to it is refused at once.
pub fn dead_addr(ip: &str) -> SocketAddr {
    let listener = TcpListener::bind((ip, 0)).expect("bind a throwaway port");
    let addr = listener.local_addr().expect("throwaway local addr");
    drop(listener);
    addr
}

/// A discovered node bound to `identity` that advertises `v4` and/or `v6`.
pub fn node_record(
    identity: &str,
    v4: Option<SocketAddr>,
    v6: Option<SocketAddr>,
) -> DiscoveredNode {
    let v4 = v4.map(|addr| match addr {
        SocketAddr::V4(v4) => v4,
        SocketAddr::V6(_) => panic!("expected an IPv4 address, got {addr}"),
    });
    let v6 = v6.map(|addr| match addr {
        SocketAddr::V6(v6) => v6,
        SocketAddr::V4(_) => panic!("expected an IPv6 address, got {addr}"),
    });
    DiscoveredNode {
        identity: identity.to_string(),
        endpoint: EndpointRecord::new(v4, v6).expect("endpoint record"),
    }
}

/// The identity bound to `key`, for a node that is not started (a record whose addresses are dead).
pub fn identity_of(key: &SigningKey) -> String {
    generate(key).expect("mint identity certificate").address
}
