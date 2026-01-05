use std::net::{SocketAddr, UdpSocket, ToSocketAddrs};
use std::any::Any;
use std::collections::VecDeque;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::network_mux_trait::{HandleDtls, HandleStun, HandleTurn, NetworkMux, Result};
use log::warn;

/// Mux classification types translated from the provided Kotlin function
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuxType {
    Stun,
    Zrtp,
    Dtls,
    TurnChannelData,
    Rtp,
    Unknown,
}

/// Determine the MuxType for the given data buffer.
pub fn mux_type_for(data: &[u8]) -> MuxType {
    if data.is_empty() {
        return MuxType::Unknown;
    }
    let first = data[0];
    // Note: range checks mirroring the Kotlin logic
    if (0..=3).contains(&first) {
        return MuxType::Stun;
    }
    if (16..=19).contains(&first) {
        return MuxType::Zrtp;
    }
    if (20..=63).contains(&first) {
        return MuxType::Dtls;
    }
    if (0x40..=0x7f).contains(&first) {
        return MuxType::TurnChannelData;
    }
    if (128..=191).contains(&first) {
        return MuxType::Rtp;
    }
    MuxType::Unknown
}

/// UDP-based NetworkMux implementation
pub struct UdpNetworkMux {
    socket: UdpSocket,
    handle_dtls: Mutex<Option<HandleDtls>>,
    handle_stun: Mutex<Option<HandleStun>>,
    handle_turn: Mutex<Option<HandleTurn>>,
    running: AtomicBool,
    rx_thread: Mutex<Option<JoinHandle<()>>>,
    dtls_queue: Mutex<VecDeque<(SocketAddr, Vec<u8>)>>,
}

impl UdpNetworkMux {
    /// Bind a UDP socket on the given local address
    pub fn bind<A: ToSocketAddrs + std::fmt::Debug>(addr: A) -> std::io::Result<Self> {
        // Ensure immediate printing for tests/envs where buffering would hide logs
        #[allow(unused)]
        {
            crate::util::printing::enable_immediate_prints();
        }
        warn!("[UdpNetworkMux] bind {:?}", addr);
        let socket = UdpSocket::bind(&addr).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("udp bind to {:?} failed: {}", addr, e)))?;
        // Set a modest read timeout to allow responsive shutdown of the receive loop
        socket.set_read_timeout(Some(Duration::from_millis(200)))?;
        Ok(Self {
            socket,
            handle_dtls: std::sync::Mutex::new(None),
            handle_stun: std::sync::Mutex::new(None),
            handle_turn: std::sync::Mutex::new(None),
            running: AtomicBool::new(false),
            rx_thread: Mutex::new(None),
            dtls_queue: Mutex::new(VecDeque::new()),
        })
    }

    /// Get the local socket address this mux is bound to
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.socket.local_addr()
    }

    /// Get only the bound IP address (for debug printing)
    pub fn bound_ip(&self) -> std::io::Result<std::net::IpAddr> {
        self.socket.local_addr().map(|a| a.ip())
    }

    /// Set read timeout on the underlying socket
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> std::io::Result<()> {
        self.socket.set_read_timeout(dur)
    }

    /// Peek the next DTLS datagram from the internal queue without removing it.
    pub fn dtls_peek_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        use std::io::{Error, ErrorKind};
        let q = self.dtls_queue.lock().map_err(|e| Error::new(ErrorKind::Other, format!("queue poisoned: {}", e)))?;
        if let Some((from, data)) = q.front() {
            let n = data.len().min(buf.len());
            buf[..n].copy_from_slice(&data[..n]);
            Ok((n, *from))
        } else {
            Err(Error::from(ErrorKind::WouldBlock))
        }
    }

    /// Pop the next DTLS datagram from the internal queue.
    pub fn dtls_recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        use std::io::{Error, ErrorKind};
        let mut q = self.dtls_queue.lock().map_err(|e| Error::new(ErrorKind::Other, format!("queue poisoned: {}", e)))?;
        if let Some((from, data)) = q.pop_front() {
            let n = data.len().min(buf.len());
            buf[..n].copy_from_slice(&data[..n]);
            Ok((n, from))
        } else {
            Err(Error::from(ErrorKind::WouldBlock))
        }
    }

    /// Pop the next DTLS datagram for a specific peer from the internal queue.
    pub fn dtls_recv_from_peer(&self, peer: SocketAddr, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::io::{Error, ErrorKind};
        let mut q = self.dtls_queue.lock().map_err(|e| Error::new(ErrorKind::Other, format!("queue poisoned: {}", e)))?;
        if let Some(idx) = q.iter().position(|(from, _)| *from == peer) {
            if let Some((_, data)) = q.remove(idx) {
                let n = data.len().min(buf.len());
                buf[..n].copy_from_slice(&data[..n]);
                return Ok(n);
            }
        }
        Err(Error::from(ErrorKind::WouldBlock))
    }

    /// Start the receive loop in a background thread.
    /// Note: call this on an Arc to allow handlers to receive `&dyn NetworkMux` reference.
    pub fn start(self: &Arc<Self>) -> std::io::Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            // already running
            return Ok(());
        }
        let socket = self.socket.try_clone()?;
        let this = Arc::clone(self);
        let to = self.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut buf = [0u8; 2048];

            warn!("[UdpNetworkMux][receive][loop on {:?}] starts", to);
            #[allow(unused)] {  }

            while this.running.load(Ordering::SeqCst) {
                match socket.recv_from(&mut buf) {
                    Ok((n, from)) => {
                        if n == 0 { continue; }
                        let data = &buf[..n];
                        this.process_packet(from, data);
                    }
                    Err(e) => {
                        // Respect timeout for shutdown; ignore WouldBlock/TimedOut, break on other errors
                        if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut {
                            continue;
                        } else {
                            // If socket error, stop running
                            warn!("[UdpNetworkMux][receive][loop on {:?}] error {:?}", to, e);
                            #[allow(unused)] {  }

                            this.running.store(false, Ordering::SeqCst);
                        }
                    }
                }
            }
            warn!("[UdpNetworkMux][receive][loop on {:?}] done", to);
            #[allow(unused)] {  }

        });
        let mut slot = self.rx_thread.lock().unwrap();
        *slot = Some(handle);
        Ok(())
    }

    /// Stop the receive loop and join the background thread.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        // poke the socket by setting a very short timeout so the thread wakes up soon
        let _ = self.socket.set_read_timeout(Some(Duration::from_millis(10)));
        if let Ok(mut slot) = self.rx_thread.lock() {
            if let Some(handle) = slot.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for UdpNetworkMux {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        // We cannot join here because the JoinHandle is owned behind a Mutex<Option<..>> and
        // may already be taken or we're in drop; rely on stop() in normal flows.
    }
}

