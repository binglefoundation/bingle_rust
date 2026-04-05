use axum::http::StatusCode;
use bingle_webserver::{create_router, AppState};
use tower::util::ServiceExt;
use axum::body::Body;
use axum::http::Request;
use serde_json::json;
use std::sync::{Arc, Mutex};
use crate::common::MockBingleApi;

fn setup_state() -> AppState {
    AppState {
        api: Arc::new(MockBingleApi),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: None,
        local_file: None,
    }
}

#[tokio::test]
async fn test_full_handle_lookup() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/handleLookup?handle=foo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_full_handle_lookup_not_found() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/handleLookup?handle=notfound")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_full_send_message_to_id() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sendMessageToId")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({
                    "userId": "test-user",
                    "message": "hello"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_full_send_message_to_handle() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sendMessageToHandle")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({
                    "handle": "test-handle",
                    "message": "hello"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_full_send_message_to_network() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sendMessageToNetwork")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({
                    "networkSourceKey": {
                        "inetSocketAddress": {
                            "host": "127.0.0.1",
                            "port": 1234
                        }
                    },
                    "userId": "test-user",
                    "message": "hello"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_full_send_message_to_id_with_response() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sendMessageToIdWithResponse")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({
                    "userId": "test-user",
                    "message": "hello"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_full_send_message_to_handle_with_response() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sendMessageToHandleWithResponse")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({
                    "handle": "test-handle",
                    "message": "hello"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_full_send_message_to_network_with_response() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sendMessageToNetworkWithResponse")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({
                    "networkSourceKey": {
                        "inetSocketAddress": {
                            "host": "127.0.0.1",
                            "port": 1234
                        }
                    },
                    "userId": "test-user",
                    "message": "hello"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_queued() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/queued")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
