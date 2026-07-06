use axum::{
    extract::{Json, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};

use crate::common::{HandleMockBingleApi, MockBingleApi};
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use bingle_webserver::AppState;
use bingle_webserver::handlers as web_handlers;
use bingle_webserver::handlers::{HandleQuery, handle_lookup, send_message_to_id};
use bingle_webserver::models::SendMessageToIdRequest;

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
    let query = Query(HandleQuery {
        handle: "foo".to_string(),
    });
    let response = handle_lookup(State(state), query).await.into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let id: String = serde_json::from_slice(&body).unwrap();
    assert_eq!(id, "mock-id-foo");
}

#[tokio::test]
async fn test_handle_lookup_not_found() {
    let state = setup_state();
    let query = Query(HandleQuery {
        handle: "notfound".to_string(),
    });
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
    let response = send_message_to_id(State(state), request)
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_handle_version() {
    let response = bingle_webserver::handlers::handle_version()
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_local_disabled_returns_405() {
    let state = setup_state();
    let response = web_handlers::local_generate_keypair(State(state))
        .await
        .into_response();
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
        local_api: Some(Arc::new(Mutex::new(
            Box::new(impl_api) as Box<dyn BingleLocalApi>
        ))),
        local_file: Some(file_path.clone()),
        start_opts: None,
        api_started: Arc::new(Mutex::new(true)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    };

    // Call generateKeypair which should also save
    let response = web_handlers::local_generate_keypair(State(state))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    // File should exist and be non-empty
    let meta = std::fs::metadata(&file_path).unwrap();
    assert!(meta.is_file());
    assert!(meta.len() > 0);
}

#[tokio::test]
async fn test_local_keypair_status_disabled_returns_405() {
    let state = setup_state();
    let response = web_handlers::local_keypair_status(State(state))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_local_keypair_status_no_keypair() {
    let impl_api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let state = AppState {
        api: Arc::new(MockBingleApi),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(
            Box::new(impl_api) as Box<dyn BingleLocalApi>
        ))),
        local_file: None,
        start_opts: None,
        api_started: Arc::new(Mutex::new(true)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    };

    let response = web_handlers::local_keypair_status(State(state))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "None");
    assert!(json.get("id").is_none());
    assert!(json.get("handle").is_none());
    assert!(json.get("requiredAlgo").is_none());
}

#[tokio::test]
async fn test_get_nat_type_returns_unknown_by_default() {
    let state = setup_state();
    let response = web_handlers::get_nat_type(State(state))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
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
    let response = web_handlers::get_nat_type(State(state))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["natType"], "FullCone");
}

fn setup_state_with_local_api() -> AppState {
    let impl_api = BingleApiLocalImpl::new(LocalApiConfig::default());
    AppState {
        api: Arc::new(MockBingleApi),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(
            Box::new(impl_api) as Box<dyn BingleLocalApi>
        ))),
        local_file: None,
        start_opts: None,
        api_started: Arc::new(Mutex::new(true)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    }
}

#[tokio::test]
async fn test_on_message_saves_to_local_api_get_messages() {
    let state = setup_state_with_local_api();

    // Simulate what the fixed on_message handler does: add message to local API
    {
        let local_arc = state.local_api.as_ref().expect("local_api should be Some");
        let mut guard = local_arc.lock().expect("local_api lock");
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        guard
            .add_message(
                "alice".to_string(),
                vec!["me".to_string()],
                timestamp,
                "hello from alice".to_string(),
                None,
            )
            .expect("add_message should succeed");
    }

    // Call local_get_messages endpoint and verify the message appears
    let response = web_handlers::local_get_messages(State(state))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let messages: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg["sender_handle"], "alice");
    assert_eq!(msg["text"], "hello from alice");
    let recipients = msg["recipient_handles"]
        .as_array()
        .expect("recipient_handles should be array");
    assert_eq!(recipients.len(), 1);
    assert_eq!(recipients[0], "me");
}

