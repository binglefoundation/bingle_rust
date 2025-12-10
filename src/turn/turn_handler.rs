use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, IpAddr};
use std::sync::Mutex;

// Shared TURN ChannelData helpers (reused by both Relay and Client implementations)
fn parse_channel_data_header(packet: &[u8]) -> Option<(u16, usize, usize)> {
    if packet.len() < 4 { return None; }
    let ch = u16::from_be_bytes([packet[0], packet[1]]);
    let len = u16::from_be_bytes([packet[2], packet[3]]) as usize;
    let total = packet.len();
    if total < 4 + len { return None; }
    // padding up to 4-byte boundary for the data portion
    let padded_len = (len + 3) & !3;
    if total < 4 + padded_len { return None; }
    let padding = padded_len - len;
    Some((ch, len, padding))
}

fn build_channel_data(channel: u16, data: &[u8]) -> Option<Vec<u8>> {
    if data.len() > std::u16::MAX as usize { return None; }
    let mut out = Vec::with_capacity(4 + data.len() + 3);
    out.extend_from_slice(&channel.to_be_bytes());
    out.extend_from_slice(&(data.len() as u16).to_be_bytes());
    out.extend_from_slice(data);
    let pad = (4 - (data.len() % 4)) % 4;
    if pad > 0 { out.extend(std::iter::repeat(0u8).take(pad)); }
    Some(out)
}

pub struct TurnMessageWithAddress {
    pub ipAddress: SocketAddr,
    pub message: Vec<u8>,
}

pub struct WrappedMessageWithAddress {
    pub ipAddress: SocketAddr,
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
     * Adds the sender's IP to the set of valid addresses from whom TURN packets will be accepted.
     *
     * @param source the address of the peer sending the `Listen` request
     * @return true if the address was recorded (or already present), false on error
     */
    fn handle_listen(&self, source: &SocketAddr) -> bool;

    /**
     * Handle an incoming TURN packet from a peer, which may be the originator or the destination node
     *
     * @param packet the TURN packet payload
     *
     * @return the contained packet and its source IP address
     */
    fn handle_turn_incoming(&self, packet: &[u8]) -> Option<WrappedMessageWithAddress>;

    /**
     * Wrap an outgoing TURN message from this node
     *
     * @param source the address of the peer sending the packet
     * @param dest the address of the peer the packet is sent to
     * @param packet the packet payload to be wrapped
     *
     * @return the wrapped packet and its destination IP address
     */
    fn send_turn_outgoing(&self, source: &SocketAddr, dest: &SocketAddr, packet: &[u8]) -> Option<TurnMessageWithAddress>;
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
    fn handle_call_response(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16);
    /// Received when we are the recipient of a call via the relay (source -> dest on channel).
    fn handle_called(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16);
}

/// Concrete TURN handler implementing RFC5766 ChannelData wrapping and demux (relay variant)
pub struct TurnHandlerImpl {
    // channel -> destination peer address for this channel
    ch_to_addr: Mutex<HashMap<u16, SocketAddr>>,
    // (source, dest) -> channel
    pair_to_ch: Mutex<HashMap<(SocketAddr, SocketAddr), u16>>,
    // Allowed source IPs (Listen registered)
    allowed_ips: Mutex<HashSet<IpAddr>>,
}

impl TurnHandlerImpl {
    pub fn new() -> Self {
        Self {
            ch_to_addr: Mutex::new(HashMap::new()),
            pair_to_ch: Mutex::new(HashMap::new()),
            allowed_ips: Mutex::new(HashSet::new()),
        }
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
            if let Ok(map) = self.ch_to_addr.lock() {
                if !map.contains_key(&candidate) {
                    return Some(candidate);
                }
            } else {
                return None;
            }
            // Next candidate with wrap-around
            candidate = if candidate == Self::MAX_CH { Self::MIN_CH } else { candidate + 1 };
        }
        None
    }

}

impl Default for TurnHandlerImpl { fn default() -> Self { Self::new() } }

impl TurnHandler for TurnHandlerImpl {
    fn handle_listen(&self, source: &SocketAddr) -> bool {
        let ip = source.ip();
        if let Ok(mut set) = self.allowed_ips.lock() {
            let inserted = set.insert(ip);
            if inserted {
                log::info!("[TurnHandlerImpl::handle_listen] added allowed ip {}", ip);
            } else {
                log::info!("[TurnHandlerImpl::handle_listen] ip {} already allowed", ip);
            }
            true
        } else {
            log::info!("[TurnHandlerImpl::handle_listen] lock poisoned while adding {}", ip);
            false
        }
    }

    fn handle_turn_incoming(&self, packet: &[u8]) -> Option<WrappedMessageWithAddress> {
        let (ch, len, _pad) = parse_channel_data_header(packet)?;
        let addr = {
            let map = self.ch_to_addr.lock().ok()?;
            map.get(&ch).cloned()?
        };
        // Gate by allowed IPs
        let ip = addr.ip();
        if let Ok(set) = self.allowed_ips.lock() {
            if !set.contains(&ip) {
                log::info!("[TurnHandlerImpl::handle_turn_incoming] rejecting packet from {}: not in allowed set", ip);
                return None;
            }
        } else {
            log::info!("[TurnHandlerImpl::handle_turn_incoming] allowed_ips lock poisoned; rejecting packet from {}", ip);
            return None;
        }
        let payload = packet[4..4+len].to_vec();
        log::info!("[TurnHandlerImpl::handle_turn_incoming] accepted packet from {} on ch {} ({} bytes)", addr, ch, len);
        Some(WrappedMessageWithAddress { ipAddress: addr, message: payload })
    }

