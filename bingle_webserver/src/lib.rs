use crate::models::BingleMessage;
use axum::{
    Router,
    routing::{get, post},
};
use bingle_local::api::bingle_local_api::BingleLocalApi;
use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::engine::BingleAccessUnsafeForTests;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;

pub mod handlers;
pub mod models;
pub mod module_version;

#[derive(Clone)]
pub struct AppState {
    pub api: Arc<dyn BingleApi>,
    pub messages: Arc<Mutex<Vec<BingleMessage>>>,
    pub local_api: Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>,
    pub local_file: Option<PathBuf>,
    pub start_opts: Option<StartOptions>,
    pub api_started: Arc<Mutex<bool>>,
    pub nat_type: Arc<Mutex<String>>,
}

/// Attempt to start the Bingle API if it hasn't been started yet and
/// the local keypair status is "ACTIVE". Call this after any operation
/// that might transition keypair_status to ACTIVE (e.g. register_keypair).
pub fn try_start_api(state: &AppState) {
    let Some(opts) = &state.start_opts else {
        return;
    };
    let mut started = state.api_started.lock().unwrap();
    if *started {
        return;
    }
    // Check keypair status via local_api
    if let Some(local_arc) = &state.local_api {
        if let Ok(guard) = local_arc.lock() {
            if let Ok(status) = guard.keypair_status() {
                if status.status == "ACTIVE" {
                    // Update handle and algo_passphrase from the local API's
                    // generated keypair and registered handle before starting.
                    let mut opts_clone = opts.clone();
                    if let Some(handle) = &status.handle {
                        opts_clone.handle = handle.clone();
                    }
                    if let Ok(Some(kp)) = guard.get_keypair() {
                        opts_clone.algo_passphrase = Some(kp.passphrase);
                    }
                    let api_clone = state.api.clone();
                    api_clone.access_unsafe_for_tests(|api_mut| {
                        if let Err(e) = api_mut.start(&opts_clone) {
                            tracing::error!("Failed to start Bingle API: {}", e);
                        } else {
                            tracing::info!("Bingle API started (keypair is ACTIVE)");
                        }
                    });
                    *started = true;
                }
            }
        }
    }
}

pub fn create_router(state: AppState) -> Router {
    let router = Router::new()
        .route("/handleLookup", get(handlers::handle_lookup))
        .route("/sendMessageToId", post(handlers::send_message_to_id))
        .route(
            "/sendMessageToHandle",
            post(handlers::send_message_to_handle),
        )
        .route(
            "/sendMessageToNetwork",
            post(handlers::send_message_to_network),
        )
        .route(
            "/sendMessageToIdWithResponse",
            post(handlers::send_message_to_id_with_response),
        )
        .route(
            "/sendMessageToHandleWithResponse",
            post(handlers::send_message_to_handle_with_response),
        )
        .route(
            "/sendMessageToNetworkWithResponse",
            post(handlers::send_message_to_network_with_response),
        )
        .route("/queued", get(handlers::get_queued))
        .route("/version", get(handlers::handle_version));

    // Always register local routes; handlers will return 405 if local API isn't enabled
    let router = router
        .route(
            "/local/generateKeypair",
            post(handlers::local_generate_keypair),
        )
        .route(
            "/local/registerKeypair",
            post(handlers::local_register_keypair),
        )
        .route("/local/addContact", post(handlers::local_add_contact))
        .route("/local/blockContact", post(handlers::local_block_contact))
        .route("/local/removeContact", post(handlers::local_remove_contact))
        .route("/local/isBlocked", get(handlers::local_is_blocked))
        .route("/local/getContacts", get(handlers::local_get_contacts))
        .route("/local/addMessage", post(handlers::local_add_message))
        .route("/local/getMessages", get(handlers::local_get_messages))
        .route("/local/keypairStatus", get(handlers::local_keypair_status))
        .route("/local/save", post(handlers::local_save))
        .route("/local/load", post(handlers::local_load));

    let router = router.route("/getNatType", get(handlers::get_nat_type));

    // Apply CORS layer to ALL routes (must be after all routes are registered)
    router.layer(CorsLayer::permissive()).with_state(state)
}

pub async fn start_server(addr: SocketAddr, state: AppState) -> anyhow::Result<()> {
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Listening on {}", addr);
    tracing::info!("Started. Press Ctrl-C to stop.");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received.");
}
