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
use crate::try_start_api;
use bingle_local::api::bingle_local_api::{BingleLocalApi, ContactSource};

#[derive(Deserialize)]
pub struct HandleQuery {
    pub handle: String,
}

pub async fn handle_lookup(
    State(state): State<AppState>,
    Query(query): Query<HandleQuery>
) -> impl IntoResponse {
    let api = state.api.clone();
    let handle = query.handle;
    let result = tokio::task::spawn_blocking(move || {
        api.handle_lookup(&handle)
    }).await;
    match result {
        Ok(Ok(Some(id))) => Json(id).into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "Handle not found").into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {}", e)).into_response(),
    }
}

pub async fn send_message_to_id(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToIdRequest>
) -> impl IntoResponse {
    let api = state.api.clone();
    let local_api_arc = state.local_api.clone();
    let local_file = state.local_file.clone();
    let message_clone = payload.message.clone();
    let user_id_clone = payload.user_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        let ok = api.send_message_to_id(&payload.user_id, payload.message, None);
        if ok {
            if let Some(local_arc) = &local_api_arc {
                if let Ok(mut guard) = local_arc.lock() {
                    let text = message_clone.get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| message_clone.to_string());
                    let sender_handle = match api.get_handle() {
                        Some(h) => h,
                        None => {
                            tracing::error!("[send_message_to_id] api.get_handle() returned None; not saving sent message to local API");
                            return ok;
                        }
                    };
                    let recipient_handle = match api.handle_lookup_by_id(&user_id_clone) {
                        Some(h) => h,
                        None => {
                            tracing::error!("[send_message_to_id] handle_lookup_by_id returned None for user_id {}; not saving sent message to local API", user_id_clone);
                            return ok;
                        }
                    };
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    if let Err(e) = guard.add_message(
                        sender_handle,
                        vec![recipient_handle],
                        timestamp,
                        text,
                    ) {
                        tracing::warn!("[send_message_to_id] failed to add sent message to local API: {}", e);
                    }
                    if let Some(path) = &local_file {
                        let _ = guard.save(path.to_string_lossy().as_ref());
                    }
                }
            }
        }
        ok
    }).await;
    match result {
        Ok(ok) => Json(ok).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {}", e)).into_response(),
    }
}

pub async fn send_message_to_handle(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToHandleRequest>
) -> impl IntoResponse {
    let api = state.api.clone();
    let result = tokio::task::spawn_blocking(move || {
        api.send_message_to_handle(&payload.handle, payload.message, None)
    }).await;
    match result {
        Ok(ok) => Json(ok).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {}", e)).into_response(),
    }
}

pub async fn send_message_to_network(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToNetworkRequest>
) -> impl IntoResponse {
    let api = state.api.clone();
    let result = tokio::task::spawn_blocking(move || {
        let nsk: NetworkEndpoint = payload.network_source_key.into();
        api.send_message_to_network(&nsk, &payload.user_id, payload.message, None)
    }).await;
    match result {
        Ok(ok) => Json(ok).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {}", e)).into_response(),
    }
}

pub async fn send_message_to_id_with_response(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToIdRequest>
) -> impl IntoResponse {
    let api = state.api.clone();
    let result = tokio::task::spawn_blocking(move || {
        api.send_message_to_id_with_response(&payload.user_id, payload.message, None)
    }).await;
    match result {
        Ok(Ok(res)) => Json(res).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {}", e)).into_response(),
    }
}

pub async fn send_message_to_handle_with_response(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToHandleRequest>
) -> impl IntoResponse {
    let api = state.api.clone();
    let result = tokio::task::spawn_blocking(move || {
        api.send_message_to_handle_with_response(&payload.handle, payload.message, None)
    }).await;
    match result {
        Ok(Ok(res)) => Json(res).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {}", e)).into_response(),
    }
}

pub async fn send_message_to_network_with_response(
    State(state): State<AppState>,
    Json(payload): Json<SendMessageToNetworkRequest>
) -> impl IntoResponse {
    let api = state.api.clone();
    let result = tokio::task::spawn_blocking(move || {
        let nsk: NetworkEndpoint = payload.network_source_key.into();
        api.send_message_to_network_with_response(&nsk, &payload.user_id, payload.message, None)
    }).await;
    match result {
        Ok(Ok(res)) => Json(res).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {}", e)).into_response(),
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
    // Check local API is enabled before spawning blocking task
    if state.local_api.is_none() {
        return (StatusCode::METHOD_NOT_ALLOWED, "Local API disabled").into_response();
    }
    let result = tokio::task::spawn_blocking(move || {
        let local_arc = state.local_api.as_ref().unwrap();
        let api = match local_arc.lock() {
            Ok(g) => g,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Local API poisoned".to_string()).into_response(),
        };
        match api.register_keypair(req.handle) {
            Ok(_) => {
                drop(api); // release lock before save and start attempt
                save_if_configured(&state);
                try_start_api(&state);
                StatusCode::OK.into_response()
            }
            Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
        }
    }).await;
    match result {
        Ok(resp) => resp,
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {}", e)).into_response(),
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
        Ok(api) => match api.is_blocked(&q.id) {
            Ok(val) => AxumJson(val).into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
        },
        Err(resp) => resp,
    }
}

pub async fn local_get_contacts(State(state): State<AppState>) -> impl IntoResponse {
    match local_api_guard(&state) {
        Ok(api) => match api.get_contacts() {
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
        Ok(api) => match api.get_messages() {
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
        Ok(api) => {
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

pub async fn get_nat_type(State(state): State<AppState>) -> impl IntoResponse {
    let nat = state.nat_type.lock().unwrap();
    AxumJson(serde_json::json!({ "natType": *nat })).into_response()
}

pub async fn local_keypair_status(State(state): State<AppState>) -> impl IntoResponse {
    // Check local API is enabled before spawning blocking task
    if state.local_api.is_none() {
        return (StatusCode::METHOD_NOT_ALLOWED, "Local API disabled").into_response();
    }
    let result = tokio::task::spawn_blocking(move || {
        let local_arc = state.local_api.as_ref().unwrap();
        let api = match local_arc.lock() {
            Ok(g) => g,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Local API poisoned".to_string()).into_response(),
        };
        match api.keypair_status() {
            Ok(status) => AxumJson(status).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        }
    }).await;
    match result {
        Ok(resp) => resp,
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {}", e)).into_response(),
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
