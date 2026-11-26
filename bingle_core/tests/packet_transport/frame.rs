//! FRPT wire format (spec §4, §9): exact layouts and parsing of malformed packets.

use bingle_core::packet_transport::frame::{
    FragHeader, Frame, build_ack_complete, build_ack_window, build_data_frag, build_data_single,
    parse,
};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn data_frag_has_the_twelve_byte_header_of_spec_9_1() {
    let header = FragHeader {
        tx_id: 0x0102,
        frag_index: 0x0304,
        frag_count: 0x0506,
        block_len: 0x0708_090A,
    };
    let packet = build_data_frag(&header, b"xy");
    assert_eq!(
        packet,
        vec![
            0x12, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, b'x', b'y'
        ]
    );
    assert_eq!(
        parse(&packet),
        Some(Frame::DataFrag {
            header,
            payload: b"xy"
        })
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ack_window_has_the_fourteen_byte_layout_of_spec_9_2() {
    let packet = build_ack_window(0x0A0B, 0x0040, 0x8000_0000_0000_0001);
    assert_eq!(
        packet,
        vec![
            0x13, 0x00, 0x0A, 0x0B, 0x00, 0x40, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01
        ]
    );
    assert_eq!(
        parse(&packet),
        Some(Frame::AckWindow {
            tx_id: 0x0A0B,
            base_index: 0x40,
            ack_bits: 0x8000_0000_0000_0001
        })
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn data_single_and_ack_complete_round_trip() {
    assert_eq!(
        parse(&build_data_single(7, b"hi")),
        Some(Frame::DataSingle {
            tx_id: 7,
            payload: b"hi"
        })
    );
    assert_eq!(
        parse(&build_ack_complete(9)),
        Some(Frame::AckComplete { tx_id: 9 })
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn malformed_frpt_packets_are_unsupported_not_panics() {
    // DATA_FRAG with a header but no payload.
    assert_eq!(
        parse(&[0x12, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 9]),
        Some(Frame::Unsupported)
    );
    // DATA_FRAG with a truncated header.
    assert_eq!(parse(&[0x12, 0, 0, 1, 0, 0]), Some(Frame::Unsupported));
    // ACK_WINDOW one byte short and one byte long.
    assert_eq!(parse(&[0x13; 13]), Some(Frame::Unsupported));
    assert_eq!(parse(&[0x13; 15]), Some(Frame::Unsupported));
    // ACK_COMPLETE with trailing bytes, and an unknown type.
    assert_eq!(parse(&[0x14, 0, 0, 1, 0]), Some(Frame::Unsupported));
    assert_eq!(parse(&[0x1F, 0, 0, 1]), Some(Frame::Unsupported));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn non_frpt_packets_are_passed_through_as_legacy() {
    assert_eq!(parse(br#"{"type":"ping"}"#), None);
    assert_eq!(parse(&[0x11, 0, 0]), None);
    assert_eq!(parse(&[0x21, 0, 0, 0]), None);
}
