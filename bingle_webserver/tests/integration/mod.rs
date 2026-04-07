use axum::http::StatusCode;
use bingle_webserver::{create_router, AppState};
use tower::util::ServiceExt;
use axum::body::Body;
use axum::http::Request;
use serde_json::json;
use std::sync::{Arc, Mutex};
use crate::common::MockBingleApi;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};

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

#[tokio::test]
async fn test_full_keypair_status_disabled() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/local/keypairStatus")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_full_keypair_status_no_keypair() {
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
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/local/keypairStatus")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "None");
    assert!(json.get("id").is_none());
}

#[tokio::test]
async fn test_full_get_nat_type_default() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/getNatType")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["natType"], "Unknown");
}

#[tokio::test]
async fn test_cors_headers_on_version() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/version")
                .header("Origin", "http://localhost:3000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let cors_header = response.headers().get("access-control-allow-origin");
    assert!(cors_header.is_some(), "CORS header missing on /version");
    assert_eq!(cors_header.unwrap(), "*");
}

#[tokio::test]
async fn test_cors_headers_on_local_keypair_status() {
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
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/local/keypairStatus")
                .header("Origin", "http://localhost:3000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let cors_header = response.headers().get("access-control-allow-origin");
    assert!(cors_header.is_some(), "CORS header missing on /local/keypairStatus");
    assert_eq!(cors_header.unwrap(), "*");
}

#[tokio::test]
async fn test_cors_preflight_on_local_route() {
    let app = create_router(setup_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/local/generateKeypair")
                .header("Origin", "http://localhost:3000")
                .header("Access-Control-Request-Method", "POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let cors_header = response.headers().get("access-control-allow-origin");
    assert!(cors_header.is_some(), "CORS header missing on OPTIONS preflight for /local/generateKeypair");
    assert_eq!(cors_header.unwrap(), "*");
}
