use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use rust_comms::turn::turn_handler::{TurnHandler, TurnHandlerImpl, TurnRelayHandler};

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[test]
fn unit_turn_incoming_rejected_without_listen() {
    let handler = TurnHandlerImpl::new();
    let src = addr(7001);
    let dst = addr(7000);
    // Allocate a channel for (src,dst)
    let ch = TurnRelayHandler::handle_call(&handler, &src, &dst);
    assert!(ch >= 0);
    // Build a small payload and wrap
    let payload = b"xyz";
    let wrapped = handler.send_turn_outgoing(&src, &dst, payload).expect("wrap outgoing");
    // Now, since we never called handle_listen, incoming should be rejected (src ip not registered)
    let incoming = handler.handle_turn_incoming(None, &wrapped.message);
    assert!(incoming.is_none(), "expected rejection before listen registration");
}

#[test]
fn unit_turn_incoming_accepted_after_listen() {
    let handler = TurnHandlerImpl::new();
    let src = addr(7101);
    let dst = addr(7100);
    // Register listen for destination IP (callee)
    let ok = handler.handle_listen("DSTID", &dst);
    assert!(ok);
    // Allocate a channel for (src,dst)
    let ch = TurnRelayHandler::handle_call(&handler, &src, &dst);
    assert!(ch >= 0);
    // Wrap a payload
    let payload = b"abcd"; // len 4, no padding
    let wrapped = handler.send_turn_outgoing(&src, &dst, payload).expect("wrap outgoing");
    // Incoming should now be accepted
    let incoming = handler.handle_turn_incoming(None, &wrapped.message);
    assert!(incoming.is_some(), "expected acceptance after listen registration");
    let incoming = incoming.unwrap();
    assert_eq!(incoming.ip_address, dst);
    assert_eq!(incoming.message, payload);
}
