use crate::api::bingle_api::NetworkEndpoint;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

// Shared TURN ChannelData helpers (reused by both Relay and Client implementations)
fn parse_channel_data_header(packet: &[u8]) -> Option<(u16, usize, usize)> {
    if packet.len() < 4 {
        return None;
    }
    let ch = u16::from_be_bytes([packet[0], packet[1]]);
    let len = u16::from_be_bytes([packet[2], packet[3]]) as usize;
    let total = packet.len();
    if total < 4 + len {
        return None;
    }
    // padding up to 4-byte boundary for the data portion
    let padded_len = (len + 3) & !3;
    if total < 4 + padded_len {
        return None;
    }
    let padding = padded_len - len;
    Some((ch, len, padding))
}

pub fn build_channel_data(channel: u16, data: &[u8]) -> Option<Vec<u8>> {
    if data.len() > std::u16::MAX as usize {
        return None;
    }
    let mut out = Vec::with_capacity(4 + data.len() + 3);
    out.extend_from_slice(&channel.to_be_bytes());
    out.extend_from_slice(&(data.len() as u16).to_be_bytes());
    out.extend_from_slice(data);
    let pad = (4 - (data.len() % 4)) % 4;
    if pad > 0 {
        out.extend(std::iter::repeat(0u8).take(pad));
    }
    Some(out)
}

pub struct TurnMessageWithAddress {
    pub ip_address: SocketAddr,
    pub message: Vec<u8>,
}

pub struct WrappedMessageWithNetworkEndpoint {
    pub ip_address: SocketAddr,
    pub network_endpoint: NetworkEndpoint,
    pub message: Vec<u8>,
}

/**
 * Base TURN handler interface
 * Provides common TURN ChannelData wrap/unwrap helpers used by both relay and client roles.
 */
pub trait TurnHandler {
    /**
     * Handle a TURN Listen operation, which in Bingle results
     * from a Relay::Listen message directed at the relay server.
     * Records the sender's id and address so TURN packets will be accepted and
     * the relay can resolve id -> address for subsequent Call routing.
     *
     * @param source_id the id of the peer sending the `Listen` request (issuer suffix trimmed)
     * @param source the address of the peer sending the `Listen` request
     * @return true if the id/address was recorded (or already present), false on error
     */
    fn handle_listen(&self, source_id: &str, source: &SocketAddr) -> bool;

    /**
     * Handle an incoming TURN packet from a peer, which may be the originator or the destination node
     *
     * @param packet the TURN packet payload
     *
     * @return the contained packet and its source IP address
     */
    fn handle_turn_incoming(
        &self,
        sender_address: Option<&SocketAddr>,
        local_public_address: Option<SocketAddr>,
        packet: &[u8],
    ) -> Option<WrappedMessageWithNetworkEndpoint>;

    /**
     * Wrap an outgoing TURN message from this node
     *
     * @param source the address of the peer sending the packet
     * @param dest the address of the peer the packet is sent to
     * @param packet the packet payload to be wrapped
     *
     * @return the wrapped packet and its destination IP address
     */
    fn send_turn_outgoing(
        &self,
        source: &SocketAddr,
        dest: &SocketAddr,
        packet: &[u8],
    ) -> Option<TurnMessageWithAddress>;
}

/**
 * Relay-side control interface: invoked when relay receives Call and when it informs called party.
 */
pub trait TurnRelayHandler {
    /// Allocate or reuse a channel for the (source, dest) pair; returns channel or -1.
    fn handle_call(&self, source: &SocketAddr, dest: &SocketAddr) -> i32;
}

/**
 * Client-side control interface: invoked when client learns about calls/channels.
 */
pub trait TurnClientHandler {
    /// Received when we called someone and got a CallResponse back (source -> dest on channel).
    fn handle_call_response(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16, relay_id: &str);
    /// Received when we are the recipient of a call via the relay (source -> dest on channel).
    fn handle_called(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16);
    /// Received when we get a Listen response from a relay (registers relay address and ID).
    fn handle_listen_response(&self, relay_address: &SocketAddr, relay_id: &str);
}