    fn send_turn_outgoing(&self, source: &SocketAddr, dest: &SocketAddr, packet: &[u8]) -> Option<TurnMessageWithAddress> {
        let ch = {
            let map = self.pair_to_ch.lock().ok()?;
            map.get(&(*source, *dest)).cloned()?
        };
        let msg = build_channel_data(ch, packet)?;
        Some(TurnMessageWithAddress { ipAddress: *dest, message: msg })
    }
}

impl super::turn_handler::TurnRelayHandler for TurnHandlerImpl {
    fn handle_call(&self, source: &SocketAddr, dest: &SocketAddr) -> i32 {
        // If already have a channel for this (source, dest) pair, return it
        if let Ok(map) = self.pair_to_ch.lock() {
            if let Some(ch) = map.get(&(*source, *dest)).cloned() { return ch as i32; }
        } else { return -1; }
        // Allocate new channel
        let ch_opt = self.alloc_channel();
        let ch = match ch_opt { Some(v) => v, None => return -1 };
        // Insert into maps
        if let (Ok(mut c2a), Ok(mut p2c)) = (self.ch_to_addr.lock(), self.pair_to_ch.lock()) {
            // Map channel to the source address (originator) for incoming source attribution
            c2a.insert(ch, *source);
            p2c.insert((*source, *dest), ch);
            log::info!("[TurnRelayImpl::handle_call] allocated channel {:#X} for {} -> {}", ch, source, dest);
            ch as i32
        } else {
            -1
        }
    }
}

pub type TurnRelayImpl = TurnHandlerImpl;

/// Client-side TURN implementation
pub struct TurnClientImpl {
    ch_to_addr: Mutex<HashMap<u16, SocketAddr>>,           // channel -> source (originator)
    pair_to_ch: Mutex<HashMap<(SocketAddr, SocketAddr), u16>>, // (a,b) -> ch for both directions
    allowed_ips: Mutex<HashSet<IpAddr>>,                   // Listen-registered IPs
}

impl TurnClientImpl {
    pub fn new() -> Self {
        Self {
            ch_to_addr: Mutex::new(HashMap::new()),
            pair_to_ch: Mutex::new(HashMap::new()),
            allowed_ips: Mutex::new(HashSet::new()),
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

impl Default for TurnClientImpl { fn default() -> Self { Self::new() } }

impl TurnHandler for TurnClientImpl {
    fn handle_listen(&self, source: &SocketAddr) -> bool {
        let ip = source.ip();
        if let Ok(mut set) = self.allowed_ips.lock() {
            let inserted = set.insert(ip);
            if inserted {
                log::info!("[TurnClientImpl::handle_listen] added allowed ip {}", ip);
            } else {
                log::info!("[TurnClientImpl::handle_listen] ip {} already allowed", ip);
            }
            true
        } else {
            log::info!("[TurnClientImpl::handle_listen] lock poisoned while adding {}", ip);
            false
        }
    }

    fn handle_turn_incoming(&self, packet: &[u8]) -> Option<WrappedMessageWithAddress> {
        let (ch, len, _pad) = parse_channel_data_header(packet)?;
        let addr = {
            let map = self.ch_to_addr.lock().ok()?;
            map.get(&ch).cloned()?
        };
        // Gate by allowed IPs
        let ip = addr.ip();
        if let Ok(set) = self.allowed_ips.lock() {
            if !set.contains(&ip) { log::info!("[TurnClientImpl::handle_turn_incoming] rejecting packet from {}: not in allowed set", ip); return None; }
        } else { log::info!("[TurnClientImpl::handle_turn_incoming] allowed_ips lock poisoned; rejecting packet from {}", ip); return None; }
        let payload = packet[4..4+len].to_vec();
        log::info!("[TurnClientImpl::handle_turn_incoming] accepted packet from {} on ch {} ({} bytes)", addr, ch, len);
        Some(WrappedMessageWithAddress { ipAddress: addr, message: payload })
    }

    fn send_turn_outgoing(&self, source: &SocketAddr, dest: &SocketAddr, packet: &[u8]) -> Option<TurnMessageWithAddress> {
        let ch = {
            let map = self.pair_to_ch.lock().ok()?;
            map.get(&(*source, *dest)).cloned()?
        };
        let msg = build_channel_data(ch, packet)?;
        Some(TurnMessageWithAddress { ipAddress: *dest, message: msg })
    }
}

impl super::turn_handler::TurnClientHandler for TurnClientImpl {
    fn handle_call_response(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16) {
        self.insert_mapping(source, dest, channel);
        log::info!("[TurnClientImpl::handle_call_response] set mapping ch={:#X} for {} <-> {}", channel, source, dest);
    }

    fn handle_called(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16) {
        self.insert_mapping(source, dest, channel);
        log::info!("[TurnClientImpl::handle_called] set mapping ch={:#X} for {} <-> {}", channel, source, dest);
    }
}
