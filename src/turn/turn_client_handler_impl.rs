use crate::api::bingle_api::NetworkEndpoint;
use crate::turn::turn_handler::{TurnClientHandler, TurnHandler, TurnMessageWithAddress, WrappedMessageWithNetworkEndpoint};
use std::collections::HashMap;
use std::net::SocketAddr;
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

/// Client-side TURN implementation, split into its own file
pub struct TurnClientHandlerImpl {
    ch_to_addr: Mutex<HashMap<u16, SocketAddr>>,                 // channel -> source (originator)
    pair_to_ch: Mutex<HashMap<(SocketAddr, SocketAddr), u16>>,   // (a,b) -> ch for both directions
    allowed_id_to_addr: Mutex<HashMap<String, SocketAddr>>,      // id -> addr
    allowed_addr_to_id: Mutex<HashMap<SocketAddr, String>>,      // addr -> id
}

impl TurnClientHandlerImpl {
    pub fn new() -> Self {
        Self {
            ch_to_addr: Mutex::new(HashMap::new()),
            pair_to_ch: Mutex::new(HashMap::new()),
            allowed_id_to_addr: Mutex::new(HashMap::new()),
            allowed_addr_to_id: Mutex::new(HashMap::new()),
        }
    }

    pub fn lookup_addr_by_id(&self, id: &str) -> Option<SocketAddr> {
        self.allowed_id_to_addr.lock().ok()?.get(id).cloned()
    }
    pub fn lookup_id_by_addr(&self, addr: &SocketAddr) -> Option<String> {
        self.allowed_addr_to_id.lock().ok()?.get(addr).cloned()
    }

    fn insert_mapping(&self, source: &SocketAddr, dest: &SocketAddr, ch: u16) {
        if let (Ok(mut c2a), Ok(mut p2c)) = (self.ch_to_addr.lock(), self.pair_to_ch.lock()) {
            c2a.insert(ch, *source); // always attribute to originator
            p2c.insert((*source, *dest), ch);
            p2c.insert((*dest, *source), ch);
        }
    }
}

impl Default for TurnClientHandlerImpl { fn default() -> Self { Self::new() } }

impl TurnHandler for TurnClientHandlerImpl {
    fn handle_listen(&self, _source_id: &str, _source: &SocketAddr) -> bool {
        tracing::error!("[TurnClientHandlerImpl::handle_listen] unexpected relay command listen on client; ignoring");
        false
    }

    fn handle_turn_incoming(
        &self,
        sender_address: Option<&SocketAddr>,
        _local_public_address: Option<SocketAddr>,
        packet: &[u8],
    ) -> Option<WrappedMessageWithNetworkEndpoint> {
        tracing::info!("[TurnClientHandlerImpl::handle_turn_incoming] received TURN packet from {:?} with {} bytes", sender_address, packet.len());
        let (ch, len, _pad) = match parse_channel_data_header(packet) {
            Some(header) => header,
            None => {
                tracing::error!("[TurnClientHandlerImpl::handle_turn_incoming] failed to parse TURN channel data header from {} byte packet", packet.len());
                return None;
            }
        };

        let payload = match packet.get(4..4 + len) {
            Some(slice) => slice.to_vec(),
            None => {
                tracing::error!("[TurnClientHandlerImpl::handle_turn_incoming] packet too short for payload: expected {} bytes at offset 4", len);
                return None;
            }
        };

        // If the packet is from our listener relay (registered via Listen), wrap with a relay-based endpoint.
        if let Some(relay_addr) = sender_address {
            match self.allowed_addr_to_id.lock() {
                Ok(map) => {
                    if let Some(relay_id) = map.get(relay_addr).cloned() {
                        let network_endpoint = NetworkEndpoint::new_relay(relay_id, Some(*relay_addr), Some(ch));
                        tracing::info!(
                            "[TurnClientHandlerImpl::handle_turn_incoming] from registered relay {}; wrapping as {} (ch {}, {} bytes)",
                            relay_addr,
                            network_endpoint,
                            ch,
                            len
                        );
                        return Some(WrappedMessageWithNetworkEndpoint { ip_address: *relay_addr, message: payload, network_endpoint, is_relay_local: false });
                    }
                    else {
                        tracing::warn!("[TurnClientHandlerImpl::handle_turn_incoming] packet from unknown relay {:?}; dropping", relay_addr);
                        return None;
                    }
                }
                Err(_) => {
                    tracing::error!("[TurnClientHandlerImpl::handle_turn_incoming] failed to lock allowed_addr_to_id");
                    return None;
                }
            }
        }
        else {
            tracing::warn!("[TurnClientHandlerImpl::handle_turn_incoming] no sender address for incoming packet; dropping");
            return None;
        }
    }

