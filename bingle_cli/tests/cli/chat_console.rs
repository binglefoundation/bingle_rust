// Unit tests for the chat REPL's line editor setup and shared shutdown (issue #336):
// bingle_cli::chat_console. The terminal itself is not driven here; these cover what can be tested
// without one.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bingle_cli::chat_console::{ChatConsole, SessionShutdown, add_to_history, new_editor};
use bingle_cli::chat_state::ChatState;
use bingle_core::api::bingle_api::{BingleError, ProgressCallback, StartOptions};
use bingle_local::api::MailboxConfig;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use bingle_local::api::pending_sender::{MessageDelivery, PendingSender, PendingSenderOptions};
use rustyline::history::History;
use serde_json::Value;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_editor_keeps_session_history_in_memory() {
    let mut editor = new_editor().expect("editor");
    assert_eq!(editor.history().len(), 0, "history starts empty");
    add_to_history(&mut editor, "hello");
    add_to_history(&mut editor, "hello");
    add_to_history(&mut editor, "bye");
    // Consecutive duplicates are dropped.
    assert_eq!(editor.history().len(), 2);
    // History is the in-memory MemHistory (by type), with no file behind it; a new editor starts
    // empty again.
    assert_eq!(new_editor().expect("editor").history().len(), 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn blank_lines_are_not_added_to_history() {
    let mut editor = new_editor().expect("editor");
    add_to_history(&mut editor, "");
    add_to_history(&mut editor, "   ");
    assert_eq!(editor.history().len(), 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn plain_console_does_not_use_the_editor() {
    // Piped input (and the integration tests) read plain lines.
    assert!(!ChatConsole::plain().is_editing());
}

/// Delivery that never succeeds; the shutdown tests only flush, so it is never called.
struct NoDelivery;

impl MessageDelivery for NoDelivery {
    fn deliver(
        &self,
        _recipient: &str,
        _message: Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(false)
    }
}

/// A gated "alice" session with a state file, and a shutdown over it whose engine stop is counted.
fn session(dir: &tempfile::TempDir) -> (ChatState, Arc<SessionShutdown>, Arc<AtomicUsize>, String) {
    let mut local = BingleApiLocalImpl::new(LocalApiConfig {
        sidewinder: Some(MailboxConfig::discovered(1)),
        store_and_forward_send: true,
        ..LocalApiConfig::default()
    });
    local.generate_keypair().expect("keypair");
    local.seed_own_handle_for_tests("alice".to_string());
    let state = ChatState::from_parts_for_tests(local, StartOptions::new("alice".to_string()));
    let path = dir.path().join("state.json").to_string_lossy().into_owned();
    let sender = Arc::new(PendingSender::start(
        Arc::new(state.outbound_store()),
        Arc::new(NoDelivery),
        PendingSenderOptions {
            tick: Duration::from_millis(20),
            not_ready_timeout: Duration::from_secs(60),
            ..PendingSenderOptions::new()
        },
        // Never ready, so nothing is sent before the shutdown flush.
        Arc::new(|| false),
        Arc::new(|_| {}),
    ));
    let stops = Arc::new(AtomicUsize::new(0));
    let stops_in = Arc::clone(&stops);
    let shutdown = Arc::new(SessionShutdown::new(
        sender,
        state.outbound_store(),
        Box::new(move || {
            stops_in.fetch_add(1, Ordering::SeqCst);
        }),
        state.local_handle(),
        Some(path.clone()),
    ));
    (state, shutdown, stops, path)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn only_the_first_caller_claims_the_shutdown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (_state, shutdown, _stops, _path) = session(&dir);
    assert!(shutdown.claim());
    assert!(
        !shutdown.claim(),
        "a second Ctrl-C finds shutdown under way"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn shutdown_flushes_queued_messages_stops_the_engine_and_saves() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut state, shutdown, stops, path) = session(&dir);
    let ts = state.queue_outbound("bob", "bye").expect("queue");
    // Pre-marked posted (test seam), so the flush completes without a node.
    state.mark_forwarded_for_tests(&ts, "bob");

    let printed = Mutex::new(Vec::<String>::new());
    assert!(shutdown.claim());
    shutdown.run(&|line| printed.lock().expect("printed").push(line.to_string()));

    let printed = printed.into_inner().expect("printed");
    assert!(
        printed[0].starts_with("finishing 1 queued message(s)"),
        "{printed:?}"
    );
    assert!(
        printed.iter().any(|l| l.starts_with("↪ bob")),
        "{printed:?}"
    );
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert!(state.pending_outbound().expect("pending").is_empty());
    assert!(std::path::Path::new(&path).exists(), "state saved");
}
