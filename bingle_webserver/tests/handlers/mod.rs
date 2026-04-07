use axum::{
    extract::{Query, Json, State},
    response::IntoResponse,
    http::StatusCode,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};

use bingle_webserver::handlers::{handle_lookup, HandleQuery, send_message_to_id};
use bingle_webserver::models::{SendMessageToIdRequest};
use bingle_webserver::AppState;
use crate::common::MockBingleApi;
use bingle_webserver::handlers as web_handlers;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use bingle_local::api::bingle_local_api::BingleLocalApi;

fn setup_state() -> AppState {
    AppState {
        api: Arc::new(MockBingleApi),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: None,
        local_file: None,
        start_opts: None,
        api_started: Arc::new(Mutex::new(true)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    }
}

#[tokio::test]
async fn test_handle_lookup_success() {
    let state = setup_state();
    let query = Query(HandleQuery { handle: "foo".to_string() });
    let response = handle_lookup(State(state), query).await.into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let id: String = serde_json::from_slice(&body).unwrap();
    assert_eq!(id, "mock-id-foo");
}

#[tokio::test]
async fn test_handle_lookup_not_found() {
    let state = setup_state();
    let query = Query(HandleQuery { handle: "notfound".to_string() });
    let response = handle_lookup(State(state), query).await.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_send_message_to_id() {
    let state = setup_state();
    let request = Json(SendMessageToIdRequest {
        user_id: "test-user".to_string(),
        message: Value::String("hello".to_string()),
    });
    let response = send_message_to_id(State(state), request).await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_handle_version() {
    let response = bingle_webserver::handlers::handle_version().await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_local_disabled_returns_405() {
    let state = setup_state();
    let response = web_handlers::local_generate_keypair(State(state)).await.into_response();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_local_generate_keypair_saves_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("state.json");

    // Prepare local API
    let impl_api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let state = AppState {
        api: Arc::new(MockBingleApi),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(Box::new(impl_api) as Box<dyn BingleLocalApi>))),
        local_file: Some(file_path.clone()),
        start_opts: None,
        api_started: Arc::new(Mutex::new(true)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    };

    // Call generateKeypair which should also save
    let response = web_handlers::local_generate_keypair(State(state)).await.into_response();
    assert_eq!(response.status(), StatusCode::OK);

    // File should exist and be non-empty
    let meta = std::fs::metadata(&file_path).unwrap();
    assert!(meta.is_file());
    assert!(meta.len() > 0);
}

#[tokio::test]
async fn test_local_keypair_status_disabled_returns_405() {
    let state = setup_state();
    let response = web_handlers::local_keypair_status(State(state)).await.into_response();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_local_keypair_status_no_keypair() {
    let impl_api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let state = AppState {
        api: Arc::new(MockBingleApi),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(Box::new(impl_api) as Box<dyn BingleLocalApi>))),
        local_file: None,
        start_opts: None,
        api_started: Arc::new(Mutex::new(true)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    };

    let response = web_handlers::local_keypair_status(State(state)).await.into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "None");
    assert!(json.get("id").is_none());
    assert!(json.get("handle").is_none());
    assert!(json.get("requiredAlgo").is_none());
}

#[tokio::test]
async fn test_get_nat_type_returns_unknown_by_default() {
    let state = setup_state();
    let response = web_handlers::get_nat_type(State(state)).await.into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["natType"], "Unknown");
}

#[tokio::test]
async fn test_get_nat_type_reflects_updated_value() {
    let state = setup_state();
    // Simulate the on_listening handler updating nat_type
    {
        let mut guard = state.nat_type.lock().unwrap();
        *guard = "FullCone".to_string();
    }
    let response = web_handlers::get_nat_type(State(state)).await.into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["natType"], "FullCone");
}