    // This is never called?
    fn send_turn_outgoing(&self, source: &SocketAddr, dest: &SocketAddr, packet: &[u8]) -> Option<TurnMessageWithAddress> {
        let ch = match self.pair_to_ch.lock() {
            Ok(map) => {
                match map.get(&(*source, *dest)).cloned() {
                    Some(channel) => channel,
                    None => {
                        tracing::error!("[TurnClientHandlerImpl::send_turn_outgoing] no channel mapping found for {} -> {}", source, dest);
                        return None;
                    }
                }
            }
            Err(_) => {
                tracing::error!("[TurnClientHandlerImpl::send_turn_outgoing] failed to lock pair_to_ch");
                return None;
            }
        };

        let msg = match build_channel_data(ch, packet) {
            Some(data) => data,
            None => {
                tracing::error!("[TurnClientHandlerImpl::send_turn_outgoing] failed to build channel data for channel {}", ch);
                return None;
            }
        };

        Some(TurnMessageWithAddress { ip_address: *dest, message: msg })
    }

    fn handle_call_response(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16, relay_id: &str) {
        if let (Ok(mut id2a), Ok(mut a2id)) = (self.allowed_id_to_addr.lock(), self.allowed_addr_to_id.lock()) {
            // Ensure both addresses are present in allowed maps under the relay id
            id2a.insert(relay_id.to_string(), *source);
            a2id.insert(*source, relay_id.to_string());
            id2a.insert(relay_id.to_string(), *dest);
            a2id.insert(*dest, relay_id.to_string());
        }
        self.insert_mapping(source, dest, channel);
        tracing::info!(
            "[TurnClientHandlerImpl] CallResponse: {} -> {} using ch {} (relay_id={})",
            source, dest, channel, relay_id
        );
    }
}

impl TurnClientHandler for TurnClientHandlerImpl {
    fn handle_listen_response(&self, relay_address: &SocketAddr, relay_id: &str) {
        // Register allowed relay id/address mapping on client
        if let (Ok(mut id2a), Ok(mut a2id)) = (self.allowed_id_to_addr.lock(), self.allowed_addr_to_id.lock()) {
            id2a.insert(relay_id.to_string(), *relay_address);
            a2id.insert(*relay_address, relay_id.to_string());
        }
        tracing::info!("[TurnClientHandlerImpl] registered relay {} at {}", relay_id, relay_address);
    }

    fn handle_called(&self, source: &SocketAddr, dest: &SocketAddr, channel: u16) {
        // Check if source is in allowed addresses
        // if let Ok(map) = self.allowed_id_to_addr.lock() {
        //     let source_allowed = map.values().any(|addr| addr == source);
        //     if !source_allowed {
        //         tracing::error!(
        //             "[TurnClientHandlerImpl::handle_called] destination {} not in allowed addresses; ignoring Called message",
        //             dest
        //         );
        //         return;
        //     }
        // } else {
        //     tracing::error!("[TurnClientHandlerImpl::handle_called] failed to lock allowed_id_to_addr");
        //     return;
        // }

        self.insert_mapping(source, dest, channel);
        tracing::info!(
            "[TurnClientHandlerImpl] Called: {} -> {} using ch {}",
            source, dest, channel
        );
    }
}