impl NetworkMux for UdpNetworkMux {
    fn write(&self, to: &crate::api::bingle_api::NetworkSourceKey, buf: &[u8]) -> Result<()> {
        // Extract destination inet address; panic if missing per design for future TURN support
        let to_addr = to.inet_socket_address.expect("UdpNetworkMux::write: NetworkSourceKey missing inet_socket_address");
        // Determine mux type and print a debug line; if DTLS, try to print JSON packet
        let from_addr = self.socket.local_addr().ok();
        match mux_type_for(buf) {
            MuxType::Dtls => {
                if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(buf) {
                    warn!("[UdpNetworkMux][write DTLS][{:?} -> {}] {}", from_addr, to_addr, json);
                    #[allow(unused)] {  }
                } else {
                    warn!("[UdpNetworkMux][write DTLS][{:?} -> {}] <parse error> ({} bytes)", from_addr, to_addr, buf.len());
                    #[allow(unused)] {  }
                }
            }
            other => {
                warn!("[UdpNetworkMux][write other][{:?} -> {}] {:?} ({} bytes)", from_addr, to_addr, other, buf.len());
                #[allow(unused)] {  }
            }
        }
        match self.socket.send_to(buf, to_addr) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("udp send_to failed: {}", e)),
        }
    }

    fn get_handle_dtls(&self) -> Option<HandleDtls> { self.handle_dtls.lock().ok().and_then(|g| g.clone()) }

    fn set_handle_dtls(&mut self, handler: Option<HandleDtls>) { if let Ok(mut g) = self.handle_dtls.lock() { *g = handler; } }

    fn with_handle_dtls(self, handler: HandleDtls) -> Self where Self: Sized {
        if let Ok(mut g) = self.handle_dtls.lock() { *g = Some(handler); }
        self
    }

    fn get_handle_stun(&self) -> Option<HandleStun> { self.handle_stun.lock().ok().and_then(|g| g.clone()) }

    fn set_handle_stun(&mut self, handler: Option<HandleStun>) { if let Ok(mut g) = self.handle_stun.lock() { *g = handler; } }

    fn with_handle_stun(self, handler: HandleStun) -> Self where Self: Sized {
        if let Ok(mut g) = self.handle_stun.lock() { *g = Some(handler); }
        self
    }

    fn get_handle_turn(&self) -> Option<HandleTurn> { self.handle_turn.lock().ok().and_then(|g| g.clone()) }

    fn set_handle_turn(&mut self, handler: Option<HandleTurn>) { if let Ok(mut g) = self.handle_turn.lock() { *g = handler; } }

    fn with_handle_turn(self, handler: HandleTurn) -> Self where Self: Sized {
        if let Ok(mut g) = self.handle_turn.lock() { *g = Some(handler); }
        self
    }

    fn as_any(&self) -> &dyn Any { self }
}


