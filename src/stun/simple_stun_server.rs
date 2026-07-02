use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};

/// A very small STUN server used for integration tests.
///
/// Features:
/// - Listens on a UDP port and responds to STUN Binding Requests (type 0x0001)
/// - Replies with a Binding Success (type 0x0101) containing XOR-MAPPED-ADDRESS (IPv4)
/// - Optional broken_nat mode: obfuscate the reported address per requester deterministically
/// - Optional `attach_to`: if set, use this address's IP in the reported mapping instead of the observed src IP
///
/// Notes:
/// - This is intentionally minimal and only supports IPv4 XOR-MAPPED-ADDRESS.
/// - It is designed strictly for tests; do not use in production.
pub struct SimpleStunServer {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Clone, Debug)]
pub struct StartOptions {
    pub bind_addr: SocketAddr,
    pub attach_to: Option<SocketAddr>,
    pub broken_nat: bool,
}

impl Default for StartOptions {
    fn default() -> Self {
        Self { bind_addr: "127.0.0.1:3478".parse().unwrap(), attach_to: None, broken_nat: false }
    }
}

impl SimpleStunServer {
    pub fn start(opts: StartOptions) -> std::io::Result<Self> {
        let sock = UdpSocket::bind(opts.bind_addr)?;
        sock.set_nonblocking(true)?;
        let running = Arc::new(AtomicBool::new(true));
        let running_thread = running.clone();

        // Per-requestor obfuscation map (stable within process lifetime)
        let map: Arc<Mutex<HashMap<SocketAddr, (u16, [u8;4])>>> = Arc::new(Mutex::new(HashMap::new()));
        let map_thread = map.clone();

        let handle = thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while running_thread.load(Ordering::SeqCst) {
                match sock.recv_from(&mut buf) {
                    Ok((n, from)) => {
                        if n < 20 { continue; }
                        let data = &buf[..n];
                        // Verify Binding Request with magic cookie
                        if data[0] == 0x00 && data[1] == 0x01 && data[4..8] == [0x21, 0x12, 0xA4, 0x42] {
                            // Build a Binding Success Response echoing the transaction id
                            let mut resp: Vec<u8> = Vec::with_capacity(32);
                            // Type 0x0101, length to be filled later
                            resp.extend_from_slice(&[0x01, 0x01, 0x00, 0x00]);
                            // Magic cookie
                            resp.extend_from_slice(&[0x21, 0x12, 0xA4, 0x42]);
                            // Transaction ID (12 bytes)
                            resp.extend_from_slice(&data[8..20]);

                            // Compute reported address
                            let (ip_bytes, port): ([u8;4], u16) = {
                                // Source as seen by this socket
                                let mut ip = match from.ip() {
                                    std::net::IpAddr::V4(v4) => v4.octets(),
                                    _ => [127,0,0,1], // limit to IPv4 in tests
                                };
                                let mut port = from.port();
                                // If attach_to set, override IP (keep port unless broken_nat wants to change)
                                if let Some(att) = opts.attach_to
                                    && let std::net::IpAddr::V4(v4) = att.ip() { ip = v4.octets(); }
                                if opts.broken_nat {
                                    // Derive a deterministic but different mapping per requester
                                    // Simple hash: xor octets and ports into a small seed
                                    let seed = (from.port() as u32)
                                        ^ ((from.ip().to_string().bytes().fold(0u32, |a, b| a.wrapping_add(b as u32))) << 1);
                                    // Perturb port and ip slightly but keep them in a sensible range
                                    port = 1024 + (((port as u32 ^ seed) % 50000) as u16);
                                    ip[3] = 1 + (((ip[3] as u32 ^ seed) % 253) as u8); // 1..253
                                    // Remember per-requestor so it's stable during the server lifetime
                                    if let Ok(mut m) = map_thread.lock() {
                                        m.entry(from).or_insert((port, ip));
                                    }
                                } else if let Ok(m) = map_thread.lock()
                                    && let Some((p, i)) = m.get(&from) { port = *p; ip = *i; }
                                (ip, port)
                            };

                            // XOR-MAPPED-ADDRESS attribute (type 0x0020)
                            let atype: [u8;2] = [0x00, 0x20];
                            let alen: [u8;2] = [0x00, 0x08];
                            let family: [u8;2] = [0x00, 0x01]; // IPv4
                            let xport = ((port ^ 0x2112)).to_be_bytes();
                            let mut xip = ip_bytes;
                            let cookie = [0x21u8, 0x12, 0xA4, 0x42];
                            for i in 0..4 { xip[i] ^= cookie[i]; }

                            resp.extend_from_slice(&atype);
                            resp.extend_from_slice(&alen);
                            resp.extend_from_slice(&family);
                            resp.extend_from_slice(&xport);
                            resp.extend_from_slice(&xip);

                            // Fix length field
                            let msg_len = (resp.len() - 20) as u16;
                            let len_bytes = msg_len.to_be_bytes();
                            resp[2] = len_bytes[0]; resp[3] = len_bytes[1];

                            let _ = sock.send_to(&resp, from);
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut {
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

        Ok(Self { running, thread: Some(handle) })
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
