use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;
use rust_comms::api::bingle_api::BingleApi;
use crate::models::BingleMessage;
use std::path::PathBuf;
use bingle_local::api::bingle_local_api::BingleLocalApi;

pub mod handlers;
pub mod models;

#[derive(Clone)]
pub struct AppState {
    pub api: Arc<dyn BingleApi>,
    pub messages: Arc<Mutex<Vec<BingleMessage>>>,
    pub local_api: Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>,
    pub local_file: Option<PathBuf>,
}

pub fn create_router(state: AppState) -> Router {
    let router = Router::new()
        .route("/handleLookup", get(handlers::handle_lookup))
        .route("/sendMessageToId", post(handlers::send_message_to_id))
        .route("/sendMessageToHandle", post(handlers::send_message_to_handle))
        .route("/sendMessageToNetwork", post(handlers::send_message_to_network))
        .route("/sendMessageToIdWithResponse", post(handlers::send_message_to_id_with_response))
        .route("/sendMessageToHandleWithResponse", post(handlers::send_message_to_handle_with_response))
        .route("/sendMessageToNetworkWithResponse", post(handlers::send_message_to_network_with_response))
        .route("/queued", get(handlers::get_queued))
        .route("/version", get(handlers::handle_version))
        .layer(CorsLayer::permissive());

    // Always register local routes; handlers will return 405 if local API isn't enabled
    let router = router
        .route("/local/generateKeypair", post(handlers::local_generate_keypair))
        .route("/local/registerKeypair", post(handlers::local_register_keypair))
        .route("/local/addContact", post(handlers::local_add_contact))
        .route("/local/blockContact", post(handlers::local_block_contact))
        .route("/local/removeContact", post(handlers::local_remove_contact))
        .route("/local/isBlocked", get(handlers::local_is_blocked))
        .route("/local/getContacts", get(handlers::local_get_contacts))
        .route("/local/addMessage", post(handlers::local_add_message))
        .route("/local/getMessages", get(handlers::local_get_messages))
        .route("/local/save", post(handlers::local_save))
        .route("/local/load", post(handlers::local_load));

    router.with_state(state)
}

pub async fn start_server(addr: SocketAddr, state: AppState) -> anyhow::Result<()> {
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    log::info!("Listening on {}", addr);
    log::info!("Started. Press Ctrl-C to stop.");
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

    log::info!("Shutdown signal received.");
}