#[cfg(test)]
mod tests {
    use super::{mux_type_for, MuxType};

    #[test]
    fn mux_type_empty_is_unknown() {
        assert_eq!(mux_type_for(&[]), MuxType::Unknown);
    }

    #[test]
    fn mux_type_stun_bounds() {
        assert_eq!(mux_type_for(&[0]), MuxType::Stun);
        assert_eq!(mux_type_for(&[3]), MuxType::Stun);
        assert_eq!(mux_type_for(&[4]), MuxType::Unknown);
    }

    #[test]
    fn mux_type_zrtp_bounds() {
        assert_eq!(mux_type_for(&[16]), MuxType::Zrtp);
        assert_eq!(mux_type_for(&[19]), MuxType::Zrtp);
        assert_eq!(mux_type_for(&[15]), MuxType::Unknown);
    }

    #[test]
    fn mux_type_dtls_bounds() {
        assert_eq!(mux_type_for(&[20]), MuxType::Dtls);
        assert_eq!(mux_type_for(&[63]), MuxType::Dtls);
        assert_eq!(mux_type_for(&[64]), MuxType::TurnChannelData);
    }

    #[test]
    fn mux_type_turn_bounds() {
        assert_eq!(mux_type_for(&[0x40]), MuxType::TurnChannelData);
        assert_eq!(mux_type_for(&[0x7f]), MuxType::TurnChannelData);
        assert_eq!(mux_type_for(&[0x80]), MuxType::Rtp);
    }

    #[test]
    fn mux_type_rtp_bounds() {
        assert_eq!(mux_type_for(&[128]), MuxType::Rtp);
        assert_eq!(mux_type_for(&[191]), MuxType::Rtp);
        assert_eq!(mux_type_for(&[192]), MuxType::Unknown);
    }

    #[test]
    fn mux_type_unknown_outside_ranges() {
        assert_eq!(mux_type_for(&[255]), MuxType::Unknown);
    }
}


impl UdpNetworkMux {
    /// Arc-friendly setter for DTLS handler to allow installing from Arc<UdpNetworkMux>.
    pub fn set_handle_dtls_arc(self: &std::sync::Arc<Self>, handler: Option<HandleDtls>) {
        if let Ok(mut g) = self.handle_dtls.lock() { *g = handler; }
    }

    /// Shared handler that processes a datagram as if received on the socket.
    /// Classifies, logs, enqueues DTLS payloads, and invokes installed handlers.
    pub fn process_packet(&self, from: SocketAddr, data: &[u8]) {
        if data.is_empty() { return; }
        let to = self.local_addr().unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
        match mux_type_for(data) {
            MuxType::Dtls => {
                if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(data) {
                    warn!("[UdpNetworkMux][receive][{} -> {:?}] {}", from, to, json);
                    #[allow(unused)] {  }
                } else {
                    warn!("[UdpNetworkMux][receive][{} -> {:?}] <parse error> ({} bytes)", from, to, data.len());
                    #[allow(unused)] {  }
                }
                if let Ok(mut q) = self.dtls_queue.lock() {
                    q.push_back((from, data.to_vec()));
                }
                if let Some(h) = self.handle_dtls.lock().ok().and_then(|g| g.clone()) {
                    let source: &dyn NetworkMux = self;
                    (h)(source, &from, data);
                }
            }
            MuxType::Stun => {
                if let Some(h) = self.handle_stun.lock().ok().and_then(|g| g.clone()) {
                    let source: &dyn NetworkMux = self;
                    (h)(source, &from, data);
                }
            }
            MuxType::TurnChannelData => {
                if let Some(h) = self.handle_turn.lock().ok().and_then(|g| g.clone()) {
                    let source: &dyn NetworkMux = self;
                    (h)(source, &from, data);
                }
            }
            _ => { /* ignore ZRTP, RTP, UNKNOWN */ }
        }
    }

    /// Re-dispatch a buffer as if it was received from the socket from the specified source address.
    /// This mirrors the classification and handler invocation logic used in the receive loop
    /// and additionally enqueues DTLS packets into the internal queue for dtls_recv_* helpers.
    pub fn reprocess(&self, from: SocketAddr, buf: &[u8]) {
        if buf.is_empty() { return; }
        self.process_packet(from, buf);
    }
}
