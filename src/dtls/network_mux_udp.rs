use std::any::Any;
use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::network_mux_trait::{HandleDtls, HandleStun, HandleTurn, NetworkMux, Result};
use std::sync::OnceLock;
use crate::api::bingle_api::NetworkEndpoint;
use tracing::warn;

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
    socket: Mutex<Option<UdpSocket>>,
    handle_dtls: Mutex<Option<HandleDtls>>,
    handle_stun: Mutex<Option<HandleStun>>,
    handle_turn: std::sync::OnceLock<HandleTurn>,
    running: AtomicBool,
    rx_thread: Mutex<Option<JoinHandle<()>>>,
    pub(crate) span: tracing::Span,
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
            socket: Mutex::new(Some(socket)),
            handle_dtls: std::sync::Mutex::new(None),
            handle_stun: std::sync::Mutex::new(None),
            handle_turn: OnceLock::new(),
            running: AtomicBool::new(false),
            rx_thread: Mutex::new(None),
            span: tracing::Span::none(),
        })
    }

    /// Get the local socket address this mux is bound to
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        let guard = self.socket.lock().map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "socket lock poisoned"))?;
        guard.as_ref().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotConnected, "socket closed"))?.local_addr()
    }

    /// Get only the bound IP address (for debug printing)
    pub fn bound_ip(&self) -> std::io::Result<std::net::IpAddr> {
        self.local_addr().map(|a| a.ip())
    }

    /// Set read timeout on the underlying socket
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> std::io::Result<()> {
        let guard = self.socket.lock().map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "socket lock poisoned"))?;
        if let Some(s) = guard.as_ref() { s.set_read_timeout(dur) } else { Ok(()) }
    }

    // /// Peek the next DTLS datagram from the internal queue without removing it.
    // pub fn dtls_peek_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, NetworkEndpoint)> {
    //     use std::io::{Error, ErrorKind};
    //     let q = self.dtls_queue.lock().map_err(|e| Error::new(ErrorKind::Other, format!("queue poisoned: {}", e)))?;
    //     if let Some((from, data)) = q.front() {
    //         let n = data.len().min(buf.len());
    //         buf[..n].copy_from_slice(&data[..n]);
    //         Ok((n, from.clone()))
    //     } else {
    //         Err(Error::from(ErrorKind::WouldBlock))
    //     }
    // }
    //
    // /// Pop the next DTLS datagram from the internal queue.
    // pub fn dtls_recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, NetworkEndpoint)> {
    //     use std::io::{Error, ErrorKind};
    //     let mut q = self.dtls_queue.lock().map_err(|e| Error::new(ErrorKind::Other, format!("queue poisoned: {}", e)))?;
    //     if let Some((from, data)) = q.pop_front() {
    //         let n = data.len().min(buf.len());
    //         buf[..n].copy_from_slice(&data[..n]);
    //         Ok((n, from))
    //     } else {
    //         Err(Error::from(ErrorKind::WouldBlock))
    //     }
    // }
    //
    // /// Pop the next DTLS datagram for a specific peer from the internal queue.
    // pub fn dtls_recv_from_peer(&self, peer_endpoint: NetworkEndpoint, buf: &mut [u8]) -> std::io::Result<usize> {
    //     use std::io::{Error, ErrorKind};
    //     let mut q = self.dtls_queue.lock().map_err(|e| Error::new(ErrorKind::Other, format!("queue poisoned: {}", e)))?;
    //     if let Some(idx) = q.iter().position(|(from, _)| *from == peer_endpoint) {
    //         if let Some((_, data)) = q.remove(idx) {
    //             let n = data.len().min(buf.len());
    //             buf[..n].copy_from_slice(&data[..n]);
    //             return Ok(n);
    //         }
    //     }
    //     Err(Error::from(ErrorKind::WouldBlock))
    // }

    /// Start the receive loop in a background thread.
    /// Note: call this on an Arc to allow handlers to receive `&dyn NetworkMux` reference.
    pub fn start(self: &Arc<Self>) -> std::io::Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            // already running
            return Ok(());
        }
        let socket = {
            let guard = self.socket.lock().map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "socket lock poisoned"))?;
            guard.as_ref().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotConnected, "socket closed"))?.try_clone()?
        };
        let this = Arc::clone(self);
        let to = self.local_addr().unwrap();
        let span = self.span.clone();
        let handle = thread::spawn(move || {
            let _guard = span.enter();
            let mut buf = [0u8; 2048];

            warn!("[UdpNetworkMux][receive][loop on {:?}] starts", to);

            while this.running.load(Ordering::SeqCst) {
                match socket.recv_from(&mut buf) {
                    Ok((n, from)) => {
                        if n == 0 { continue; }
                        let data = &buf[..n];
                        tracing::trace!("[UdpNetworkMux][receive][loop on {:?}] recv_from {}: {} bytes", to, from, n);
                        this.process_packet(&NetworkEndpoint::new_direct(from), data);
                    }
                    Err(e) => {
                        // Respect timeout for shutdown; ignore WouldBlock/TimedOut, break on other errors
                        if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut {
                            continue;
                        } else {
                            // If socket error, stop running
                            warn!("[UdpNetworkMux][receive][loop on {:?}] error {:?}", to, e);

                            this.running.store(false, Ordering::SeqCst);
                        }
                    }
                }
            }
            // Drop the cloned socket used by the receive loop
            drop(socket);
            // Take the original socket out of the Mutex to close it and free the port
            if let Ok(mut guard) = this.socket.lock() {
                let taken = guard.take();
                drop(taken);
                warn!("[UdpNetworkMux][receive][loop on {:?}] socket closed, port freed", to);
            }
            warn!("[UdpNetworkMux][receive][loop on {:?}] done", to);

        });
        let mut slot = self.rx_thread.lock().unwrap();
        *slot = Some(handle);
        Ok(())
    }

    /// Stop the receive loop and join the background thread.
    pub fn stop(&self) {
        tracing::debug!("[UdpNetworkMux::stop]");
        self.running.store(false, Ordering::SeqCst);
        // poke the socket by setting a very short timeout so the thread wakes up soon
        let _ = self.set_read_timeout(Some(Duration::from_millis(10)));
        if let Ok(mut slot) = self.rx_thread.lock() {
            if let Some(handle) = slot.take() {
                tracing::debug!("[UdpNetworkMux::stop] joining rx_thread");
                let _ = handle.join();
            }
            else {
                tracing::warn!("[UdpNetworkMux::stop] rx_thread not running");
            }
        }
        else {
            tracing::warn!("[UdpNetworkMux::stop] rx_thread lock poisoned");
        }
        tracing::debug!("[UdpNetworkMux::stop] done");
    }

    /// Returns true if the socket has been closed (taken out of the Mutex).
    pub fn is_closed(&self) -> bool {
        self.socket.lock().map(|g| g.is_none()).unwrap_or(true)
    }
}