/// Concrete TURN handler implementing RFC5766 ChannelData wrapping and demux (relay variant)
pub struct TurnHandlerImpl {
    // channel -> (source, destination) peer addresses for this channel
    ch_to_pair: Mutex<HashMap<u16, (SocketAddr, SocketAddr)>>,
    // (source, dest) -> channel
    pair_to_ch: Mutex<HashMap<(SocketAddr, SocketAddr), u16>>,
    // Allowed mapping of ids to addresses and reverse (registered via Listen or Call)
    allowed_id_to_addr: Mutex<HashMap<String, SocketAddr>>,
    allowed_addr_to_id: Mutex<HashMap<SocketAddr, String>>,
}

impl TurnHandlerImpl {
    pub fn new() -> Self {
        Self {
            ch_to_pair: Mutex::new(HashMap::new()),
            pair_to_ch: Mutex::new(HashMap::new()),
            allowed_id_to_addr: Mutex::new(HashMap::new()),
            allowed_addr_to_id: Mutex::new(HashMap::new()),
        }
    }

    /// Test helper: check if any registered address matches the given IP
    pub fn is_ip_allowed(&self, ip: IpAddr) -> bool {
        if let Ok(map) = self.allowed_id_to_addr.lock() {
            map.values().any(|addr| addr.ip() == ip)
        } else {
            false
        }
    }

    /// Lookup helpers for tests/handlers
    pub fn lookup_addr_by_id(&self, id: &str) -> Option<SocketAddr> {
        self.allowed_id_to_addr.lock().ok()?.get(id).cloned()
    }
    pub fn lookup_id_by_addr(&self, addr: &SocketAddr) -> Option<String> {
        self.allowed_addr_to_id.lock().ok()?.get(addr).cloned()
    }
    pub fn lookup_addr_by_channel_for_tests(&self, ch: u16) -> Option<(SocketAddr, SocketAddr)> {
        self.ch_to_pair.lock().ok()?.get(&ch).cloned()
    }
    pub fn lookup_channel_for_pair_for_tests(&self, a: &SocketAddr, b: &SocketAddr) -> Option<u16> {
        self.pair_to_ch.lock().ok()?.get(&(*a, *b)).cloned()
    }

    const MIN_CH: u16 = 0x4000;
    const MAX_CH: u16 = 0x7FFE; // inclusive per RFC range

    fn alloc_channel(&self) -> Option<u16> {
        use uuid::Uuid;
        let seed = Uuid::new_v4().as_u128();
        let range = (Self::MAX_CH - Self::MIN_CH + 1) as u32;
        let mut candidate: u16 = (Self::MIN_CH as u32 + (seed as u32 % range)) as u16;
        // Probe up to range size for a free slot
        for _ in 0..range {
            // Ensure candidate is within [MIN, MAX]
            if candidate < Self::MIN_CH || candidate > Self::MAX_CH {
                candidate = Self::MIN_CH;
            }
            if let Ok(map) = self.ch_to_pair.lock() {
                if !map.contains_key(&candidate) {
                    return Some(candidate);
                }
            } else {
                return None;
            }
            // Next candidate with wrap-around
            candidate = if candidate == Self::MAX_CH {
                Self::MIN_CH
            } else {
                candidate + 1
            };
        }
        None
    }
}

impl Default for TurnHandlerImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnHandler for TurnHandlerImpl {
    fn handle_listen(&self, source_id: &str, source: &SocketAddr) -> bool {
        // Record id -> addr and addr -> id
        if let (Ok(mut id2a), Ok(mut a2id)) = (
            self.allowed_id_to_addr.lock(),
            self.allowed_addr_to_id.lock(),
        ) {
            id2a.insert(source_id.to_string(), *source);
            a2id.insert(*source, source_id.to_string());
            log::info!(
                "[TurnHandlerImpl::handle_listen] registered {} -> {}",
                source_id,
                source
            );
            true
        } else {
            log::info!(
                "[TurnHandlerImpl::handle_listen] lock poisoned while adding {} -> {}",
                source_id,
                source
            );
            false
        }
    }

