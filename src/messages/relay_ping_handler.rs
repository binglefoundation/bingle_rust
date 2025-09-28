use std::net::SocketAddr;
use std::sync::Arc;

use crate::dtls::dtls_trait::Dtls;
use crate::messages::handlers::MessageHandler;
use crate::messages::marshal::to_json_string;
use crate::messages::types::*;

/// RelayPingHandler implements the relay triangle test behaviors:
/// - On TriangleTest1: if a peer relay address is configured, send TriangleTest2 to that peer.
/// - On TriangleTest2: send TriangleTest3 to the provided checkingEndpoint.
///
/// Note: We currently do not have identity wiring; checkingId is populated with an empty string.
#[derive(Clone)]
pub struct RelayPingHandler {
    pub dtls: Arc<dyn Dtls>,
    pub peer_relay: Option<SocketAddr>,
}

impl RelayPingHandler {
    pub fn new(dtls: Arc<dyn Dtls>, peer_relay: Option<SocketAddr>) -> Self {
        Self { dtls, peer_relay }
    }

    fn send_json_to(&self, to: SocketAddr, msg: &Message) {
        let json = to_json_string(msg);
        self.dtls.send(to, json.as_bytes()).expect("DTLS send failed in RelayPingHandler");
    }
}

impl MessageHandler for RelayPingHandler {
    fn on_triangle_test1(&self, from_id: &str, msg: &RelayTriangleTest1) {
        if let Some(to_peer) = self.peer_relay {
            // Compose TriangleTest2 to the peer relay, echoing the checkingEndpoint.
            let t2 = RelayTriangleTest2 { app: None, checkingId: from_id.to_string(), checkingEndpoint: msg.checkingEndpoint };
            let out = Message::Relay(RelayMessage::TriangleTest2(t2));
            self.send_json_to(to_peer, &out);
        } else {
            // Default behavior if no peer relay configured: fall back to unimplemented print
            self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest1(msg.clone())));
        }
    }

    fn on_triangle_test2(&self, _from_id: &str, msg: &RelayTriangleTest2) {
        // Send TriangleTest3 to the node at checkingEndpoint
        let t3 = RelayTriangleTest3 { app: None };
        let out = Message::Relay(RelayMessage::TriangleTest3(t3));
        self.send_json_to(msg.checkingEndpoint, &out);
    }
}
