use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use rust_comms::turn::turn_handler::{TurnHandler, TurnHandlerImpl};

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[test]
fn unit_turn_handle_call_allocates_in_range_and_reuses() {
    let handler = TurnHandlerImpl::new();
    let peer = addr(5000);

    let ch1 = handler.handle_call(&peer);
    assert!(ch1 >= 0, "channel should be non-negative");
    let ch1u = ch1 as u16;
    assert!(ch1u >= 0x4000 && ch1u <= 0x7FFE, "channel in TURN range: {:#X}", ch1u);

    let ch2 = handler.handle_call(&peer);
    assert_eq!(ch1, ch2, "channel must be reused for same peer");
}

#[test]
fn unit_turn_wraps_and_unwraps_channel_data_with_padding() {
    let handler = TurnHandlerImpl::new();
    let peer = addr(6000);
    let ch = handler.handle_call(&peer);
    assert!(ch >= 0);

    let payload = b"abc"; // len 3 -> padding 1 byte
    let wrapped = handler.send_turn_outgoing(&peer, payload);
    assert!(wrapped.is_some(), "expected wrapped message");
    let wrapped = wrapped.unwrap();

    // Validate ChannelData header
    let msg = wrapped.message;
    assert!(msg.len() >= 4);
    let ch_be = u16::from_be_bytes([msg[0], msg[1]]) as i32;
    let len = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    assert_eq!(ch_be, ch);
    assert_eq!(len, payload.len());
    // Padding to 4-byte boundary should be present
    let padded_len = (len + 3) & !3;
    assert_eq!(msg.len(), 4 + padded_len);

    // Now feed back to incoming parser
    let incoming = handler.handle_turn_incoming(&msg);
    assert!(incoming.is_some(), "expected incoming to parse");
    let incoming = incoming.unwrap();
    assert_eq!(incoming.ipAddress, peer);
    assert_eq!(incoming.message, payload);
}

#[test]
fn unit_turn_incoming_invalid_packets_return_none() {
    let handler = TurnHandlerImpl::new();
    // Too short
    assert!(handler.handle_turn_incoming(&[0x40]).is_none());
    // Declared len longer than actual
    let bad = [0x40, 0x00, 0x00, 0x10];
    assert!(handler.handle_turn_incoming(&bad).is_none());
}