impl Drop for UdpNetworkMux {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        // poke the socket by setting a very short timeout so the thread wakes up soon
        let _ = self.set_read_timeout(Some(Duration::from_millis(10)));
        // We cannot join here because the JoinHandle is owned behind a Mutex<Option<..>> and
        // may already be taken or we're in drop; rely on stop() in normal flows.
    }
}

impl NetworkMux for UdpNetworkMux {
    fn write(&self, to: &crate::api::bingle_api::NetworkEndpoint, buf: &[u8]) -> Result<()> {
        // Support two paths:
        // - Relay: when relay_channel and relay_address are provided, wrap payload in TURN ChannelData and send to relay_address
        // - Direct: otherwise, require inet_socket_address and send raw payload
        let socket_guard = self.socket.lock().map_err(|_| "socket lock poisoned".to_string())?;
        let sock = socket_guard.as_ref().ok_or_else(|| "socket closed".to_string())?;
        let from_addr = sock.local_addr().ok();
        if let (Some(ch), Some(relay_addr)) = (to.relay_channel(), to.relay_address()) {
            // Build TURN ChannelData
            let wrapped = match crate::turn::turn_handler::build_channel_data(ch, buf) {
                Some(v) => v,
                None => return Err("TURN build_channel_data failed (payload too large)".to_string()),
            };
            // Log as TURN send
            warn!(
                "[UdpNetworkMux][write TURN][{:?} -> {}][ch=0x{:04X}] inner_len={} wrapped_len={}",
                from_addr,
                relay_addr,
                ch,
                buf.len(),
                wrapped.len()
            );
            return match sock.send_to(&wrapped, relay_addr) {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("udp send_to (relay) failed: {}", e)),
            };
        }
        // Direct path: Extract destination inet address; panic if missing per design
        let to_addr = to
            .inet_socket_address()
            .expect("UdpNetworkMux::write: NetworkSourceKey missing inet_socket_address (and no relay_address)");
        // Determine mux type and print a debug line; if DTLS, try to print JSON packet
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
                tracing::debug!("[UdpNetworkMux][write other][{:?} -> {}] {:?} ({} bytes)", from_addr, to_addr, other, buf.len());
                #[allow(unused)] {  }
            }
        }
        match sock.send_to(buf, to_addr) {
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

    fn get_handle_turn<'a>(&'a self) -> Option<&'a HandleTurn> { self.handle_turn.get() }

    fn set_handle_turn(&mut self, handler: Option<&HandleTurn>) { 
        tracing::debug!("[UdpNetworkMux] set_handle_turn: handler={}", if handler.is_some() { "Some" } else { "None" });
        if let Some(h) = handler { let _ = self.handle_turn.set(h.clone()); }
    }

    fn with_handle_turn(self, handler: HandleTurn) -> Self where Self: Sized {
        let _ = self.handle_turn.set(handler);
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

    /// Arc-friendly setter for STUN handler to allow installing from Arc<UdpNetworkMux>.
    pub fn set_handle_stun_arc(self: &std::sync::Arc<Self>, handler: Option<HandleStun>) {
        if let Ok(mut g) = self.handle_stun.lock() { *g = handler; }
    }

    /// Shared handler that processes a datagram as if received on the socket.
    /// Classifies, logs, enqueues DTLS payloads, and invokes installed handlers.
    pub fn process_packet(&self, from_endpoint: &NetworkEndpoint, data: &[u8]) {
        if data.is_empty() { return; }
        let to = self.local_addr().unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
        match mux_type_for(data) {
            MuxType::Dtls => {
                if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(data) {
                    tracing::trace!("[UdpNetworkMux][process_packet][receive DTLS][{} -> {:?}] {}", from_endpoint, to, json);
                    #[allow(unused)] {  }
                } else {
                    warn!("[UdpNetworkMux][process_packet][receive DTLS][{} -> {:?}] <parse error> ({} bytes)", from_endpoint, to, data.len());
                    #[allow(unused)] {  }
                }
                // if let Ok(mut q) = self.dtls_queue.lock() {
                //     q.push_back((from_endpoint.clone(), data.to_vec()));
                // }
                if let Some(h) = self.handle_dtls.lock().ok().and_then(|g| g.clone()) {
                    let start = Instant::now();
                    let source: &dyn NetworkMux = self;
                    (h)(source, from_endpoint, data);
                    let elapsed = start.elapsed();
                    if elapsed.as_millis() > 0 {
                        tracing::trace!("[UdpNetworkMux][process_packet][DTLS handler] took {}ms", elapsed.as_millis());
                    } else if elapsed.as_micros() > 100 {
                        tracing::trace!("[UdpNetworkMux][process_packet][DTLS handler] took {}μs", elapsed.as_micros());
                    }
                }
            }
            MuxType::Stun => {
                if let Some(h) = self.handle_stun.lock().ok().and_then(|g| g.clone()) {
                    let start = Instant::now();
                    let source: &dyn NetworkMux = self;
                    (h)(source, &from_endpoint.inet_socket_address().expect("Stun messages must originate from an IP"), data);
                    let elapsed = start.elapsed();
                    if elapsed.as_millis() > 0 {
                        tracing::trace!("[UdpNetworkMux][process_packet][STUN handler] took {}ms", elapsed.as_millis());
                    } else if elapsed.as_micros() > 100 {
                        tracing::trace!("[UdpNetworkMux][process_packet][STUN handler] took {}μs", elapsed.as_micros());
                    }
                }
            }
            MuxType::TurnChannelData => {
                if let Some(h) = self.handle_turn.get() {
                    tracing::info!("[UdpNetworkMux][process_packet][receive TURN][{} -> {:?}] {} bytes", from_endpoint, to, data.len());
                    let start = Instant::now();
                    let source: &dyn NetworkMux = self;
                    (h)(source, &from_endpoint.inet_socket_address().expect("TURN messages must originate from an IP"), data);
                    let elapsed = start.elapsed();
                    if elapsed.as_millis() > 0 {
                        tracing::trace!("[UdpNetworkMux][process_packet][TURN handler] took {}ms", elapsed.as_millis());
                    } else if elapsed.as_micros() > 100 {
                        tracing::trace!("[UdpNetworkMux][process_packet][TURN handler] took {}μs", elapsed.as_micros());
                    }
                }
            }
            _ => { /* ignore ZRTP, RTP, UNKNOWN */ }
        }
    }

    /// Re-dispatch a buffer as if it was received from the socket from the specified source endpoint.
    /// This mirrors the classification and handler invocation logic used in the receive loop
    /// and additionally enqueues DTLS packets into the internal queue for dtls_recv_* helpers.
    pub fn reprocess(&self, from: &crate::api::bingle_api::NetworkEndpoint, buf: &[u8]) {
        if buf.is_empty() { return; }
        
        self.process_packet(from, buf);
    }
}