    fn handle_turn_incoming(
        &self,
        sender_address: Option<&SocketAddr>,
        local_public_address: Option<SocketAddr>,
        packet: &[u8],
    ) -> Option<WrappedMessageWithNetworkEndpoint> {
        log::info!("[TurnHandlerImpl::handle_turn_incoming] {} bytes from {:?}:", packet.len(), sender_address);

        let (ch, len, _pad) = match parse_channel_data_header(packet) {
            Some(header) => header,
            None => {
                log::warn!("[TurnHandlerImpl::handle_turn_incoming] failed to parse TURN channel data header from {} byte packet", packet.len());
                return None;
            }
        };

        let (source_addr, dest_addr) = {
            let map = match self.ch_to_pair.lock() {
                Ok(m) => m,
                Err(_) => {
                    log::error!("[TurnHandlerImpl::handle_turn_incoming] ch_to_pair lock is poisoned");
                    return None;
                }
            };

            match map.get(&ch).cloned() {
                Some(pair) => pair,
                None => {
                    // No mapping found - this could be an inbound packet from a channel we advertised via listen
                    log::info!("[TurnHandlerImpl::handle_turn_incoming] no mapping found for channel {:#X}, checking if sender is allowed", ch);

                    let sender_addr = match sender_address {
                        Some(addr) => *addr,
                        None => {
                            log::warn!("[TurnHandlerImpl::handle_turn_incoming] no sender address provided for unmapped channel {:#X}", ch);
                            return None;
                        }
                    };

                    // Check if sender is in allowed addresses (registered via Listen)
                    let relay_id = match self.allowed_addr_to_id.lock() {
                        Ok(addr_to_id) => {
                            match addr_to_id.get(&sender_addr).cloned() {
                                Some(id) => id,
                                None => {
                                    log::warn!("[TurnHandlerImpl::handle_turn_incoming] sender {} not in allowed addresses for unmapped channel {:#X}", sender_addr, ch);
                                    return None;
                                }
                            }
                        }
                        Err(_) => {
                            log::error!("[TurnHandlerImpl::handle_turn_incoming] allowed_addr_to_id lock is poisoned");
                            return None;
                        }
                    };

                    // Extract payload
                    let payload = packet[4..4 + len].to_vec();

                    // Create relay NetworkEndpoint
                    let network_endpoint = NetworkEndpoint::new_relay(
                        relay_id.clone(),
                        Some(sender_addr),
                        Some(ch),
                    );

                    log::info!(
                        "[TurnHandlerImpl::handle_turn_incoming] inbound packet from advertised relay {} on ch {:#X} ({} bytes), wrapping with endpoint {:?}",
                        relay_id,
                        ch,
                        len,
                        network_endpoint
                    );

                    return Some(WrappedMessageWithNetworkEndpoint {
                        ip_address: sender_addr,
                        message: payload,
                        network_endpoint,
                    });
                }
            }
        };

        // Gate by allowed addr presence
        if let Ok(map) = self.allowed_addr_to_id.lock() {
            if !map.contains_key(&dest_addr) {
                log::info!(
                    "[TurnHandlerImpl::handle_turn_incoming] rejecting packet from {}: address not registered via Listen",
                    dest_addr
                );
                return None;
            }
        } else {
            log::info!(
                "[TurnHandlerImpl::handle_turn_incoming] allowed_addr_to_id lock poisoned; rejecting packet from {}",
                dest_addr
            );
            return None;
        }

        let is_packet_from_dest = sender_address.map(|a| a != &source_addr).unwrap_or(false);

        let payload = packet[4..4 + len].to_vec();
        log::info!(
            "[TurnHandlerImpl::handle_turn_incoming] accepted packet is_packet_from_dest={} from {:?} on ch {} ({:?} {} {:?}) ({} bytes)",
            is_packet_from_dest,
            sender_address,
            ch,
            source_addr,
            (if is_packet_from_dest {"<-"} else {"->"}),
            dest_addr,
            len
        );

        let network_endpoint: NetworkEndpoint = if let Some(sender_addr) = sender_address {
            if let Some(relay_id) = self.lookup_id_by_addr(sender_addr) {
                NetworkEndpoint::new_relay(
                    relay_id,
                    Some(local_public_address.expect("No local public address")),
                    Some(ch),
                )
            } else {
                log::warn!("[TurnHandlerImpl::handle_turn_incoming] from_address {} not registered via Listen; wrapping as direct", sender_addr);
                NetworkEndpoint::new_direct(*sender_addr)
            }
        } else {
            panic!("[TurnHandlerImpl::handle_turn_incoming] from_address None;");
        };
        log::info!(
            "[TurnHandlerImpl::handle_turn_incoming] wrapping packet with network endpoint {:?}",
            network_endpoint
        );

        Some(WrappedMessageWithNetworkEndpoint {
            ip_address: if is_packet_from_dest { source_addr } else { dest_addr },
            message: payload,
            network_endpoint: network_endpoint,
        })
    }

