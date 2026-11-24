use std::any::Any;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6, ToSocketAddrs, UdpSocket};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::network_mux_trait::{HandleDtls, HandleStun, HandleTurn, NetworkMux, Result};
use crate::api::bingle_api::NetworkEndpoint;
use std::sync::OnceLock;
use tracing::{debug, info, warn};

/// Which IP address families the node's UDP socket uses (issue #494). `Auto`, the default, is
/// dual-stack where the host supports IPv6 and IPv4-only where it doesn't; `V4` and `V6` restrict
/// the node to one family, to reproduce single-stack problems or to switch IPv6 off in the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IpFamily {
    /// Dual-stack where the host has IPv6, IPv4-only where it doesn't.
    #[default]
    Auto,
    /// IPv4 only.
    V4,
    /// IPv6 only.
    V6,
}

impl std::str::FromStr for IpFamily {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(IpFamily::Auto),
            "v4" | "ipv4" => Ok(IpFamily::V4),
            "v6" | "ipv6" => Ok(IpFamily::V6),
            other => Err(format!(
                "invalid IP family '{other}': expected auto, v4 or v6"
            )),
        }
    }
}

impl std::fmt::Display for IpFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            IpFamily::Auto => "auto",
            IpFamily::V4 => "v4",
            IpFamily::V6 => "v6",
        })
    }
}

/// How a bound socket handles address families. Fixed for the mux's lifetime, so a rebind after
/// a network error recreates the same kind of socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketMode {
    /// An IPv4 socket: IPv4 peers only.
    V4,
    /// An IPv6 socket with `IPV6_V6ONLY` off: IPv6 peers natively and IPv4 peers as
    /// IPv4-mapped (`::ffff:a.b.c.d`) addresses.
    DualStack,
    /// An IPv6 socket with `IPV6_V6ONLY` on: IPv6 peers only.
    V6Only,
}

/// Open a UDP socket on the unspecified address of `mode`'s family at `port` (0 for any).
/// `IPV6_V6ONLY` is always set explicitly: BSD and iOS default it on, Linux and Android off.
fn open_udp(mode: SocketMode, port: u16) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, SockAddr, Socket, Type};
    match mode {
        SocketMode::V4 => UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port)),
        SocketMode::DualStack | SocketMode::V6Only => {
            let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
            socket.set_only_v6(mode == SocketMode::V6Only)?;
            socket.bind(&SockAddr::from(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                port,
            )))?;
            Ok(socket.into())
        }
    }
}

/// The mode of an already-bound socket, read back from the OS.
fn mode_of(socket: &UdpSocket) -> std::io::Result<SocketMode> {
    if socket.local_addr()?.is_ipv4() {
        return Ok(SocketMode::V4);
    }
    if socket2::SockRef::from(socket).only_v6()? {
        Ok(SocketMode::V6Only)
    } else {
        Ok(SocketMode::DualStack)
    }
}

/// The address a received datagram came from, with an IPv4-mapped IPv6 address (`::ffff:a.b.c.d`,
/// as a dual-stack socket reports an IPv4 peer) turned back into plain IPv4. Everything above the
/// socket then sees one form per peer, so endpoint keys, STUN server matches and TURN maps compare
/// equal whichever socket mode received the packet.
pub fn canonical_peer(from: SocketAddr) -> SocketAddr {
    SocketAddr::new(from.ip().to_canonical(), from.port())
}

