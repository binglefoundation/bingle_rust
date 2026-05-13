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

pub type StateChangeHandler = Arc<dyn Fn(StunState, Option<SocketAddr>) + Send + Sync + 'static>;
pub type ErrorHandler = Arc<dyn Fn(String) + Send + Sync + 'static>;
/// Handler to send a packet given a hostname, port and payload bytes.
pub type SendPacketHandler = Arc<dyn Fn(&str, u16, &[u8]) + Send + Sync + 'static>;

/// Trait describing a STUN endpoint finder that polls servers and evaluates consistency
pub trait StunEndpointFinder: Send + Sync {
    /// Start a thread that polls the provided list of STUN servers.
    /// search_time_ms is used when state is NONE or SINGLE.
    /// repeat_time_ms is used when state is CONSISTENT or INCONSISTENT.
    fn start(&mut self, servers: Vec<SocketAddr>, search_time_ms: u64, repeat_time_ms: u64);

    /// Stop the background thread and clean up resources.
    fn stop(&mut self);

    /// Process an incoming STUN packet. Update the source server status, recompute state
    /// and invoke the stateChangeHandler if the state has changed.
    fn process_packet(&mut self, from: SocketAddr, data: &[u8]);

    /// Set the state change handler callback.
    fn set_state_change_handler(&mut self, handler: Option<StateChangeHandler>);

    /// Set the error handler callback.
    fn set_error_handler(&mut self, handler: Option<ErrorHandler>);

    /// Set the send packet handler callback used to transmit STUN requests.
    fn set_send_packet_handler(&mut self, handler: Option<SendPacketHandler>);
}
