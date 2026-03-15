use axum::{
    extract::{Query, Json, State},
    response::IntoResponse,
    http::StatusCode,
};
use serde::Deserialize;
use rust_comms::api::network_endpoint::NetworkEndpoint;

use crate::models::{
    SendMessageToIdRequest, SendMessageToHandleRequest, SendMessageToNetworkRequest,
};
use crate::AppState;

#[derive(Deserialize)]
pub struct HandleQuery {
    pub handle: String,
}

pub async fn handle_lookup(
    State(_state): State<AppState>,
    Query(query): Query<HandleQuery>
) -> impl IntoResponse {
    // Still a stub since BingleApi doesn't have a direct handleLookup method yet
    if query.handle == "notfound" {
        return (StatusCode::NOT_FOUND, "Handle not found").into_response();
    }
    Json(format!("stub-id-{}", query.handle)).into_response()
}

pub async fn send_message_to_id(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToIdRequest>
) -> impl IntoResponse {
    let ok = state.api.send_message_to_id(&payload.user_id, payload.message, None);
    Json(ok).into_response()
}

pub async fn send_message_to_handle(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToHandleRequest>
) -> impl IntoResponse {
    let ok = state.api.send_message_to_handle(&payload.handle, payload.message, None);
    Json(ok).into_response()
}

pub async fn send_message_to_network(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToNetworkRequest>
) -> impl IntoResponse {
    let nsk: NetworkEndpoint = payload.network_source_key.into();
    let ok = state.api.send_message_to_network(&nsk, &payload.user_id, payload.message, None);
    Json(ok).into_response()
}

pub async fn send_message_to_id_with_response(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToIdRequest>
) -> impl IntoResponse {
    match state.api.send_message_to_id_with_response(&payload.user_id, payload.message, None) {
        Ok(res) => Json(res).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub async fn send_message_to_handle_with_response(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToHandleRequest>
) -> impl IntoResponse {
    match state.api.send_message_to_handle_with_response(&payload.handle, payload.message, None) {
        Ok(res) => Json(res).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub async fn send_message_to_network_with_response(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToNetworkRequest>
) -> impl IntoResponse {
    let nsk: NetworkEndpoint = payload.network_source_key.into();
    match state.api.send_message_to_network_with_response(&nsk, &payload.user_id, payload.message, None) {
        Ok(res) => Json(res).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub async fn get_queued(State(state): State<AppState>) -> impl IntoResponse {
    let messages = state.messages.lock().unwrap();
    Json(messages.clone()).into_response()
}

pub async fn handle_version() -> impl IntoResponse {
    let version_info = rust_comms::util::version::get_version_info();
    Json(version_info).into_response()
}
