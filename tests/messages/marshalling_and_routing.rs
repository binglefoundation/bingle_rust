use crate::util::reusable_mock_api::MockApiBoth;
use rust_comms::messages::*;
use crate::util::test_util::init_test_logging;

fn decode(input: &str) -> Message {
    from_json_str(input).expect("decode")
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_plain_text() {
    let msg = decode("{\"text\":\"hi\"}");
    match msg {
        Message::PlainText(pt) => assert_eq!(pt.text, "hi"),
        _ => panic!("expected PlainText"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_relay_call() {
    let msg = decode("{\"app\":null,\"type\":\"Call\",\"calledId\":\"abc\"}");
    match msg {
        Message::Relay(RelayMessage::Call(m)) => assert_eq!(m.called_id, "abc"),
        _ => panic!("expected Relay Call"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_relay_response() {
    let msg = decode("{\"app\":null,\"type\":\"RelayResponse\",\"channel\":5}");
    match msg {
        Message::Relay(RelayMessage::RelayResponse(m)) => assert_eq!(m.channel, Some(5)),
        _ => panic!("expected RelayResponse"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_triangle_test1() {
    init_test_logging();
    let msg = decode("{\"app\":null,\"type\":\"TriangleTest1\",\"checkingEndpoint\":{\"host\":\"127.0.0.1\",\"port\":3456}}");
    tracing::debug!("{:?}", msg);
    match msg {
        Message::Relay(RelayMessage::TriangleTest1(m)) => assert_eq!(m.checking_endpoint.to_string(), "127.0.0.1:3456"),
        _ => panic!("expected TriangleTest1"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_triangle_test2() {
    let msg = decode("{\"app\":null,\"type\":\"TriangleTest2\",\"checkingId\":\"id1\",\"checkingEndpoint\":{\"host\":\"10.0.0.1\",\"port\":1111}}");
    match msg {
        Message::Relay(RelayMessage::TriangleTest2(m)) => {
            assert_eq!(m.checking_id, "id1");
            assert_eq!(m.checking_endpoint.to_string(), "10.0.0.1:1111");
        }
        _ => panic!("expected TriangleTest2"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_triangle_test3() {
    let msg = decode("{\"app\":null,\"type\":\"TriangleTest3\"}");
    match msg {
        Message::Relay(RelayMessage::TriangleTest3(_)) => {}
        _ => panic!("expected TriangleTest3"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_listen() {
    let msg = decode("{\"app\":null,\"type\":\"Listen\"}");
    match msg {
        Message::Relay(RelayMessage::Listen(_)) => {}
        _ => panic!("expected Listen"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_check() {
    let msg = decode("{\"app\":null,\"type\":\"Check\"}");
    match msg {
        Message::Relay(RelayMessage::Check(_)) => {}
        _ => panic!("expected Check"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_listen_response() {
    let msg = decode("{\"app\":null,\"type\":\"ListenResponse\"}");
    match msg {
        Message::Relay(RelayMessage::ListenResponse(_)) => {}
        _ => panic!("expected ListenResponse"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_check_response() {
    let msg = decode("{\"app\":null,\"type\":\"CheckResponse\",\"state\":\"available\"}");
    match msg {
        Message::Relay(RelayMessage::CheckResponse(m)) => assert_eq!(m.relay_state, "available"),
        _ => panic!("expected CheckResponse"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_call_response() {
    let msg = decode("{\"app\":null,\"type\":\"CallResponse\",\"calledId\":\"x\",\"channel\":42}");
    match msg {
        Message::Relay(RelayMessage::CallResponse(m)) => {
            assert_eq!(m.called_id, "x");
            assert_eq!(m.channel, 42);
        }
        _ => panic!("expected CallResponse"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_decode_keep_alive() {
    let msg = decode("{\"app\":null,\"type\":\"KeepAlive\"}");
    match msg {
        Message::Relay(RelayMessage::KeepAlive(_)) => {}
        _ => panic!("expected KeepAlive"),
    }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn integration_unimplemented_handler_prints_without_panic() {

    // Marshal to JSON and route using DefaultPrintingHandler; ensure no panic
    let msg = Message::Relay(RelayMessage::Check(RelayCheck { app: None }));
    let _json = to_json_string(&msg);
    let handler = DefaultPrintingHandler;
    // Should simply print unimplemented message; we just ensure it runs
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(crate::util::reusable_mock_api::to_weak(MockApiBoth::new())));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "FROMID");
    });
}
