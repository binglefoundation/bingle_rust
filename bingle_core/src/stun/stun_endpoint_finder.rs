use std::net::SocketAddr;
use std::sync::Arc;

/// States of the endpoint discovery across STUN servers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StunState {
    None,
    Single,
    Inconsistent,
    Consistent,
    Blocked,
}

/// The address family a STUN state belongs to (issue #498). The family is that of the mapped
/// address a server reports, not of the server's own address: an IPv4 server reached through NAT64
/// reports an IPv4 mapped address, so it counts towards the IPv4 state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddrFamily {
    V4,
    V6,
}

impl AddrFamily {
    /// The family of `addr`; an IPv4-mapped IPv6 address (`::ffff:a.b.c.d`) counts as IPv4.
    pub fn of(addr: &SocketAddr) -> Self {
        if addr.ip().to_canonical().is_ipv6() {
            AddrFamily::V6
        } else {
            AddrFamily::V4
        }
    }
}

/// Called when the state or endpoint of one address family changes. Each family is tracked on its
/// own, so a dual-stack host reports an IPv4 and an IPv6 state rather than one Inconsistent state.
/// The endpoint is `Some` in `Consistent` (the agreed mapped address) and `Single` (the one mapped
/// address seen so far), and `None` otherwise.
pub type StateChangeHandler =
    Arc<dyn Fn(AddrFamily, StunState, Option<SocketAddr>) + Send + Sync + 'static>;
pub type ErrorHandler = Arc<dyn Fn(String) + Send + Sync + 'static>;
/// Handler to send a packet given a hostname, port and payload bytes.
pub type SendPacketHandler = Arc<dyn Fn(&str, u16, &[u8]) + Send + Sync + 'static>;

/// Trait describing a STUN endpoint finder that polls servers and evaluates consistency
pub trait StunEndpointFinder: Send + Sync {
    /// Initialize the internal state with the provided servers and timing parameters.
    /// Does not start the background thread.
    fn init(&mut self, servers: Vec<SocketAddr>, search_time_ms: u64, repeat_time_ms: u64);

    /// Start a thread that polls the provided list of STUN servers.
    /// search_time_ms is used when state is NONE or SINGLE.
    /// repeat_time_ms is used when state is CONSISTENT or INCONSISTENT.
    fn start(&mut self, servers: Vec<SocketAddr>, search_time_ms: u64, repeat_time_ms: u64);

    /// Stop the background thread and clean up resources.
    fn stop(&mut self);

    /// Process an incoming STUN packet. Update the source server status, recompute state
    /// and invoke the state_change_handler if the state has changed.
    fn process_packet(&mut self, from: SocketAddr, data: &[u8]);

    /// Set the state change handler callback.
    fn set_state_change_handler(&mut self, handler: Option<StateChangeHandler>);

    /// Set the error handler callback.
    fn set_error_handler(&mut self, handler: Option<ErrorHandler>);

    /// Set the send packet handler callback used to transmit STUN requests.
    fn set_send_packet_handler(&mut self, handler: Option<SendPacketHandler>);

    /// Reset the internal state to None so that a future STUN response can trigger the state
    /// change handler again (e.g. after a transient "no relay" condition).
    fn reset_state(&mut self);
}
