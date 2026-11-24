use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

/// Per-requestor obfuscated (port, address) pairs, shared with the server thread.
type ObfuscationMap = Arc<Mutex<HashMap<SocketAddr, (u16, IpAddr)>>>;

/// A very small STUN server used for integration tests.
///
/// Features:
/// - Listens on a UDP port and responds to STUN Binding Requests (type 0x0001)
/// - Replies with a Binding Success (type 0x0101) containing XOR-MAPPED-ADDRESS in the family of
///   the request's source: 0x01 for IPv4, 0x02 for IPv6 (issue #495)
/// - Optional broken_nat mode: obfuscate the reported address per requester deterministically
/// - Optional `attach_to`: if set, use this address's IP in the reported mapping instead of the observed src IP
///
/// Notes:
/// - This is intentionally minimal: XOR-MAPPED-ADDRESS only, no other attributes.
/// - It is designed strictly for tests; do not use in production.
pub struct SimpleStunServer {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    local_addr: SocketAddr,
}

#[derive(Clone, Debug)]
pub struct StartOptions {
    pub bind_addr: SocketAddr,
    pub attach_to: Option<SocketAddr>,
    pub broken_nat: bool,
}

impl Default for StartOptions {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3478".parse().unwrap(),
            attach_to: None,
            broken_nat: false,
        }
    }
}

impl SimpleStunServer {
    pub fn start(opts: StartOptions) -> std::io::Result<Self> {
        let sock = UdpSocket::bind(opts.bind_addr)?;
        // Capture the actual bound address before the socket is moved into the worker thread.
        // This lets callers bind to port 0 (OS-assigned) and read the real address back, avoiding
        // the allocate-a-port-then-bind-it TOCTOU.
        let local_addr = sock.local_addr()?;
        sock.set_nonblocking(true)?;
        let running = Arc::new(AtomicBool::new(true));
        let running_thread = running.clone();

        // Per-requestor obfuscation map (stable within process lifetime)
        let map: ObfuscationMap = Arc::new(Mutex::new(HashMap::new()));
        let map_thread = map.clone();

        let handle = thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while running_thread.load(Ordering::SeqCst) {
                match sock.recv_from(&mut buf) {
                    Ok((n, from)) => {
                        if n < 20 {
                            continue;
                        }
                        let data = &buf[..n];
                        // Verify Binding Request with magic cookie
                        if data[0] == 0x00
                            && data[1] == 0x01
                            && data[4..8] == [0x21, 0x12, 0xA4, 0x42]
                        {
                            // Build a Binding Success Response echoing the transaction id
                            let mut resp: Vec<u8> = Vec::with_capacity(32);
                            // Type 0x0101, length to be filled later
                            resp.extend_from_slice(&[0x01, 0x01, 0x00, 0x00]);
                            // Magic cookie
                            resp.extend_from_slice(&[0x21, 0x12, 0xA4, 0x42]);
                            // Transaction ID (12 bytes)
                            resp.extend_from_slice(&data[8..20]);

                            // Compute reported address: the source as seen by this socket, with an
                            // IPv4-mapped source (from a dual-stack socket) reported as plain IPv4.
                            let source = SocketAddr::new(from.ip().to_canonical(), from.port());
                            let (ip, port): (IpAddr, u16) = {
                                let mut ip = source.ip();
                                let mut port = source.port();
                                // If attach_to is set in the same family, report its IP instead.
                                if let Some(att) = opts.attach_to
                                    && att.is_ipv4() == ip.is_ipv4()
                                {
                                    ip = att.ip();
                                }
                                if opts.broken_nat {
                                    // Derive a deterministic but different mapping per requester
                                    let seed = (from.port() as u32)
                                        ^ ((from
                                            .ip()
                                            .to_string()
                                            .bytes()
                                            .fold(0u32, |a, b| a.wrapping_add(b as u32)))
                                            << 1);
                                    // Perturb port and the last address byte, in a sensible range
                                    port = 1024 + (((port as u32 ^ seed) % 50000) as u16);
                                    ip = match ip {
                                        IpAddr::V4(v4) => {
                                            let mut o = v4.octets();
                                            o[3] = 1 + (((o[3] as u32 ^ seed) % 253) as u8); // 1..253
                                            IpAddr::from(o)
                                        }
                                        IpAddr::V6(v6) => {
                                            let mut o = v6.octets();
                                            o[15] = 1 + (((o[15] as u32 ^ seed) % 253) as u8);
                                            IpAddr::from(o)
                                        }
                                    };
                                    // Remember per-requestor so it's stable during the server lifetime
                                    if let Ok(mut m) = map_thread.lock() {
                                        m.entry(from).or_insert((port, ip));
                                    }
                                } else if let Ok(m) = map_thread.lock()
                                    && let Some((p, i)) = m.get(&from)
                                {
                                    port = *p;
                                    ip = *i;
                                }
                                (ip, port)
                            };
                            resp.extend_from_slice(&xor_mapped_address(ip, port, &data[8..20]));

                            // Fix length field
                            let msg_len = (resp.len() - 20) as u16;
                            let len_bytes = msg_len.to_be_bytes();
                            resp[2] = len_bytes[0];
                            resp[3] = len_bytes[1];

                            let _ = sock.send_to(&resp, from);
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut
                        {
                            thread::sleep(std::time::Duration::from_millis(5));
                            continue;
                        } else {
                            // Stop on hard errors
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self {
            running,
            thread: Some(handle),
            local_addr,
        })
    }

    /// The actual address the server is bound to (useful when started on port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

impl Drop for SimpleStunServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Encode an XOR-MAPPED-ADDRESS attribute (RFC 8489 §14.2) for `ip`:`port`: family 0x01 with the
/// address masked by the magic cookie, or family 0x02 masked by the cookie then `transaction_id`.
fn xor_mapped_address(ip: IpAddr, port: u16, transaction_id: &[u8]) -> Vec<u8> {
    let cookie = crate::stun::stun_endpoint_finder_impl::STUN_MAGIC_COOKIE;
    let (family, mut addr): (u8, Vec<u8>) = match ip {
        IpAddr::V4(v4) => (0x01, v4.octets().to_vec()),
        IpAddr::V6(v6) => (0x02, v6.octets().to_vec()),
    };
    for (b, m) in addr.iter_mut().zip(cookie.iter().chain(transaction_id)) {
        *b ^= *m;
    }
    let value_len = 4 + addr.len() as u16;
    let mut out = Vec::with_capacity(4 + value_len as usize);
    out.extend_from_slice(&0x0020u16.to_be_bytes());
    out.extend_from_slice(&value_len.to_be_bytes());
    out.extend_from_slice(&[0x00, family]);
    out.extend_from_slice(&(port ^ 0x2112).to_be_bytes());
    out.extend_from_slice(&addr);
    out
}