/// The address to pass to `send_to` on a socket of `mode` for destination `to`. A dual-stack socket
/// sends to an IPv4 peer through its IPv4-mapped form; a single-family socket refuses the other
/// family. The mapped form stays local to the socket call and never reaches a message or advert.
pub fn socket_target(mode: SocketMode, to: SocketAddr) -> std::result::Result<SocketAddr, String> {
    let to = canonical_peer(to);
    match (mode, to) {
        (SocketMode::V4, SocketAddr::V4(_)) => Ok(to),
        (SocketMode::V4, SocketAddr::V6(_)) => Err(format!(
            "cannot send to IPv6 address {to} from an IPv4 socket"
        )),
        (SocketMode::DualStack, SocketAddr::V4(v4)) => Ok(SocketAddr::V6(SocketAddrV6::new(
            v4.ip().to_ipv6_mapped(),
            v4.port(),
            0,
            0,
        ))),
        (SocketMode::DualStack | SocketMode::V6Only, SocketAddr::V6(_)) => Ok(to),
        (SocketMode::V6Only, SocketAddr::V4(_)) => Err(format!(
            "cannot send to IPv4 address {to} from an IPv6-only socket"
        )),
    }
}

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
    /// The concrete local address the socket is bound to (e.g. `0.0.0.0:PORT` or `[::]:PORT`).
    /// Kept so the receive loop can rebind to the same port after a transient
    /// network error (mobile connectivity drop) instead of tearing the transport
    /// down permanently.
    bind_addr: std::net::SocketAddr,
    /// The socket's address-family mode, kept so a rebind recreates the same kind of socket and
    /// so sends can map IPv4 destinations on a dual-stack socket.
    mode: SocketMode,
    handle_dtls: Mutex<Option<HandleDtls>>,
    handle_stun: Mutex<Option<HandleStun>>,
    handle_turn: OnceLock<HandleTurn>,
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
        info!("[UdpNetworkMux] bind {:?}", addr);
        let socket = UdpSocket::bind(&addr)
            .map_err(|e| std::io::Error::other(format!("udp bind to {:?} failed: {}", addr, e)))?;
        Self::from_socket(socket)
    }

    /// Bind a UDP socket on every local address of `family` at `port` (0 for any): `[::]` with
    /// `IPV6_V6ONLY` off for `Auto`, falling back to `0.0.0.0` when the host has no IPv6; `0.0.0.0`
    /// for `V4`; `[::]` with `IPV6_V6ONLY` on for `V6`.
    pub fn bind_all(port: u16, family: IpFamily) -> std::io::Result<Self> {
        #[allow(unused)]
        {
            crate::util::printing::enable_immediate_prints();
        }
        let socket = match family {
            IpFamily::V4 => open_udp(SocketMode::V4, port),
            IpFamily::V6 => open_udp(SocketMode::V6Only, port),
            IpFamily::Auto => open_udp(SocketMode::DualStack, port).or_else(|e| {
                info!(
                    "[UdpNetworkMux] dual-stack bind on port {} failed ({}); falling back to IPv4",
                    port, e
                );
                open_udp(SocketMode::V4, port)
            }),
        }
        .map_err(|e| {
            std::io::Error::other(format!(
                "udp bind (family {family}) to port {port} failed: {e}"
            ))
        })?;
        info!(
            "[UdpNetworkMux] bound {:?} for family {}",
            socket.local_addr(),
            family
        );
        Self::from_socket(socket)
    }

    fn from_socket(socket: UdpSocket) -> std::io::Result<Self> {
        // Set a modest read timeout to allow responsive shutdown of the receive loop
        socket.set_read_timeout(Some(Duration::from_millis(200)))?;
        // Remember the concrete bound address (resolves port 0 to the assigned
        // port) so we can rebind to the same port on transient network errors.
        let bind_addr = socket.local_addr()?;
        let mode = mode_of(&socket)?;
        Ok(Self {
            socket: Mutex::new(Some(socket)),
            bind_addr,
            mode,
            handle_dtls: Mutex::new(None),
            handle_stun: Mutex::new(None),
            handle_turn: OnceLock::new(),
            running: AtomicBool::new(false),
            rx_thread: Mutex::new(None),
            span: tracing::Span::none(),
        })
    }

    /// Get the local socket address this mux is bound to
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        let guard = self
            .socket
            .lock()
            .map_err(|_| std::io::Error::other("socket lock poisoned"))?;
        guard
            .as_ref()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotConnected, "socket closed"))?
            .local_addr()
    }

    /// The socket's address-family mode.
    pub fn mode(&self) -> SocketMode {
        self.mode
    }

    /// Get only the bound IP address (for debug printing)
    pub fn bound_ip(&self) -> std::io::Result<std::net::IpAddr> {
        self.local_addr().map(|a| a.ip())
    }

    /// Rebind a fresh UDP socket to the original bind address, replacing the one
    /// held in the Mutex, and return a clone for the receive loop.
    ///
    /// Used to recover the transport after a transient network error (e.g. a
    /// mobile connectivity drop that killed the old socket). The caller must have
    /// already dropped every handle to the previous socket — including clearing
    /// the Mutex — so the port is free to rebind.
    fn rebind(&self) -> std::io::Result<UdpSocket> {
        let socket = if self.bind_addr.ip().is_unspecified() {
            open_udp(self.mode, self.bind_addr.port())
        } else {
            UdpSocket::bind(self.bind_addr)
        }
        .map_err(|e| {
            std::io::Error::other(format!("udp rebind to {:?} failed: {}", self.bind_addr, e))
        })?;
        socket.set_read_timeout(Some(Duration::from_millis(200)))?;
        let loop_clone = socket.try_clone()?;
        // Install the live socket so write() and local_addr() use it again.
        let mut guard = self
            .socket
            .lock()
            .map_err(|_| std::io::Error::other("socket lock poisoned"))?;
        *guard = Some(socket);
        Ok(loop_clone)
    }

    /// Test hook: drop the live socket, as the receive loop does after a receive error, so the
    /// port is free for `rebind_for_tests`.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn drop_socket_for_tests(&self) {
        let taken = self.socket.lock().ok().and_then(|mut guard| guard.take());
        drop(taken);
    }

    /// Test hook: run the receive loop's `rebind` recovery and return its socket clone.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn rebind_for_tests(&self) -> std::io::Result<UdpSocket> {
        self.rebind()
    }

    /// Set read timeout on the underlying socket
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> std::io::Result<()> {
        let guard = self
            .socket
            .lock()
            .map_err(|_| std::io::Error::other("socket lock poisoned"))?;
        if let Some(s) = guard.as_ref() {
            s.set_read_timeout(dur)
        } else {
            Ok(())
        }
    }

    /// Start the receive loop in a background thread.
    /// Note: call this on an Arc to allow handlers to receive `&dyn NetworkMux` reference.
    pub fn start(self: &Arc<Self>) -> std::io::Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            // already running
            return Ok(());
        }
        let socket = {
            let guard = self
                .socket
                .lock()
                .map_err(|_| std::io::Error::other("socket lock poisoned"))?;
            guard
                .as_ref()
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::NotConnected, "socket closed")
                })?
                .try_clone()?
        };
        let this = Arc::clone(self);
        let to = self.local_addr().unwrap();
        let span = self.span.clone();
        let handle = thread::spawn(move || {
            let _guard = span.enter();
            let mut buf = [0u8; 2048];
            // Held in an Option so a transient network error can drop it (freeing
            // the port) and a later rebind can replace it, keeping the loop alive.
            let mut socket_opt: Option<UdpSocket> = Some(socket);

            info!("[UdpNetworkMux][receive][loop on {:?}] starts", to);

            while this.running.load(Ordering::SeqCst) {
                // Recover the socket if a prior error dropped it. Binding to the
                // unspecified address (0.0.0.0:PORT) succeeds even while the
                // network is down, after which recv just times out until traffic
                // returns — so this backs off rather than spinning.
                if socket_opt.is_none() {
                    match this.rebind() {
                        Ok(s) => {
                            info!(
                                "[UdpNetworkMux][receive][loop on {:?}] rebound socket after network error",
                                to
                            );
                            socket_opt = Some(s);
                        }
                        Err(e) => {
                            debug!(
                                "[UdpNetworkMux][receive][loop on {:?}] rebind failed: {}; retrying",
                                to, e
                            );
                            thread::sleep(Duration::from_millis(500));
                            continue;
                        }
                    }
                }

                let recv_result = socket_opt.as_ref().unwrap().recv_from(&mut buf);
                match recv_result {
                    Ok((n, from)) => {
                        if n == 0 {
                            continue;
                        }
                        let from = canonical_peer(from);
                        let data = &buf[..n];
                        tracing::trace!(
                            "[UdpNetworkMux][receive][loop on {:?}] recv_from {}: {} bytes",
                            to,
                            from,
                            n
                        );
                        this.process_packet(&NetworkEndpoint::new_direct(from), data);
                    }
                    Err(e) => {
                        // Timeouts are expected (they let us poll `running` for
                        // shutdown) — just loop again.
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut
                        {
                            continue;
                        }
                        // Any other error means the underlying socket died — on
                        // mobile this is a transient connectivity drop, not a
                        // permanent failure. Drop the dead socket (both this
                        // clone and the Mutex copy, freeing the port) and let the
                        // top of the loop rebind so listening recovers on its own
                        // when the network returns.
                        warn!(
                            "[UdpNetworkMux][receive][loop on {:?}] recv error {:?}; will rebind",
                            to, e
                        );
                        socket_opt = None;
                        if let Ok(mut guard) = this.socket.lock() {
                            let taken = guard.take();
                            drop(taken);
                        }
                        thread::sleep(Duration::from_millis(500));
                    }
                }
            }
            // Shutting down: drop the receive-loop socket and clear the Mutex to
            // close it and free the port.
            drop(socket_opt);
            if let Ok(mut guard) = this.socket.lock() {
                let taken = guard.take();
                drop(taken);
                info!(
                    "[UdpNetworkMux][receive][loop on {:?}] socket closed, port freed",
                    to
                );
            }
            info!("[UdpNetworkMux][receive][loop on {:?}] done", to);
        });
        let mut slot = self.rx_thread.lock().unwrap();
        *slot = Some(handle);
        Ok(())
    }

    /// Stop the receive loop and join the background thread.
    pub fn stop(&self) {
        tracing::info!("[UdpNetworkMux::stop]");
        self.running.store(false, Ordering::SeqCst);
        // poke the socket by setting a very short timeout so the thread wakes up soon
        let _ = self.set_read_timeout(Some(Duration::from_millis(10)));
        if let Ok(mut slot) = self.rx_thread.lock() {
            if let Some(handle) = slot.take() {
                tracing::debug!("[UdpNetworkMux::stop] joining rx_thread");
                let _ = handle.join();
            } else {
                tracing::warn!("[UdpNetworkMux::stop] rx_thread not running");
            }
        } else {
            tracing::warn!("[UdpNetworkMux::stop] rx_thread lock poisoned");
        }
        tracing::info!("[UdpNetworkMux::stop] done");
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
    fn write(&self, to: &NetworkEndpoint, buf: &[u8]) -> Result<()> {
        // Support two paths:
        // - Relay: when relay_channel and relay_address are provided, wrap payload in TURN ChannelData and send to relay_address
        // - Direct: otherwise, require inet_socket_address and send raw payload
        let socket_guard = self
            .socket
            .lock()
            .map_err(|_| "socket lock poisoned".to_string())?;
        let sock = socket_guard
            .as_ref()
            .ok_or_else(|| "socket closed".to_string())?;
        let from_addr = sock.local_addr().ok();
        if let (Some(ch), Some(relay_addr)) = (to.relay_channel(), to.relay_address()) {
            // Build TURN ChannelData
            let wrapped = match crate::turn::turn_handler::build_channel_data(ch, buf) {
                Some(v) => v,
                None => {
                    return Err("TURN build_channel_data failed (payload too large)".to_string());
                }
            };
            // Log as TURN send
            info!(
                "[UdpNetworkMux][write TURN][{:?} -> {}][ch=0x{:04X}] inner_len={} wrapped_len={}",
                from_addr,
                relay_addr,
                ch,
                buf.len(),
                wrapped.len()
            );
            let relay_target = socket_target(self.mode, relay_addr)?;
            return match sock.send_to(&wrapped, relay_target) {
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
                    debug!(
                        "[UdpNetworkMux][write DTLS][{:?} -> {}] {}",
                        from_addr, to_addr, json
                    );
                    #[allow(unused)]
                    {}
                } else {
                    warn!(
                        "[UdpNetworkMux][write DTLS][{:?} -> {}] <parse error> ({} bytes)",
                        from_addr,
                        to_addr,
                        buf.len()
                    );
                    #[allow(unused)]
                    {}
                }
            }
            other => {
                tracing::debug!(
                    "[UdpNetworkMux][write other][{:?} -> {}] {:?} ({} bytes)",
                    from_addr,
                    to_addr,
                    other,
                    buf.len()
                );
                #[allow(unused)]
                {}
            }
        }
        let target = socket_target(self.mode, to_addr)?;
        match sock.send_to(buf, target) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("udp send_to failed: {}", e)),
        }
    }

    fn get_handle_dtls(&self) -> Option<HandleDtls> {
        self.handle_dtls.lock().ok().and_then(|g| g.clone())
    }

    fn set_handle_dtls(&mut self, handler: Option<HandleDtls>) {
        if let Ok(mut g) = self.handle_dtls.lock() {
            *g = handler;
        }
    }

    fn with_handle_dtls(self, handler: HandleDtls) -> Self
    where
        Self: Sized,
    {
        if let Ok(mut g) = self.handle_dtls.lock() {
            *g = Some(handler);
        }
        self
    }

    fn get_handle_stun(&self) -> Option<HandleStun> {
        self.handle_stun.lock().ok().and_then(|g| g.clone())
    }

    fn set_handle_stun(&mut self, handler: Option<HandleStun>) {
        if let Ok(mut g) = self.handle_stun.lock() {
            *g = handler;
        }
    }

    fn with_handle_stun(self, handler: HandleStun) -> Self
    where
        Self: Sized,
    {
        if let Ok(mut g) = self.handle_stun.lock() {
            *g = Some(handler);
        }
        self
    }

    fn get_handle_turn(&self) -> Option<&HandleTurn> {
        self.handle_turn.get()
    }

    fn set_handle_turn(&mut self, handler: Option<&HandleTurn>) {
        tracing::debug!(
            "[UdpNetworkMux] set_handle_turn: handler={}",
            if handler.is_some() { "Some" } else { "None" }
        );
        if let Some(h) = handler {
            let _ = self.handle_turn.set(h.clone());
        }
    }

    fn with_handle_turn(self, handler: HandleTurn) -> Self
    where
        Self: Sized,
    {
        let _ = self.handle_turn.set(handler);
        self
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl UdpNetworkMux {
    /// Arc-friendly setter for DTLS handler to allow installing from Arc<UdpNetworkMux>.
    pub fn set_handle_dtls_arc(self: &Arc<Self>, handler: Option<HandleDtls>) {
        if let Ok(mut g) = self.handle_dtls.lock() {
            *g = handler;
        }
    }

    /// Arc-friendly setter for STUN handler to allow installing from Arc<UdpNetworkMux>.
    pub fn set_handle_stun_arc(self: &Arc<Self>, handler: Option<HandleStun>) {
        if let Ok(mut g) = self.handle_stun.lock() {
            *g = handler;
        }
    }

    /// Shared handler that processes a datagram as if received on the socket.
    /// Classifies, logs, enqueues DTLS payloads, and invokes installed handlers.
    pub fn process_packet(&self, from_endpoint: &NetworkEndpoint, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let to = self
            .local_addr()
            .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
        match mux_type_for(data) {
            MuxType::Dtls => {
                if let Ok(json) = crate::dtls::dtls_debug::dtls_udp_to_json(data) {
                    tracing::trace!(
                        "[UdpNetworkMux][process_packet][receive DTLS][{} -> {:?}] {}",
                        from_endpoint,
                        to,
                        json
                    );
                    #[allow(unused)]
                    {}
                } else {
                    warn!(
                        "[UdpNetworkMux][process_packet][receive DTLS][{} -> {:?}] <parse error> ({} bytes)",
                        from_endpoint,
                        to,
                        data.len()
                    );
                    #[allow(unused)]
                    {}
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
                        tracing::trace!(
                            "[UdpNetworkMux][process_packet][DTLS handler] took {}ms",
                            elapsed.as_millis()
                        );
                    } else if elapsed.as_micros() > 100 {
                        tracing::trace!(
                            "[UdpNetworkMux][process_packet][DTLS handler] took {}μs",
                            elapsed.as_micros()
                        );
                    }
                }
            }
            MuxType::Stun => {
                if let Some(h) = self.handle_stun.lock().ok().and_then(|g| g.clone()) {
                    let start = Instant::now();
                    let source: &dyn NetworkMux = self;
                    (h)(
                        source,
                        &from_endpoint
                            .inet_socket_address()
                            .expect("Stun messages must originate from an IP"),
                        data,
                    );
                    let elapsed = start.elapsed();
                    if elapsed.as_millis() > 0 {
                        tracing::trace!(
                            "[UdpNetworkMux][process_packet][STUN handler] took {}ms",
                            elapsed.as_millis()
                        );
                    } else if elapsed.as_micros() > 100 {
                        tracing::trace!(
                            "[UdpNetworkMux][process_packet][STUN handler] took {}μs",
                            elapsed.as_micros()
                        );
                    }
                }
            }
            MuxType::TurnChannelData => {
                if let Some(h) = self.handle_turn.get() {
                    tracing::info!(
                        "[UdpNetworkMux][process_packet][receive TURN][{} -> {:?}] {} bytes",
                        from_endpoint,
                        to,
                        data.len()
                    );
                    let start = Instant::now();
                    let source: &dyn NetworkMux = self;
                    (h)(
                        source,
                        &from_endpoint
                            .inet_socket_address()
                            .expect("TURN messages must originate from an IP"),
                        data,
                    );
                    let elapsed = start.elapsed();
                    if elapsed.as_millis() > 0 {
                        tracing::trace!(
                            "[UdpNetworkMux][process_packet][TURN handler] took {}ms",
                            elapsed.as_millis()
                        );
                    } else if elapsed.as_micros() > 100 {
                        tracing::trace!(
                            "[UdpNetworkMux][process_packet][TURN handler] took {}μs",
                            elapsed.as_micros()
                        );
                    }
                }
            }
            _ => { /* ignore ZRTP, RTP, UNKNOWN */ }
        }
    }

    /// Re-dispatch a buffer as if it was received from the socket from the specified source endpoint.
    /// This mirrors the classification and handler invocation logic used in the receive loop
    /// and additionally enqueues DTLS packets into the internal queue for dtls_recv_* helpers.
    pub fn reprocess(&self, from: &NetworkEndpoint, buf: &[u8]) {
        if buf.is_empty() {
            return;
        }

        self.process_packet(from, buf);
    }
}
