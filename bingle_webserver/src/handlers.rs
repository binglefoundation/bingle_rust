use axum::{
    extract::{Query, Json, State},
    response::IntoResponse,
    http::StatusCode,
};
use serde::Deserialize;
use rust_comms::api::network_endpoint::NetworkEndpoint;
use axum::Json as AxumJson;
use axum::response::Response as AxumResponse;

use crate::models::{
    SendMessageToIdRequest, SendMessageToHandleRequest, SendMessageToNetworkRequest,
    RegisterKeypairRequest, AddContactRequest, IdRequest, AddMessageRequest, PathRequest,
};
use crate::AppState;
use bingle_local::api::bingle_local_api::{BingleLocalApi, ContactSource};

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

fn local_api_guard(state: &AppState) -> Result<std::sync::MutexGuard<'_, Box<dyn BingleLocalApi>>, AxumResponse> {
    let Some(local_arc) = &state.local_api else {
        return Err((StatusCode::METHOD_NOT_ALLOWED, "Local API disabled").into_response());
    };
    match local_arc.lock() {
        Ok(g) => Ok(g),
        Err(_) => Err((StatusCode::INTERNAL_SERVER_ERROR, "Local API poisoned").into_response()),
    }
}

fn save_if_configured(state: &AppState) {
    let (Some(local_arc), Some(path)) = (&state.local_api, &state.local_file) else { return; };
    if let Ok(guard) = local_arc.lock() {
        let _ = guard.save(path.to_string_lossy().as_ref());
    }
}

pub async fn local_generate_keypair(State(state): State<AppState>) -> impl IntoResponse {
    match local_api_guard(&state) {
        Ok(mut api) => {
            let res = api.generate_keypair();
            drop(api); // release lock before potential save
            match res {
                Ok(kp) => {
                    save_if_configured(&state);
                    AxumJson(kp).into_response()
                }
                Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
            }
        }
        Err(resp) => resp,
    }
}

pub async fn local_register_keypair(
    State(state): State<AppState>,
    Json(req): Json<RegisterKeypairRequest>,
) -> impl IntoResponse {
    match local_api_guard(&state) {
        Ok(mut api) => match api.register_keypair(req.handle) {
            Ok(ok) => AxumJson(ok).into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
        },
        Err(resp) => resp,
    }
}

pub async fn local_add_contact(
    State(state): State<AppState>,
    Json(req): Json<AddContactRequest>,
) -> impl IntoResponse {
    let source = match req.source.as_str() {
        "Manual" | "manual" => ContactSource::Manual,
        "Received" | "received" => ContactSource::Received,
        _ => return (StatusCode::BAD_REQUEST, "Invalid source").into_response(),
    };
    match local_api_guard(&state) {
        Ok(mut api) => {
            let res = api.add_contact(req.handle, req.id, source);
            drop(api);
            match res {
                Ok(_) => {
                    save_if_configured(&state);
                    StatusCode::OK.into_response()
                }
                Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
            }
        }
        Err(resp) => resp,
    }
}

pub async fn local_block_contact(
    State(state): State<AppState>,
    Json(req): Json<IdRequest>,
) -> impl IntoResponse {
    match local_api_guard(&state) {
        Ok(mut api) => {
            let res = api.block_contact(req.id);
            drop(api);
            match res {
                Ok(_) => {
                    save_if_configured(&state);
                    StatusCode::OK.into_response()
                }
                Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
            }
        }
        Err(resp) => resp,
    }
}

pub async fn local_remove_contact(
    State(state): State<AppState>,
    Json(req): Json<IdRequest>,
) -> impl IntoResponse {
    match local_api_guard(&state) {
        Ok(mut api) => {
            let res = api.remove_contact(req.id);
            drop(api);
            match res {
                Ok(_) => {
                    save_if_configured(&state);
                    StatusCode::OK.into_response()
                }
                Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
            }
        }
        Err(resp) => resp,
    }
}

#[derive(Deserialize)]
pub struct IdQuery { pub id: String }

pub async fn local_is_blocked(
    State(state): State<AppState>,
    Query(q): Query<IdQuery>,
) -> impl IntoResponse {
    match local_api_guard(&state) {
        Ok(mut api) => match api.is_blocked(&q.id) {
            Ok(val) => AxumJson(val).into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
        },
        Err(resp) => resp,
    }
}

pub async fn local_get_contacts(State(state): State<AppState>) -> impl IntoResponse {
    match local_api_guard(&state) {
        Ok(mut api) => match api.get_contacts() {
            Ok(list) => AxumJson(list).into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
        },
        Err(resp) => resp,
    }
}

pub async fn local_add_message(
    State(state): State<AppState>,
    Json(req): Json<AddMessageRequest>,
) -> impl IntoResponse {
    match local_api_guard(&state) {
        Ok(mut api) => {
            let res = api.add_message(
                req.sender_handle,
                req.recipient_handles,
                req.timestamp,
                req.text,
            );
            drop(api);
            match res {
                Ok(_) => {
                    save_if_configured(&state);
                    StatusCode::OK.into_response()
                }
                Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
            }
        }
        Err(resp) => resp,
    }
}

pub async fn local_get_messages(State(state): State<AppState>) -> impl IntoResponse {
    match local_api_guard(&state) {
        Ok(mut api) => match api.get_messages() {
            Ok(list) => AxumJson(list).into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
        },
        Err(resp) => resp,
    }
}

pub async fn local_save(
    State(state): State<AppState>,
    Json(req): Json<PathRequest>,
) -> impl IntoResponse {
    match local_api_guard(&state) {
        Ok(mut api) => {
            let res = api.save(&req.path);
            drop(api);
            match res {
                Ok(_) => StatusCode::OK.into_response(),
                Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
            }
        }
        Err(resp) => resp,
    }
}

pub async fn local_load(
    State(state): State<AppState>,
    Json(req): Json<PathRequest>,
) -> impl IntoResponse {
    match local_api_guard(&state) {
        Ok(mut api) => {
            let res = api.load(&req.path);
            drop(api);
            match res {
                Ok(_) => {
                    save_if_configured(&state);
                    StatusCode::OK.into_response()
                }
                Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
            }
        }
        Err(resp) => resp,
    }
}
