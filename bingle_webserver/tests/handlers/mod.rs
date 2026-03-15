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

fn setup_state() -> AppState {
    AppState {
        api: Arc::new(MockBingleApi),
        messages: Arc::new(Mutex::new(Vec::new())),
    }
}

#[tokio::test]
async fn test_handle_lookup_success() {
    let state = setup_state();
    let query = Query(HandleQuery { handle: "foo".to_string() });
    let response = handle_lookup(State(state), query).await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
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
