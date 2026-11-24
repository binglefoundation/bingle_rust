//! FRPT wire format: building and parsing the four packet types (spec
//! `spec/reliable_packet_transport.md` §4). All integers are big-endian.

/// Protocol version carried in the high nibble of the first byte.
pub const FRPT_VERSION: u8 = 0x1;
pub const PACKET_TYPE_DATA_SINGLE: u8 = 0x1;
pub const PACKET_TYPE_DATA_FRAG: u8 = 0x2;
pub const PACKET_TYPE_ACK_WINDOW: u8 = 0x3;
pub const PACKET_TYPE_ACK_COMPLETE: u8 = 0x4;

/// Common prefix present in every FRPT packet: `ver_type`, `flags`, `tx_id`.
pub const FRPT_HEADER_LEN: usize = 4;
/// `DATA_FRAG` header: common prefix, `frag_index` u16, `frag_count` u16, `block_len` u32.
pub const DATA_FRAG_HEADER_LEN: usize = 12;
/// `ACK_WINDOW` packet: common prefix, `base_index` u16, `ack_bits` u64.
pub const ACK_WINDOW_LEN: usize = 14;
/// Fragments covered by one `ACK_WINDOW` and the sender's in-flight window.
pub const ACK_WINDOW_SPAN: usize = 64;

/// Largest block `PacketTransport::send` accepts (64 MiB). Sized for application data, such as a
/// compressed image or sound sent as one block; receivers protect themselves with reassembly
/// budgets rather than a smaller protocol cap.
pub const PACKET_MAX: usize = 64 * 1024 * 1024;

/// The metadata carried by one `DATA_FRAG` packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FragHeader {
    pub tx_id: u16,
    pub frag_index: u16,
    pub frag_count: u16,
    pub block_len: u32,
}

/// One parsed FRPT packet.
#[derive(Debug, PartialEq, Eq)]
pub enum Frame<'a> {
    DataSingle {
        tx_id: u16,
        payload: &'a [u8],
    },
    DataFrag {
        header: FragHeader,
        payload: &'a [u8],
    },
    AckWindow {
        tx_id: u16,
        base_index: u16,
        ack_bits: u64,
    },
    AckComplete {
        tx_id: u16,
    },
    /// An FRPT version-1 packet of an unknown type or a malformed length: dropped silently.
    Unsupported,
}

pub fn build_header(packet_type: u8, tx_id: u16) -> [u8; FRPT_HEADER_LEN] {
    let tx = tx_id.to_be_bytes();
    [
        ((FRPT_VERSION & 0x0F) << 4) | (packet_type & 0x0F),
        0,
        tx[0],
        tx[1],
    ]
}

pub fn build_data_single(tx_id: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRPT_HEADER_LEN + payload.len());
    out.extend_from_slice(&build_header(PACKET_TYPE_DATA_SINGLE, tx_id));
    out.extend_from_slice(payload);
    out
}

pub fn build_data_frag(header: &FragHeader, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(DATA_FRAG_HEADER_LEN + payload.len());
    out.extend_from_slice(&build_header(PACKET_TYPE_DATA_FRAG, header.tx_id));
    out.extend_from_slice(&header.frag_index.to_be_bytes());
    out.extend_from_slice(&header.frag_count.to_be_bytes());
    out.extend_from_slice(&header.block_len.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn build_ack_window(tx_id: u16, base_index: u16, ack_bits: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(ACK_WINDOW_LEN);
    out.extend_from_slice(&build_header(PACKET_TYPE_ACK_WINDOW, tx_id));
    out.extend_from_slice(&base_index.to_be_bytes());
    out.extend_from_slice(&ack_bits.to_be_bytes());
    out
}

pub fn build_ack_complete(tx_id: u16) -> [u8; FRPT_HEADER_LEN] {
    build_header(PACKET_TYPE_ACK_COMPLETE, tx_id)
}

/// Parses an inbound packet. `None` means it is not an FRPT packet (a legacy, unframed message,
/// which the caller passes straight up); `Some(Frame::Unsupported)` is an FRPT packet to drop.
pub fn parse(packet: &[u8]) -> Option<Frame<'_>> {
    if packet.len() < FRPT_HEADER_LEN {
        return None;
    }

    let ver_type = packet[0];
    if ver_type >> 4 != FRPT_VERSION {
        return None;
    }

    let tx_id = u16::from_be_bytes([packet[2], packet[3]]);
    let frame = match ver_type & 0x0F {
        PACKET_TYPE_DATA_SINGLE => Frame::DataSingle {
            tx_id,
            payload: &packet[FRPT_HEADER_LEN..],
        },
        PACKET_TYPE_DATA_FRAG if packet.len() > DATA_FRAG_HEADER_LEN => Frame::DataFrag {
            header: FragHeader {
                tx_id,
                frag_index: u16::from_be_bytes([packet[4], packet[5]]),
                frag_count: u16::from_be_bytes([packet[6], packet[7]]),
                block_len: u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]),
            },
            payload: &packet[DATA_FRAG_HEADER_LEN..],
        },
        PACKET_TYPE_ACK_WINDOW if packet.len() == ACK_WINDOW_LEN => {
            let mut bits = [0u8; 8];
            bits.copy_from_slice(&packet[6..14]);
            Frame::AckWindow {
                tx_id,
                base_index: u16::from_be_bytes([packet[4], packet[5]]),
                ack_bits: u64::from_be_bytes(bits),
            }
        }
        PACKET_TYPE_ACK_COMPLETE if packet.len() == FRPT_HEADER_LEN => Frame::AckComplete { tx_id },
        _ => Frame::Unsupported,
    };
    Some(frame)
}