    fn send_turn_outgoing(
        &self,
        source: &SocketAddr,
        dest: &SocketAddr,
        packet: &[u8],
    ) -> Option<TurnMessageWithAddress> {
        let ch = {
            let map = self.pair_to_ch.lock().ok()?;
            map.get(&(*source, *dest)).cloned()?
        };
        let msg = build_channel_data(ch, packet)?;
        Some(TurnMessageWithAddress {
            ip_address: *dest,
            message: msg,
        })
    }
}

impl super::turn_handler::TurnRelayHandler for TurnHandlerImpl {
    fn handle_call(&self, source: &SocketAddr, dest: &SocketAddr) -> i32 {
        // If already have a channel for this (source, dest) pair, return it
        if let Ok(map) = self.pair_to_ch.lock() {
            if let Some(ch) = map.get(&(*source, *dest)).cloned() {
                return ch as i32;
            }
        } else {
            return -1;
        }
        // Allocate new channel
        let ch_opt = self.alloc_channel();
        let ch = match ch_opt {
            Some(v) => v,
            None => return -1,
        };
        // Insert into maps
        if let (Ok(mut c2a), Ok(mut p2c), Ok(mut id2a), Ok(mut a2id)) = (
            self.ch_to_pair.lock(), 
            self.pair_to_ch.lock(),
            self.allowed_id_to_addr.lock(),
            self.allowed_addr_to_id.lock()
        ) {
            // Map channel to both source and destination addresses
            c2a.insert(ch, (*source, *dest));
            p2c.insert((*source, *dest), ch);
            p2c.insert((*dest, *source), ch);

            // Register source and destination addresses as allowed if not already present
            if !a2id.contains_key(source) {
                let source_id = format!("call_{}", source);
                id2a.insert(source_id.clone(), *source);
                a2id.insert(*source, source_id);
            }
            if !a2id.contains_key(dest) {
                let dest_id = format!("call_{}", dest);
                id2a.insert(dest_id.clone(), *dest);
                a2id.insert(*dest, dest_id);
            }

            log::info!(
                "[TurnRelayImpl::handle_call] allocated channel {:#X} for {} -> {} and registered addresses",
                ch,
                source,
                dest
            );
            ch as i32
        } else {
            -1
        }
    }
}

// Provide client-side control callbacks on TurnHandlerImpl for backward compatibility
impl super::turn_handler::TurnClientHandler for TurnHandlerImpl {
    fn handle_listen_response(&self, relay_address: &SocketAddr, relay_id: &str) {
        if let (Ok(mut id2a), Ok(mut a2id)) = (
            self.allowed_id_to_addr.lock(),
            self.allowed_addr_to_id.lock(),
        ) {
            id2a.insert(relay_id.to_string(), *relay_address);
            a2id.insert(*relay_address, relay_id.to_string());
            log::info!(
                "[TurnHandlerImpl::handle_listen_response] registered relay {} -> {}",
                relay_id,
                relay_address
            );
        } else {
            log::warn!(
                "[TurnHandlerImpl::handle_listen_response] failed to register relay {} -> {} due to lock poisoning",
                relay_id,
                relay_address
            );
        }
    }

