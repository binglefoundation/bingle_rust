use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;

pub struct TurnMessageWithAddress {
    pub ipAddress: SocketAddr,
    pub message: Vec<u8>,
}

pub struct WrappedMessageWithAddress {
    pub ipAddress: SocketAddr,
    pub message: Vec<u8>,
}

/**
 * TURN handler interface
 * This runs in a TURN client and handles incoming turn packets.
 *
 * To open a channel, `handle_call` is used
 * When the channel is open, `handle_turn_incoming` is used to handle incoming packets
 * The channel field in the TURN packet gets looked up to determine the source address
 * Similarly, outgoing packets are wrapped with the chennel, which can be looked up by destination address`
 */
pub trait TurnHandler {
    /**
     * Handle a TURN Call operation, which in Bingle results
     * from a Relay::Call message directed at the relay server.
     *
     * @param source the address of the peer sending the `Call` request
     * @return the channel number to use for the TURN session, or -1 if the call failed
     */
    fn handle_call(&self, source: &SocketAddr) -> i32;

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
     * @param dest the address of the peer the packet is sent to
     * @param packet the packet payload to be wrapped
     *
     * @return the wrapped packet and its destination IP address
     */
    fn send_turn_outgoing(&self, dest: &SocketAddr, packet: &[u8]) -> Option<TurnMessageWithAddress>;
}

/// Concrete TURN handler implementing RFC5766 ChannelData wrapping and demux
pub struct TurnHandlerImpl {
    // channel -> peer address
    ch_to_addr: Mutex<HashMap<u16, SocketAddr>>,
    // peer address -> channel
    addr_to_ch: Mutex<HashMap<SocketAddr, u16>>,
}

impl TurnHandlerImpl {
    pub fn new() -> Self {
        Self {
            ch_to_addr: Mutex::new(HashMap::new()),
            addr_to_ch: Mutex::new(HashMap::new()),
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

    // Parse ChannelData message; returns (channel, payload_len, padding_len)
    fn parse_header(packet: &[u8]) -> Option<(u16, usize, usize)> {
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
}

impl Default for TurnHandlerImpl { fn default() -> Self { Self::new() } }

impl TurnHandler for TurnHandlerImpl {
    fn handle_call(&self, source: &SocketAddr) -> i32 {
        // If already have a channel for this source, return it
        if let Ok(map) = self.addr_to_ch.lock() {
            if let Some(ch) = map.get(source).cloned() { return ch as i32; }
        } else { return -1; }
        // Allocate new channel
        let ch_opt = self.alloc_channel();
        let ch = match ch_opt { Some(v) => v, None => return -1 };
        // Insert into maps
        if let (Ok(mut c2a), Ok(mut a2c)) = (self.ch_to_addr.lock(), self.addr_to_ch.lock()) {
            c2a.insert(ch, *source);
            a2c.insert(*source, ch);
            ch as i32
        } else {
            -1
        }
    }

    fn handle_turn_incoming(&self, packet: &[u8]) -> Option<WrappedMessageWithAddress> {
        let (ch, len, _pad) = Self::parse_header(packet)?;
        let addr = {
            let map = self.ch_to_addr.lock().ok()?;
            map.get(&ch).cloned()?
        };
        let payload = packet[4..4+len].to_vec();
        Some(WrappedMessageWithAddress { ipAddress: addr, message: payload })
    }

    fn send_turn_outgoing(&self, dest: &SocketAddr, packet: &[u8]) -> Option<TurnMessageWithAddress> {
        let ch = {
            let map = self.addr_to_ch.lock().ok()?;
            map.get(dest).cloned()?
        };
        let msg = Self::build_channel_data(ch, packet)?;
        Some(TurnMessageWithAddress { ipAddress: *dest, message: msg })
    }
}
