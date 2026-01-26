use std::fmt;
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

/// Key type that identifies a network endpoint either by direct socket address
/// or by a relay id (when the channel/address allocation has not happened yet).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkEndpointKey {
    /// Direct socket address if the endpoint is directly reachable.
    pub inet_socket_address: Option<SocketAddr>,
    /// Relay id for endpoints that are identified by relay id.
    #[serde(default)]
    pub relay_id: Option<String>,
    /// TURN relay channel number (16-bit) when using a relay endpoint.
    #[serde(default)]
    pub relay_channel: Option<u16>,
}

impl fmt::Display for NetworkEndpointKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(addr) = self.inet_socket_address {
            write!(f, "NetworkEndpointKey(inetSocketAddress={:?})", addr)
        } else if let (Some(id), Some(ch)) = (&self.relay_id, self.relay_channel) {
            write!(f, "NetworkEndpointKey(relayId={:?}, relayChannel={:#X})", id, ch)
        } else if let Some(id) = &self.relay_id {
            // Backward-compat logging (should not occur for new relay keys)
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
    inet_socket_address: Option<SocketAddr>,
    /// TURN relay channel number if using a relay (16-bit per RFC 5766).
    relay_channel: Option<u16>,
    /// Relay server socket address (IP:port) if using a relay.
    relay_address: Option<SocketAddr>,
    /// Optional relay id (Algorand base32 address) used when a channel has not yet been allocated.
    #[serde(default)]
    relay_id: Option<String>,
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

    /// Construct an empty endpoint (no fields set).
    pub fn new_unset() -> Self {
        Self { inet_socket_address: None, relay_channel: None, relay_address: None, relay_id: None }
    }

    /// Construct a relay endpoint key.
    pub fn new_relay(relay_id: String, relay_address: Option<SocketAddr>, relay_channel: Option<u16>) -> Self {
        Self {
            inet_socket_address: None,
            relay_channel: relay_channel,
            relay_address: relay_address,
            relay_id: Some(relay_id),
        }
    }

    /// True if this key represents a relay endpoint.
    pub fn is_relay(&self) -> bool { self.relay_id.is_some() }

    /// Getters for fields
    pub fn inet_socket_address(&self) -> Option<SocketAddr> { self.inet_socket_address }
    pub fn relay_channel(&self) -> Option<u16> { self.relay_channel }
    pub fn relay_address(&self) -> Option<SocketAddr> { self.relay_address }
    pub fn relay_id(&self) -> Option<&str> { self.relay_id.as_deref() }

    /// Setters allowed: relay_address and relay_channel only
    pub fn set_relay_address(&mut self, addr: Option<SocketAddr>) { self.relay_address = addr; }
    pub fn set_relay_channel(&mut self, ch: Option<u16>) { self.relay_channel = ch; }

    /// Build a NetworkEndpointKey from this endpoint.
    /// For relay endpoints, both relay_id and relay_channel must be present.
    pub fn get_key(&self) -> Option<NetworkEndpointKey> {
        if let Some(addr) = self.inet_socket_address {
            Some(NetworkEndpointKey { inet_socket_address: Some(addr), relay_id: None, relay_channel: None })
        } else if let (Some(relay_id), Some(ch)) = (self.relay_id.as_ref(), self.relay_channel) {
            Some(NetworkEndpointKey { inet_socket_address: None, relay_id: Some(relay_id.clone()), relay_channel: Some(ch) })
        } else if self.relay_id.is_some() || self.relay_channel.is_some() {
            // Relay endpoint must include both id and channel per requirement
            panic!("NetworkEndpointKey (relay) requires both relay_id and relay_channel: {:?}", self);
        } else {
            // No fields set; cannot construct a key
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
