use std::fmt;
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

/// Key type that identifies a network endpoint either by direct socket address
/// or by a relay id (when the channel/address allocation has not happened yet).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkEndpointKey {
    /// Direct socket address if the endpoint is directly reachable.
    pub inet_socket_address: Option<SocketAddr>,
    /// Relay id for endpoints that are identified by relay id (no channel/address yet).
    #[serde(default)]
    pub relay_id: Option<String>,
}

impl fmt::Display for NetworkEndpointKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(addr) = self.inet_socket_address {
            write!(f, "NetworkEndpointKey(inetSocketAddress={:?})", addr)
        } else if let Some(id) = &self.relay_id {
            write!(f, "NetworkEndpointKey(relayId={:?})", id)
        } else {
            write!(f, "NetworkEndpointKey(<empty>)")
        }
    }
}

/// NetworkEndpoint identifies where to send network traffic (direct or via relay).
/// Translated from the provided Kotlin data class.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkEndpoint {
    /// Direct socket address if sending directly.
    pub inet_socket_address: Option<SocketAddr>,
    /// TURN relay channel number if using a relay (16-bit per RFC 5766).
    pub relay_channel: Option<u16>,
    /// Relay server socket address (IP:port) if using a relay.
    pub relay_address: Option<SocketAddr>,
    /// Optional relay id (Algorand base32 address) used when a channel has not yet been allocated.
    #[serde(default)]
    pub relay_id: Option<String>,
}

impl NetworkEndpoint {
    /// Construct a direct (non-relay) endpoint key.
    pub fn new_direct(addr: SocketAddr) -> Self {
        Self {
            inet_socket_address: Some(addr),
            relay_channel: None,
            relay_address: None,
            relay_id: None,
        }
    }

    /// Construct a relay endpoint key.
    pub fn new_relay(relay_address: SocketAddr, relay_channel: u16) -> Self {
        Self {
            inet_socket_address: None,
            relay_channel: Some(relay_channel),
            relay_address: Some(relay_address),
            relay_id: None,
        }
    }

    /// True if this key represents a relay endpoint.
    pub fn is_relay(&self) -> bool { self.relay_channel.is_some() }

    /// Build a NetworkEndpointKey from this endpoint.
    /// Returns Some(key) when either inet_socket_address or relay_id is present; otherwise None.
    pub fn get_key(&self) -> Option<NetworkEndpointKey> {
        if let Some(addr) = self.inet_socket_address {
            Some(NetworkEndpointKey { inet_socket_address: Some(addr), relay_id: None })
        } else if let Some(relay_id) = self.relay_id.as_ref() {
            Some(NetworkEndpointKey { inet_socket_address: None, relay_id: Some(relay_id.clone()) })
        } else {
            None
        }
    }
}

impl fmt::Display for NetworkEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_relay() {
            write!(
                f,
                "NetworkSourceKey(relayChannel={:?}, relayAddress={:?}, relayId={:?})",
                self.relay_channel, self.relay_address, self.relay_id
            )
        } else if self.relay_id.is_some() {
            write!(f, "NetworkSourceKey(relayId={:?})", self.relay_id)
        } else {
            write!(f, "NetworkSourceKey(inetSocketAddress={:?})", self.inet_socket_address)
        }
    }
}
