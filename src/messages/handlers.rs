use crate::messages::types::*;

pub trait MessageHandler {
    // Plain text
    fn on_plain_text(&self, from_id: &str, msg: &PlainTextMessage) {
        // Delegate to the Bingle API's on_message handler if one is installed.
        // We pass an empty sender id and use from_id (issuer) as the sender_handle to be consistent
        // with the API's DTLS handler behavior.
        let json = serde_json::to_value(msg).unwrap_or_else(|_| serde_json::json!({"text": msg.text}));
        crate::api::bingle_api_impl::global_on_message_call("".to_string(), from_id.to_string(), json);
    }

    // Relay messages
    fn on_relay_call(&self, _from_id: &str, _msg: &RelayCall) { self.on_unimplemented(&Message::Relay(RelayMessage::Call(_msg.clone()))); }
    fn on_relay_response(&self, _from_id: &str, _msg: &RelayResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::RelayResponse(_msg.clone()))); }
    fn on_triangle_test1(&self, _from_id: &str, _msg: &RelayTriangleTest1) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest1(_msg.clone()))); }
    fn on_triangle_test2(&self, _from_id: &str, _msg: &RelayTriangleTest2) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest2(_msg.clone()))); }
    fn on_triangle_test3(&self, _from_id: &str, _msg: &RelayTriangleTest3) { self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest3(_msg.clone()))); }
    fn on_relay_listen(&self, _from_id: &str, _msg: &RelayListen) { self.on_unimplemented(&Message::Relay(RelayMessage::Listen(_msg.clone()))); }
    fn on_relay_check(&self, _from_id: &str, _msg: &RelayCheck) { self.on_unimplemented(&Message::Relay(RelayMessage::Check(_msg.clone()))); }
    fn on_relay_listen_response(&self, _from_id: &str, _msg: &RelayListenResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::ListenResponse(_msg.clone()))); }
    fn on_relay_check_response(&self, _from_id: &str, _msg: &RelayCheckResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::CheckResponse(_msg.clone()))); }
    fn on_relay_call_response(&self, _from_id: &str, _msg: &RelayCallResponse) { self.on_unimplemented(&Message::Relay(RelayMessage::CallResponse(_msg.clone()))); }
    fn on_relay_keep_alive(&self, _from_id: &str, _msg: &RelayKeepAlive) { self.on_unimplemented(&Message::Relay(RelayMessage::KeepAlive(_msg.clone()))); }

    // Unknown
    fn on_unknown(&self, _raw: &serde_json::Value) {
        println!("[UNIMPLEMENTED] Unknown message: {}", _raw);
    }

    // Default unimplemented handler: prints the message JSON
    fn on_unimplemented(&self, msg: &Message) {
        println!("[UNIMPLEMENTED] {}", serde_json::to_string(&crate::messages::marshal::to_json_value(msg)).unwrap_or_else(|_| "<unprintable>".into()));
    }
}

pub struct DefaultPrintingHandler;

impl MessageHandler for DefaultPrintingHandler {}
