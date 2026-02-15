use std::fs;
use std::sync::Arc;

use rust_comms::api::bingle_api::{BingleApi, OnListeningHandler, StartOptions, BingleApiInternal};
use rust_comms::api::bingle_api_impl::BingleApiImpl;

#[test]
fn on_listening_handler_creates_and_deletes_sentinel() {
    // Create a temporary directory for the sentinel file
    let dir = tempfile::tempdir().expect("tempdir");
    let sentinel_path = dir.path().join("listening.sentinel");
    let sentinel_str = sentinel_path.to_string_lossy().to_string();

    // Set up API and install an OnListeningHandler that mirrors CLI behavior
    let api = BingleApiImpl::new(&StartOptions::default());
    let path_clone = sentinel_str.clone();
    let handler: Arc<OnListeningHandler> = Arc::new(move |listening: bool| {
        if listening {
            // Create or truncate the sentinel file
            if let Ok(mut f) = fs::OpenOptions::new().create(true).write(true).truncate(true).open(&path_clone) {
                use std::io::Write;
                let _ = writeln!(f, "listening");
            }
        } else {
            let _ = fs::remove_file(&path_clone);
        }
    });
    api.lock().unwrap().set_on_listening(Some(handler));

    // Notify true -> file should exist
    api.lock().unwrap().notify_listening(true);
    assert!(sentinel_path.exists(), "sentinel file should be created on listening=true");

    // Notify false -> file should be removed
    api.lock().unwrap().notify_listening(false);
    assert!(!sentinel_path.exists(), "sentinel file should be removed on listening=false");
}