    fn handle_call_response(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16, relay_id: &str) {
        // Record bidirectional mapping for channel and ensure relay id/address is registered
        if let (Ok(mut c2a), Ok(mut p2c)) = (self.ch_to_pair.lock(), self.pair_to_ch.lock()) {
            c2a.insert(channel, (*source, *dest));
            p2c.insert((*source, *dest), channel);
            p2c.insert((*dest, *source), channel);
        }
        if let (Ok(mut id2a), Ok(mut a2id)) = (
            self.allowed_id_to_addr.lock(),
            self.allowed_addr_to_id.lock(),
        ) {
            // Associate the relay id with the caller/source address so inbound packets from the relay are accepted
            id2a.insert(relay_id.to_string(), *source);
            a2id.insert(*source, relay_id.to_string());
        }
        log::info!(
            "[TurnHandlerImpl::handle_call_response] set mapping ch={:#X} for {} <-> {} (relay_id={})",
            channel,
            source,
            dest,
            relay_id
        );
    }

    fn handle_called(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16) {
        if let (Ok(mut c2a), Ok(mut p2c)) = (self.ch_to_pair.lock(), self.pair_to_ch.lock()) {
            c2a.insert(channel, (*source, *dest));
            p2c.insert((*source, *dest), channel);
            p2c.insert((*dest, *source), channel);
        }
        log::info!(
            "[TurnHandlerImpl::handle_called] set mapping ch={:#X} for {} <-> {}",
            channel,
            source,
            dest
        );
    }
}

pub type TurnRelayImpl = TurnHandlerImpl;

/// Client-side TURN implementation
pub struct TurnClientImpl {
    ch_to_addr: Mutex<HashMap<u16, SocketAddr>>, // channel -> source (originator)
    pair_to_ch: Mutex<HashMap<(SocketAddr, SocketAddr), u16>>, // (a,b) -> ch for both directions
    allowed_id_to_addr: Mutex<HashMap<String, SocketAddr>>, // id -> addr
    allowed_addr_to_id: Mutex<HashMap<SocketAddr, String>>, // addr -> id
}

impl TurnClientImpl {
    pub fn new() -> Self {
        Self {
            ch_to_addr: Mutex::new(HashMap::new()),
            pair_to_ch: Mutex::new(HashMap::new()),
            allowed_id_to_addr: Mutex::new(HashMap::new()),
            allowed_addr_to_id: Mutex::new(HashMap::new()),
        }
    }

    fn insert_mapping(&self, source: &SocketAddr, dest: &SocketAddr, ch: u16) {
        if let (Ok(mut c2a), Ok(mut p2c)) = (self.ch_to_addr.lock(), self.pair_to_ch.lock()) {
            c2a.insert(ch, *source); // always attribute to originator
            p2c.insert((*source, *dest), ch);
            p2c.insert((*dest, *source), ch);
        }
    }
}

impl Default for TurnClientImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnHandler for TurnClientImpl {
    fn handle_listen(&self, source_id: &str, source: &SocketAddr) -> bool {
        if let (Ok(mut id2a), Ok(mut a2id)) = (
            self.allowed_id_to_addr.lock(),
            self.allowed_addr_to_id.lock(),
        ) {
            id2a.insert(source_id.to_string(), *source);
            a2id.insert(*source, source_id.to_string());
            log::info!(
                "[TurnClientImpl::handle_listen] registered {} -> {}",
                source_id,
                source
            );
            true
        } else {
            false
        }
    }

