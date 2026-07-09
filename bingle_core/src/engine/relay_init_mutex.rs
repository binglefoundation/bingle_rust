//! Construction of the distributed mutex used to serialize relay
//! initialization across peer relays.
//!
//! This keeps the message-transport wiring (turning the mutex's per-peer
//! `send` callbacks into Bingle messages sent through the API) out of the
//! main `Engine` body.

use super::BingleAccess;
use crate::api::bingle_api::BingleApiBothType;
use crate::distributed_mutex::ModifiedLamportDistributedMutex;
use crate::messages::types::{Message, MutexMessage, MutexRelease, MutexRequest, MutexResponse};

/// Build the relay-initialization mutex for `my_id` over the given participant
/// ids, wiring its request/reply/release callbacks to send Bingle messages via
/// the API.
///
/// The send callbacks return whether the message was delivered; the mutex uses
/// a failed delivery to prune the unreachable peer from its membership set.
pub(super) fn build(
    my_id: String,
    participant_ids: Vec<String>,
    bingle_api: BingleApiBothType,
) -> ModifiedLamportDistributedMutex {
    // Common transport: resolve the peer id via the API and send the JSON.
    // Returns whether the message was delivered.
    let send_common = {
        let api_weak = bingle_api;
        let my_id_for_send = my_id.clone();
        move |dest_id: &str, json_val: serde_json::Value| -> bool {
            let uid = dest_id.to_string();

            let ok = api_weak.access(|a| {
                a.send_message_to_id(&uid, json_val.clone(), None)
                    .unwrap_or(false)
            });
            if !ok {
                tracing::warn!(
                    "[Engine::initialize_relay][mutex] send_message_to_id failed for {} my_id={} json_val={}",
                    dest_id,
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
