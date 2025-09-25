use crate::messages::handlers::MessageHandler;
use crate::messages::types::*;

pub fn route<H: MessageHandler + ?Sized>(handler: &H, msg: &Message) {
    match msg {
        Message::PlainText(pt) => handler.on_plain_text(pt),
        Message::Relay(r) => match r {
            RelayMessage::Call(m) => handler.on_relay_call(m),
            RelayMessage::RelayResponse(m) => handler.on_relay_response(m),
            RelayMessage::TriangleTest1(m) => handler.on_triangle_test1(m),
            RelayMessage::TriangleTest2(m) => handler.on_triangle_test2(m),
            RelayMessage::TriangleTest3(m) => handler.on_triangle_test3(m),
            RelayMessage::Listen(m) => handler.on_relay_listen(m),
            RelayMessage::Check(m) => handler.on_relay_check(m),
            RelayMessage::ListenResponse(m) => handler.on_relay_listen_response(m),
            RelayMessage::CheckResponse(m) => handler.on_relay_check_response(m),
            RelayMessage::CallResponse(m) => handler.on_relay_call_response(m),
            RelayMessage::KeepAlive(m) => handler.on_relay_keep_alive(m),
        },
        Message::Unknown(v) => handler.on_unknown(v),
    }
}
