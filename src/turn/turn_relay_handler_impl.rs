use crate::api::bingle_api::NetworkEndpoint;
use crate::themes;
use crate::{info_theme, warn_theme};
use crate::turn::turn_handler::{TurnHandler, TurnRelayHandler, TurnMessageWithAddress, WrappedMessageWithNetworkEndpoint};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

// Local copy of TURN ChannelData helpers (duplication acceptable at this stage)
fn parse_channel_data_header(packet: &[u8]) -> Option<(u16, usize, usize)> {
    if packet.len() < 4 { return None; }
    let ch = u16::from_be_bytes([packet[0], packet[1]]);
    let len = u16::from_be_bytes([packet[2], packet[3]]) as usize;
    let total = packet.len();
    if total < 4 + len { return None; }
    let padded_len = (len + 3) & !3; // 4-byte boundary for data portion
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

/// Relay-side TURN implementation, split into its own file
pub struct TurnRelayHandlerImpl {
    // channel -> (source, destination) peer addresses for this channel
    ch_to_pair: Mutex<HashMap<u16, (SocketAddr, SocketAddr)>>,
    // (source, dest) -> channel
    pair_to_ch: Mutex<HashMap<(SocketAddr, SocketAddr), u16>>,
    // Allowed mapping of ids to addresses and reverse (registered via Listen or Call)
    allowed_id_to_addr: Mutex<HashMap<String, SocketAddr>>,
    allowed_addr_to_id: Mutex<HashMap<SocketAddr, String>>,
}

impl TurnRelayHandlerImpl {
    pub fn new() -> Self {
        Self {
            ch_to_pair: Mutex::new(HashMap::new()),
            pair_to_ch: Mutex::new(HashMap::new()),
            allowed_id_to_addr: Mutex::new(HashMap::new()),
            allowed_addr_to_id: Mutex::new(HashMap::new()),
        }
    }

    // Optional helpers retained for parity with original impl
    pub fn is_ip_allowed(&self, ip: IpAddr) -> bool {
        if let Ok(map) = self.allowed_id_to_addr.lock() { map.values().any(|addr| addr.ip() == ip) } else { false }
    }

    /// Lookup helpers for tests/engine
    pub fn lookup_addr_by_id(&self, id: &str) -> Option<SocketAddr> {
        self.allowed_id_to_addr.lock().ok()?.get(id).cloned()
    }
    pub fn lookup_id_by_addr(&self, addr: &SocketAddr) -> Option<String> {
        self.allowed_addr_to_id.lock().ok()?.get(addr).cloned()
    }

    fn alloc_channel(&self) -> Option<u16> {
        use uuid::Uuid;
        const MIN_CH: u16 = 0x4000;
        const MAX_CH: u16 = 0x7FFE; // inclusive per RFC range
        let seed = Uuid::new_v4().as_u128();
        let range = (MAX_CH - MIN_CH + 1) as u32;
        let mut candidate: u16 = (MIN_CH as u32 + (seed as u32 % range)) as u16;
        for _ in 0..range {
            if candidate < MIN_CH || candidate > MAX_CH { candidate = MIN_CH; }
            if let Ok(map) = self.ch_to_pair.lock() { if !map.contains_key(&candidate) { return Some(candidate); } } else { return None; }
            candidate = if candidate == MAX_CH { MIN_CH } else { candidate + 1 };
        }
        None
    }
}

impl Default for TurnRelayHandlerImpl { fn default() -> Self { Self::new() } }

impl TurnHandler for TurnRelayHandlerImpl {
    fn handle_listen(&self, source_id: &str, source: &SocketAddr) -> bool {
        if let (Ok(mut id2a), Ok(mut a2id)) = (self.allowed_id_to_addr.lock(), self.allowed_addr_to_id.lock()) {
            id2a.insert(source_id.to_string(), *source);
            a2id.insert(*source, source_id.to_string());
            info_theme!(themes::TURN, "[TurnRelayHandlerImpl::handle_listen] registered {} -> {}", source_id, source);
            true
        } else {
            info_theme!(themes::TURN, "[TurnRelayHandlerImpl::handle_listen] lock poisoned while adding {} -> {}", source_id, source);
            false
        }
    }

    fn handle_turn_incoming(
        &self,
        sender_address: Option<&SocketAddr>,
        local_public_address: Option<SocketAddr>,
        packet: &[u8],
    ) -> Option<WrappedMessageWithNetworkEndpoint> {
        info_theme!(themes::TURN, "[TurnRelayHandlerImpl::handle_turn_incoming] {} bytes from {:?}, local_public_address={:?}", packet.len(), sender_address, local_public_address);
        let (ch, len, _pad) = match parse_channel_data_header(packet) {
            Some(v) => v,
            None => {
                warn_theme!(themes::TURN, "[TurnRelayHandlerImpl::handle_turn_incoming] failed to parse channel data header ({} bytes)", packet.len());
                return None;
            }
        };

        let (source_addr, dest_addr) = {
            let map = match self.ch_to_pair.lock() {
                Ok(m) => m,
                Err(_) => {
                    tracing::warn!("[TurnRelayHandlerImpl::handle_turn_incoming] ch_to_pair lock poisoned");
                    return None;
                }
            };
            if let Some(pair) = map.get(&ch).cloned() {
                tracing::debug!("[TurnRelayHandlerImpl::handle_turn_incoming] found channel mapping in ch_to_pair for ch {} => {:?}", ch, pair);
                pair
            } else {
                // No mapping found - check if sender is allowed (registered via Listen)
                let sender_addr = match sender_address {
                    Some(addr) => *addr,
                    None => {
                        tracing::warn!("[TurnRelayHandlerImpl::handle_turn_incoming] no mapping for ch {} and sender_address is None", ch);
                        return None;
                    }
                };
                let relay_id = match self.allowed_addr_to_id.lock() {
                    Ok(m) => match m.get(&sender_addr).cloned() {
                        Some(id) => id,
                        None => {
                            tracing::warn!("[TurnRelayHandlerImpl::handle_turn_incoming] no mapping for ch {} and sender {} is not registered", ch, sender_addr);
                            return None;
                        }
                    },
                    Err(_) => {
                        tracing::warn!("[TurnRelayHandlerImpl::handle_turn_incoming] allowed_addr_to_id lock poisoned");
                        return None;
                    }
                };
                let payload = match packet.get(4..4 + len) {
                    Some(p) => p.to_vec(),
                    None => {
                        tracing::warn!("[TurnRelayHandlerImpl::handle_turn_incoming] packet too short for len {} (total {})", len, packet.len());
                        return None;
                    }
                };
                let network_endpoint = NetworkEndpoint::new_relay(relay_id.clone(), Some(sender_addr), Some(ch));
                tracing::info!(
                    "[TurnRelayHandlerImpl::handle_turn_incoming] inbound from advertised relay {} on ch {} ({} bytes)",
                    relay_id, ch, len
                );
                return Some(WrappedMessageWithNetworkEndpoint { ip_address: sender_addr, message: payload, network_endpoint, is_relay_local: false });
            }
        };

        // Gate by allowed addr presence
        if let Ok(map) = self.allowed_addr_to_id.lock() {
            if !map.contains_key(&dest_addr) {
                tracing::warn!("[TurnRelayHandlerImpl::handle_turn_incoming] rejecting packet from {}: destination address not registered via Listen", dest_addr);
                return None;
            }
        } else {
            tracing::warn!("[TurnRelayHandlerImpl::handle_turn_incoming] lock poisoned while checking allowed addr presence");
            return None;
        }

        if source_addr == dest_addr {
            tracing::warn!("[TurnRelayHandlerImpl::handle_turn_incoming] {}: source and destination are the same", source_addr);
        }
        
        let is_packet_from_dest = sender_address.map(|a| a != &source_addr).unwrap_or(false);
        let payload = match packet.get(4..4 + len) {
            Some(p) => p.to_vec(),
            None => {
                tracing::warn!("[TurnRelayHandlerImpl::handle_turn_incoming] packet too short for len {} (total {})", len, packet.len());
                return None;
            }
        };
        tracing::info!(
            "[TurnRelayHandlerImpl::handle_turn_incoming] accepted is_packet_from_dest={} from {:?} on ch {} ({} bytes)",
            is_packet_from_dest, sender_address, ch, len
        );

        // NOTE: this is being ignored for a relay (always)
        // and is wrong relay_id == sender id ??
        let network_endpoint: NetworkEndpoint = if let Some(sender_addr) = sender_address {
            if let Some(relay_id) = self.allowed_addr_to_id.lock().ok().and_then(|m| m.get(sender_addr).cloned()) {
                let local_pub = match local_public_address {
                    Some(addr) => addr,
                    None => {
                        tracing::warn!("[TurnRelayHandlerImpl::handle_turn_incoming] local_public_address is None for relayed message");
                        return None;
                    }
                };
                NetworkEndpoint::new_relay(relay_id, Some(local_pub), Some(ch))
            } else { NetworkEndpoint::new_direct(*sender_addr) }
        } else {
            tracing::warn!("[TurnRelayHandlerImpl::handle_turn_incoming] rejecting packet from unknown source {:?}", sender_address);
            return None
        };

        Some(WrappedMessageWithNetworkEndpoint {
            ip_address: if is_packet_from_dest { source_addr } else { dest_addr },
            message: payload,
            network_endpoint,
            is_relay_local: source_addr == dest_addr,
        })
    }

    fn send_turn_outgoing(&self, source: &SocketAddr, dest: &SocketAddr, packet: &[u8]) -> Option<TurnMessageWithAddress> {
        let ch = { let map = self.pair_to_ch.lock().ok()?; map.get(&(*source, *dest)).cloned()? };
        let msg = build_channel_data(ch, packet)?;
        Some(TurnMessageWithAddress { ip_address: *dest, message: msg })
    }

    fn handle_call_response(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16, relay_id: &str) {
        if let (Ok(mut c2a), Ok(mut p2c), Ok(mut id2a), Ok(mut a2id)) = (
            self.ch_to_pair.lock(), self.pair_to_ch.lock(), self.allowed_id_to_addr.lock(), self.allowed_addr_to_id.lock()
        ) {
            c2a.insert(channel, (*source, *dest));
            p2c.insert((*source, *dest), channel);
            // On relay we also want to record the relay id association for the source
            id2a.insert(relay_id.to_string(), *source);
            a2id.insert(*source, relay_id.to_string());
        }
        tracing::info!(
            "[TurnRelayHandlerImpl::handle_call_response] set mapping ch={} for {} -> {} (relay_id={})",
            channel, source, dest, relay_id
        );
    }
}

impl TurnRelayHandler for TurnRelayHandlerImpl {
    fn handle_call(&self, source_id: &str, dest_id: &str, source: &SocketAddr, dest: &SocketAddr) -> i32 {
        // If already have a channel for this (source, dest) pair, return it
        if let Ok(map) = self.pair_to_ch.lock() {
            if let Some(ch) = map.get(&(*source, *dest)).cloned() { return ch as i32; }
        } else { return -1; }
        // Allocate new channel
        let ch = match self.alloc_channel() { Some(v) => v, None => return -1 };
        // Insert into maps
        if let (Ok(mut c2a), Ok(mut p2c), Ok(mut id2a), Ok(mut a2id)) = (
            self.ch_to_pair.lock(), self.pair_to_ch.lock(), self.allowed_id_to_addr.lock(), self.allowed_addr_to_id.lock()
        ) {
            // Map channel to both source and destination addresses
            c2a.insert(ch, (*source, *dest));
            p2c.insert((*source, *dest), ch);

            // Register source and destination addresses as allowed if not already present
            if !a2id.contains_key(source) {
                id2a.insert(source_id.to_string(), *source);
                a2id.insert(*source, source_id.to_string());
            }
            if !a2id.contains_key(dest) {
                id2a.insert(dest_id.to_string(), *dest);
                a2id.insert(*dest, dest_id.to_string());
            }
            tracing::info!("[TurnRelayHandlerImpl::handle_call] allocated channel {} for {} ({}) -> {} ({})", ch, source, source_id, dest, dest_id);
            ch as i32
        } else { -1 }
    }
}