    fn handle_turn_incoming(
        &self,
        sender_address: Option<&SocketAddr>,
        _local_public_address: Option<SocketAddr>,
        packet: &[u8],
    ) -> Option<WrappedMessageWithNetworkEndpoint> {
        let (ch, len, _pad) = parse_channel_data_header(packet)?;
        let payload = packet.get(4..4 + len)?.to_vec();

        // If the packet is from our listener relay (registered via Listen), wrap with a relay-based endpoint.
        if let Some(relay_addr) = sender_address {
            if let Ok(map) = self.allowed_addr_to_id.lock() {
                if let Some(relay_id) = map.get(relay_addr).cloned() {
                    let network_endpoint = NetworkEndpoint::new_relay(relay_id, Some(*relay_addr), Some(ch));
                    log::info!(
                        "[TurnClientImpl::handle_turn_incoming] from registered relay {}; wrapping as {} (ch {:#X}, {} bytes)",
                        relay_addr,
                        network_endpoint,
                        ch,
                        len
                    );
                    return Some(WrappedMessageWithNetworkEndpoint {
                        // For client-side reprocessing, ip_address is not used; set to relay_addr for completeness.
                        ip_address: *relay_addr,
                        message: payload,
                        network_endpoint,
                    });
                }
            } else {
                return None;
            }
        }

        // Otherwise consider the channel mapping and gate by the mapped peer address.
        let addr = {
            let map = self.ch_to_addr.lock().ok()?;
            map.get(&ch).cloned()?
        };

        log::info!(
            "[TurnClientImpl::handle_turn_incoming] accepted packet from {} on ch {} ({} bytes)",
            addr,
            ch,
            len
        );

        let network_endpoint: NetworkEndpoint = NetworkEndpoint::new_direct(addr);
        log::info!(
            "[TurnClientImpl::handle_turn_incoming] wrapping packet with network endpoint {:?}",
            network_endpoint
        );

        Some(WrappedMessageWithNetworkEndpoint {
            ip_address: addr,
            message: payload,
            network_endpoint,
        })
    }

    fn send_turn_outgoing(
        &self,
        source: &SocketAddr,
        dest: &SocketAddr,
        packet: &[u8],
    ) -> Option<TurnMessageWithAddress> {
        let ch = {
            let map = self.pair_to_ch.lock().ok()?;
            map.get(&(*source, *dest)).cloned()?
        };
        let msg = build_channel_data(ch, packet)?;
        Some(TurnMessageWithAddress {
            ip_address: *dest,
            message: msg,
        })
    }
}

impl super::turn_handler::TurnClientHandler for TurnClientImpl {
    fn handle_listen_response(&self, relay_address: &SocketAddr, relay_id: &str) {
        if let (Ok(mut id2a), Ok(mut a2id)) = (
            self.allowed_id_to_addr.lock(),
            self.allowed_addr_to_id.lock(),
        ) {
            id2a.insert(relay_id.to_string(), *relay_address);
            a2id.insert(*relay_address, relay_id.to_string());
            log::info!(
                "[TurnClientImpl::handle_listen_response] registered relay {} -> {}",
                relay_id,
                relay_address
            );
        } else {
            log::warn!(
                "[TurnClientImpl::handle_listen_response] failed to register relay {} -> {} due to lock poisoning",
                relay_id,
                relay_address
            );
        }
    }

    fn handle_call_response(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16, relay_id: &str) {
        self.insert_mapping(source, dest, channel);
        log::info!(
            "[TurnClientImpl::handle_call_response] set mapping ch={:#X} for {} <-> {}",
            channel,
            source,
            dest
        );
        if let (Ok(mut id2a), Ok(mut a2id)) = (
            self.allowed_id_to_addr.lock(),
            self.allowed_addr_to_id.lock(),
        ) {
            id2a.insert(relay_id.to_string(), *source);
            a2id.insert(*source, relay_id.to_string());
            log::info!(
                "[TurnClientImpl::handle_call_response] registered address to id {} -> {}",
                relay_id,
                source
            );
        }
    }

    fn handle_called(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16) {
        self.insert_mapping(source, dest, channel);
        log::info!(
            "[TurnClientImpl::handle_called] set mapping ch={:#X} for {} <-> {}",
            channel,
            source,
            dest
        );
    }
}
