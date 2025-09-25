use rust_comms::messages::*;

fn decode(input: &str) -> Message {
    from_json_str(input).expect("decode")
}

#[test]
fn integration_decode_plain_text() {
    let msg = decode("{\"text\":\"hi\"}");
    match msg {
        Message::PlainText(pt) => assert_eq!(pt.text, "hi"),
        _ => panic!("expected PlainText"),
    }
}

#[test]
fn integration_decode_relay_call() {
    let msg = decode("{\"app\":null,\"type\":\"Call\",\"calledId\":\"abc\"}");
    match msg {
        Message::Relay(RelayMessage::Call(m)) => assert_eq!(m.calledId, "abc"),
        _ => panic!("expected Relay Call"),
    }
}

#[test]
fn integration_decode_relay_response() {
    let msg = decode("{\"app\":null,\"type\":\"RelayResponse\",\"channel\":5}");
    match msg {
        Message::Relay(RelayMessage::RelayResponse(m)) => assert_eq!(m.channel, Some(5)),
        _ => panic!("expected RelayResponse"),
    }
}

#[test]
fn integration_decode_triangle_test1() {
    let msg = decode("{\"app\":null,\"type\":\"TriangleTest1\",\"checkingEndpoint\":\"127.0.0.1:3456\"}");
    match msg {
        Message::Relay(RelayMessage::TriangleTest1(m)) => assert_eq!(m.checkingEndpoint.to_string(), "127.0.0.1:3456"),
        _ => panic!("expected TriangleTest1"),
    }
}

#[test]
fn integration_decode_triangle_test2() {
    let msg = decode("{\"app\":null,\"type\":\"TriangleTest2\",\"checkingId\":\"id1\",\"checkingEndpoint\":\"10.0.0.1:1111\"}");
    match msg {
        Message::Relay(RelayMessage::TriangleTest2(m)) => {
            assert_eq!(m.checkingId, "id1");
            assert_eq!(m.checkingEndpoint.to_string(), "10.0.0.1:1111");
        }
        _ => panic!("expected TriangleTest2"),
    }
}

#[test]
fn integration_decode_triangle_test3() {
    let msg = decode("{\"app\":null,\"type\":\"TriangleTest3\"}");
    match msg {
        Message::Relay(RelayMessage::TriangleTest3(_)) => {}
        _ => panic!("expected TriangleTest3"),
    }
}

#[test]
fn integration_decode_listen() {
    let msg = decode("{\"app\":null,\"type\":\"Listen\"}");
    match msg {
        Message::Relay(RelayMessage::Listen(_)) => {}
        _ => panic!("expected Listen"),
    }
}

#[test]
fn integration_decode_check() {
    let msg = decode("{\"app\":null,\"type\":\"Check\"}");
    match msg {
        Message::Relay(RelayMessage::Check(_)) => {}
        _ => panic!("expected Check"),
    }
}

#[test]
fn integration_decode_listen_response() {
    let msg = decode("{\"app\":null,\"type\":\"ListenResponse\"}");
    match msg {
        Message::Relay(RelayMessage::ListenResponse(_)) => {}
        _ => panic!("expected ListenResponse"),
    }
}

#[test]
fn integration_decode_check_response() {
    let msg = decode("{\"app\":null,\"type\":\"CheckResponse\",\"available\":true}");
    match msg {
        Message::Relay(RelayMessage::CheckResponse(m)) => assert!(m.available),
        _ => panic!("expected CheckResponse"),
    }
}

#[test]
fn integration_decode_call_response() {
    let msg = decode("{\"app\":null,\"type\":\"CallResponse\",\"calledId\":\"x\",\"channel\":42}");
    match msg {
        Message::Relay(RelayMessage::CallResponse(m)) => {
            assert_eq!(m.calledId, "x");
            assert_eq!(m.channel, 42);
        }
        _ => panic!("expected CallResponse"),
    }
}

#[test]
fn integration_decode_keep_alive() {
    let msg = decode("{\"app\":null,\"type\":\"KeepAlive\"}");
    match msg {
        Message::Relay(RelayMessage::KeepAlive(_)) => {}
        _ => panic!("expected KeepAlive"),
    }
}

#[test]
fn integration_unimplemented_handler_prints_without_panic() {
    // Marshal to JSON and route using DefaultPrintingHandler; ensure no panic
    let msg = Message::Relay(RelayMessage::Check(RelayCheck { app: None }));
    let _json = to_json_string(&msg);
    let handler = DefaultPrintingHandler;
    // Should simply print unimplemented message; we just ensure it runs
    route(&handler, &msg);
}