#[tokio::test]
async fn test_on_message_multiple_messages_accessible_via_get_messages() {
    let state = setup_state_with_local_api();

    // Simulate two received messages
    {
        let local_arc = state.local_api.as_ref().expect("local_api should be Some");
        let mut guard = local_arc.lock().expect("local_api lock");
        guard
            .add_message(
                "alice".to_string(),
                vec!["bob".to_string()],
                1000,
                "first message".to_string(),
                None,
            )
            .expect("add_message 1");
        guard
            .add_message(
                "charlie".to_string(),
                vec!["bob".to_string()],
                2000,
                "second message".to_string(),
                None,
            )
            .expect("add_message 2");
    }

    let response = web_handlers::local_get_messages(State(state))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let messages: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["sender_handle"], "alice");
    assert_eq!(messages[0]["text"], "first message");
    assert_eq!(messages[1]["sender_handle"], "charlie");
    assert_eq!(messages[1]["text"], "second message");
}

fn setup_state_with_handle_mock() -> AppState {
    let mut id_to_handle = std::collections::HashMap::new();
    id_to_handle.insert("target-user-id".to_string(), "target-handle".to_string());
    let mock = HandleMockBingleApi::new("sender-handle".to_string(), id_to_handle);
    let impl_api = BingleApiLocalImpl::new(LocalApiConfig::default());
    AppState {
        api: Arc::new(mock),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(
            Box::new(impl_api) as Box<dyn BingleLocalApi>
        ))),
        local_file: None,
        start_opts: None,
        api_started: Arc::new(Mutex::new(true)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    }
}

#[tokio::test]
async fn test_send_message_to_id_saves_to_local_api() {
    let state = setup_state_with_handle_mock();

    // Call send_message_to_id — HandleMockBingleApi returns true for send_message_to_id
    let request = Json(SendMessageToIdRequest {
        user_id: "target-user-id".to_string(),
        message: serde_json::json!({ "text": "hello from sender" }),
    });
    let response = send_message_to_id(State(state.clone()), request)
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    // Verify the sent message was saved into local API messages
    let response = web_handlers::local_get_messages(State(state))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let messages: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg["sender_handle"], "sender-handle");
    assert_eq!(msg["text"], "hello from sender");
    let recipients = msg["recipient_handles"]
        .as_array()
        .expect("recipient_handles should be array");
    assert_eq!(recipients.len(), 1);
    assert_eq!(recipients[0], "target-handle");
}

#[tokio::test]
async fn test_send_message_to_id_without_local_api_does_not_fail() {
    // Use setup_state (no local API) — should still succeed without saving
    let state = setup_state();
    let request = Json(SendMessageToIdRequest {
        user_id: "target-user-id".to_string(),
        message: serde_json::json!({ "text": "hello" }),
    });
    let response = send_message_to_id(State(state), request)
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_send_message_to_id_no_handle_does_not_save() {
    // MockBingleApi.get_handle() returns None — message should NOT be saved
    let state = setup_state_with_local_api();
    let request = Json(SendMessageToIdRequest {
        user_id: "target-user-id".to_string(),
        message: serde_json::json!({ "text": "hello" }),
    });
    let response = send_message_to_id(State(state.clone()), request)
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    // Verify no messages were saved
    let response = web_handlers::local_get_messages(State(state))
        .await
        .into_response();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let messages: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(
        messages.is_empty(),
        "message should not be saved when get_handle returns None"
    );
}

#[tokio::test]
async fn test_send_message_to_id_no_recipient_handle_does_not_save() {
    // HandleMockBingleApi with handle set but unknown user_id — handle_lookup_by_id returns None
    let mock = HandleMockBingleApi::new("sender".to_string(), std::collections::HashMap::new());
    let impl_api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let state = AppState {
        api: Arc::new(mock),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(
            Box::new(impl_api) as Box<dyn BingleLocalApi>
        ))),
        local_file: None,
        start_opts: None,
        api_started: Arc::new(Mutex::new(true)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    };
    let request = Json(SendMessageToIdRequest {
        user_id: "unknown-user-id".to_string(),
        message: serde_json::json!({ "text": "hello" }),
    });
    let response = send_message_to_id(State(state.clone()), request)
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    // Verify no messages were saved
    let response = web_handlers::local_get_messages(State(state))
        .await
        .into_response();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let messages: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(
        messages.is_empty(),
        "message should not be saved when handle_lookup_by_id returns None"
    );
}

#[tokio::test]
async fn test_get_messages_empty_when_no_messages_received() {
    let state = setup_state_with_local_api();

    let response = web_handlers::local_get_messages(State(state))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let messages: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(messages.is_empty());
}
