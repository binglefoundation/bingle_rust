use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use rust_comms::turn::turn_handler::{TurnHandler, TurnClientImpl, TurnClientHandler};

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[test]
fn unit_turn_client_handle_called_and_send() {
    let client = TurnClientImpl::new();
    let src = addr(8001);
    let dst = addr(8000);
    // Allow source ip
    assert!(client.handle_listen(&src));

    // Relay informs client of an incoming call mapping (source -> dest) with a known channel
    let ch: u16 = 0x4001;
    TurnClientHandler::handle_called(&client, &src, &dst, ch);

    // Send an outgoing packet from source to dest using this mapping
    let payload = b"hello";
    let wrapped = client.send_turn_outgoing(&src, &dst, payload);
    assert!(wrapped.is_some(), "expected wrapped outgoing");
    let wrapped = wrapped.unwrap();

    // Verify ChannelData header carries the expected channel
    let msg = wrapped.message;
    assert!(msg.len() >= 4);
    let ch_be = u16::from_be_bytes([msg[0], msg[1]]);
    let len = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    assert_eq!(ch_be, ch);
    assert_eq!(len, payload.len());

    // Feed back into incoming path; should attribute to the source address
    let incoming = client.handle_turn_incoming(&msg);
    assert!(incoming.is_some(), "expected incoming parse");
    let incoming = incoming.unwrap();
    assert_eq!(incoming.ipAddress, src);
    assert_eq!(incoming.message, payload);
}

#[test]
fn unit_turn_client_handle_call_response_and_send() {
    let client = TurnClientImpl::new();
    let src = addr(8101);
    let dst = addr(8100);
    // Allow source ip
    assert!(client.handle_listen(&src));

    // After we call, we receive a CallResponse indicating channel
    let ch: u16 = 0x4002;
    TurnClientHandler::handle_call_response(&client, &src, &dst, ch);

    // Now we can send to dest using the established channel
    let payload = b"world";
    let wrapped = client.send_turn_outgoing(&src, &dst, payload);
    assert!(wrapped.is_some(), "expected wrapped outgoing after CallResponse");
    let wrapped = wrapped.unwrap();

    // Verify header
    let msg = wrapped.message;
    assert!(msg.len() >= 4);
    let ch_be = u16::from_be_bytes([msg[0], msg[1]]);
    let len = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    assert_eq!(ch_be, ch);
    assert_eq!(len, payload.len());

    // Incoming parse
    let incoming = client.handle_turn_incoming(&msg);
    assert!(incoming.is_some());
    let incoming = incoming.unwrap();
    assert_eq!(incoming.ipAddress, src);
    assert_eq!(incoming.message, payload);
}
