#[cfg(feature = "test-hooks")]
use std::net::SocketAddr;
#[cfg(feature = "test-hooks")]
use std::sync::Arc;

#[cfg(feature = "test-hooks")]
use crate::dtls::dtls_trait::Dtls;
#[cfg(feature = "test-hooks")]
use crate::messages::handlers::MessageHandler;
#[cfg(feature = "test-hooks")]
use crate::messages::marshal::to_json_string;
#[cfg(feature = "test-hooks")]
use crate::messages::types::*;

/// RelayPingHandler implements the relay triangle test behaviors:
/// - On TriangleTest1: if a peer relay address is configured, send TriangleTest2 to that peer.
/// - On TriangleTest2: send TriangleTest3 to the provided checkingEndpoint.
///
/// checkingId is the id of the node being checked: the TriangleTest1 sender.
#[cfg(feature = "test-hooks")]
#[derive(Clone)]
pub struct RelayPingHandler {
    pub dtls: Arc<dyn Dtls>,
    pub peer_relay: Option<SocketAddr>,
}

#[cfg(feature = "test-hooks")]
impl RelayPingHandler {
    pub fn new(dtls: Arc<dyn Dtls>, peer_relay: Option<SocketAddr>) -> Self {
        Self { dtls, peer_relay }
    }

    fn send_json_to(&self, to: SocketAddr, msg: &Message) {
        let json = to_json_string(msg);
        let nsk = crate::api::bingle_api::NetworkEndpoint::new_direct(to);
        self.dtls
            .send(&nsk, json.as_bytes())
            .expect("DTLS send failed in RelayPingHandler");
    }
}

#[cfg(feature = "test-hooks")]
impl MessageHandler for RelayPingHandler {
    fn on_triangle_test1(
        &self,
        _api: Arc<dyn crate::api::bingle_api::BingleApiBoth>,
        from: &crate::messages::handlers::FromStruct,
        msg: &RelayTriangleTest1,
    ) {
        if let Some(to_peer) = self.peer_relay {
            // Honor do_not_use_endpoints: if our peer_relay is excluded, do not use it.
            let is_excluded = msg.do_not_use_endpoints.iter().any(|ie| {
                use std::convert::TryInto;
                ie.clone()
                    .try_into()
                    .map(|addr: SocketAddr| addr == to_peer)
                    .unwrap_or(false)
            });
            if is_excluded {
                tracing::warn!(
                    "[RelayPingHandler::on_triangle_test1] configured peer relay {} is in do_not_use_endpoints; skipping",
                    to_peer
                );
                return;
            }

            // Compose TriangleTest2 to the peer relay, echoing the checking_endpoint. checking_id is
            // the id of the node being checked (our sender), which the peer relay sends
            // TriangleTest3 to.
            let t2 = RelayTriangleTest2 {
                app: None,
                checking_id: from
                    .id
                    .trim_end_matches(crate::protocol::ISSUER_SUFFIX)
                    .to_string(),
                checking_endpoint: msg.checking_endpoint.clone(),
            };
            let out = Message::Relay(RelayMessage::TriangleTest2(t2));
            self.send_json_to(to_peer, &out);
        } else {
            // Default behavior if no peer relay configured: fall back to unimplemented print
            self.on_unimplemented(&Message::Relay(RelayMessage::TriangleTest1(msg.clone())));
        }
    }

    fn on_triangle_test2(
        &self,
        _api: Arc<dyn crate::api::bingle_api::BingleApiBoth>,
        _from: &crate::messages::handlers::FromStruct,
        msg: &RelayTriangleTest2,
    ) {
        // Send TriangleTest3 to the node at checking_endpoint
        use std::convert::TryInto;
        let to_addr: SocketAddr = msg
            .checking_endpoint
            .clone()
            .try_into()
            .expect("valid checkingEndpoint in T2");
        let t3 = RelayTriangleTest3 { app: None };
        let out = Message::Relay(RelayMessage::TriangleTest3(t3));
        self.send_json_to(to_addr, &out);
    }
}
