//! Construction of the distributed mutex used to serialize relay
//! initialization across peer relays.
//!
//! This keeps the message-transport wiring (turning the mutex's per-peer
//! `send` callbacks into Bingle messages sent through the API) out of the
//! main `Engine` body.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::{BingleAccess, ConnectionEntry};
use crate::api::bingle_api::{BingleApiBothType, NetworkEndpoint, NetworkEndpointKey};
use crate::distributed_mutex::ModifiedLamportDistributedMutex;
use crate::messages::types::{Message, MutexMessage, MutexRelease, MutexRequest, MutexResponse};

/// Find the endpoint of the live connection a peer is currently on, if any.
///
/// A peer may appear on more than one connection entry over time (e.g. after an
/// IP change); we pick the one with the most recent activity so replies follow
/// the peer to its current address.
fn live_connection_endpoint(
    connections: &Mutex<HashMap<NetworkEndpointKey, ConnectionEntry>>,
    peer_id: &str,
) -> Option<NetworkEndpoint> {
    let m = connections.lock().ok()?;
    m.iter()
        .filter(|(_, e)| e.peer_id.as_deref() == Some(peer_id))
        .max_by_key(|(_, e)| e.last_seen)
        .and_then(|(k, _)| k.inet_socket_address)
        .map(NetworkEndpoint::new_direct)
}

/// Build the relay-initialization mutex for `my_id` over the given participant
/// ids, wiring its request/reply/release callbacks to send Bingle messages via
/// the API.
///
/// Replies are sent over the live connection the peer is currently on (looked
/// up in `connections`), reusing the established link instead of re-resolving
/// the id through the relay DDB — which fails for a peer that is still
/// initializing as a relay. When we have no connection to the peer, id
/// resolution is the only option and is used directly; it is deliberately not
/// used as a fallback after a connection send fails (for an initializing relay
/// it cannot succeed, and a genuine send failure is a real unreachability
/// signal the mutex should act on).
///
/// The send callbacks return whether the message was delivered; the mutex uses
/// a failed delivery to prune the unreachable peer from its membership set.
pub(super) fn build(
    my_id: String,
    participant_ids: Vec<String>,
    bingle_api: BingleApiBothType,
    connections: Arc<Mutex<HashMap<NetworkEndpointKey, ConnectionEntry>>>,
) -> ModifiedLamportDistributedMutex {
    // Common transport: send the JSON to the peer over its live connection when
    // we have one, else by id resolution. Returns whether it was delivered.
    let send_common = {
        let api_weak = bingle_api;
        let my_id_for_send = my_id.clone();
        move |dest_id: &str, json_val: serde_json::Value| -> bool {
            let uid = dest_id.to_string();
            let endpoint = live_connection_endpoint(&connections, &uid);
            let over_connection = endpoint.is_some();

            let ok = api_weak.access(|a| match &endpoint {
                Some(ep) => a
                    .send_message_to_network(ep, &uid, json_val.clone(), None)
                    .unwrap_or(false),
                None => a
                    .send_message_to_id(&uid, json_val.clone(), None)
                    .unwrap_or(false),
            });
            if !ok {
                tracing::warn!(
                    "[Engine::initialize_relay][mutex] send failed for {} (over_connection={}) my_id={} json_val={}",
                    dest_id,
                    over_connection,
                    my_id_for_send,
                    json_val
                );
            }
            ok
        }
    };

    let send_request = {
        let send_common = send_common.clone();
        move |dest_id: &str, req: &MutexRequest| -> bool {
            let msg = Message::Mutex(MutexMessage::Request(req.clone()));
            let json_val = crate::messages::marshal::to_json_value(&msg);
            send_common(dest_id, json_val)
        }
    };
    let send_reply = {
        let send_common = send_common.clone();
        move |dest_id: &str, resp: &MutexResponse| -> bool {
            let msg = Message::Mutex(MutexMessage::Response(resp.clone()));
            let json_val = crate::messages::marshal::to_json_value(&msg);
            send_common(dest_id, json_val)
        }
    };
    let send_release = {
        let send_common = send_common.clone();
        move |dest_id: &str, rel: &MutexRelease| -> bool {
            let msg = Message::Mutex(MutexMessage::Release(rel.clone()));
            let json_val = crate::messages::marshal::to_json_value(&msg);
            send_common(dest_id, json_val)
        }
    };

    ModifiedLamportDistributedMutex::new(
        my_id,
        participant_ids,
        send_request,
        send_reply,
        send_release,
    )
}
