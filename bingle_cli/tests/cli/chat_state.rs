// Unit tests for the chat state-file bridge (bingle_cli::chat_state::ChatState).
use std::io::Write;
use std::sync::{Arc, Mutex};

use bingle_cli::chat::parse_chat_args;
use bingle_cli::chat_state::ChatState;
use bingle_local::api::bingle_local_api::{BingleLocalApi, ContactSource};
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use tempfile::TempDir;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Write a BingleLocal state file containing a generated keypair, a registered `handle`, and one
/// contact, using the real `bingle_local` `save`. Returns `(path, keypair_passphrase)`.
fn write_state_file(
    dir: &TempDir,
    handle: &str,
    contact_handle: &str,
    contact_id: &str,
) -> (String, String) {
    let mut local = BingleApiLocalImpl::new(LocalApiConfig::default());
    let keypair = local.generate_keypair().expect("generate keypair");
    // Mark the account ACTIVE with a registered handle so the bridge can resolve it offline.
    local.seed_own_handle_for_tests(handle.to_string());
    local
        .add_contact(
            contact_handle.to_string(),
            contact_id.to_string(),
            ContactSource::Manual,
        )
        .expect("add contact");
    let path = dir.path().join("state.json").to_string_lossy().into_owned();
    local.save(&path).expect("save state");
    (path, keypair.passphrase)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn state_file_surfaces_keypair_handle_and_contacts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (path, passphrase) = write_state_file(&dir, "alice", "bob", "BOB_ID");

    let chat_args = parse_chat_args(args(&["--state_file", &path])).expect("parse");
    let state = ChatState::from_chat_args(&chat_args).expect("bridge should load");

    // Handle and passphrase come from the stored keypair with no CLI flags.
    assert_eq!(state.opts.handle, "alice");
    assert_eq!(
        state.opts.algo_passphrase.as_deref(),
        Some(passphrase.as_str())
    );
    // Contacts resolve for --to.
    assert_eq!(state.resolve_recipient("bob"), Some("BOB_ID"));
    assert!(state.resolve_recipient("carol").is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn cli_handle_and_passphrase_override_state_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (path, _passphrase) = write_state_file(&dir, "alice", "bob", "BOB_ID");

    let chat_args = parse_chat_args(args(&[
        "--handle",
        "override",
        "--passphrase",
        "cli-pass",
        "--state_file",
        &path,
    ]))
    .expect("parse");
    let state = ChatState::from_chat_args(&chat_args).expect("bridge should load");

    assert_eq!(state.opts.handle, "override");
    assert_eq!(state.opts.algo_passphrase.as_deref(), Some("cli-pass"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn save_state_round_trips_new_contact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (path, _passphrase) = write_state_file(&dir, "alice", "bob", "BOB_ID");

    let chat_args = parse_chat_args(args(&["--state_file", &path])).expect("parse");
    let mut state = ChatState::from_chat_args(&chat_args).expect("bridge should load");
    state.add_contact("carol", "CAROL_ID").expect("add contact");
    state.save_state().expect("save state");

    // Reload through a fresh bridge and assert both the old and new contacts persist.
    let chat_args2 = parse_chat_args(args(&["--state_file", &path])).expect("parse");
    let reloaded = ChatState::from_chat_args(&chat_args2).expect("bridge should reload");
    assert_eq!(reloaded.resolve_recipient("bob"), Some("BOB_ID"));
    assert_eq!(reloaded.resolve_recipient("carol"), Some("CAROL_ID"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn missing_state_file_with_handle_starts_empty() {
    // A path that does not exist yet is a first-run empty store; with a CLI handle it succeeds.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir
        .path()
        .join("does_not_exist.json")
        .to_string_lossy()
        .into_owned();

    let chat_args =
        parse_chat_args(args(&["--handle", "alice", "--state_file", &path])).expect("parse");
    let state = ChatState::from_chat_args(&chat_args).expect("bridge should start empty");

    assert_eq!(state.opts.handle, "alice");
    assert!(state.contacts.is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn missing_state_file_without_handle_defers_to_registration() {
    // Since issue #59, a missing file with no --handle is not an error at load time: it is an
    // unregistered account whose fate the cmd_chat registration flow decides. from_chat_args now
    // succeeds with an empty handle rather than erroring here.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("nope.json").to_string_lossy().into_owned();

    let chat_args = parse_chat_args(args(&["--state_file", &path])).expect("parse");
    let state = ChatState::from_chat_args(&chat_args).expect("load should not require a handle");
    assert_eq!(state.opts.handle, "");
    assert!(state.contacts.is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn malformed_state_file_is_clear_error_not_panic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bad.json").to_string_lossy().into_owned();
    std::fs::write(&path, "{ this is not valid json ").expect("write bad file");

    let chat_args =
        parse_chat_args(args(&["--handle", "alice", "--state_file", &path])).expect("parse");
    let err = ChatState::from_chat_args(&chat_args)
        .err()
        .expect("malformed json should error");
    assert!(
        err.contains("failed to load chat state"),
        "error should point at the state load; got: {err}"
    );
}

/// A `MakeWriter` that captures emitted log bytes into a shared buffer, so a test can assert on the
/// content of the logs produced while the bridge runs.
#[derive(Clone)]
struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

impl Write for CapturingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("log buffer poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturingWriter {
    type Writer = CapturingWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn bridge_never_logs_the_passphrase() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (path, passphrase) = write_state_file(&dir, "alice", "bob", "BOB_ID");

    let buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(CapturingWriter(buffer.clone()))
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let chat_args = parse_chat_args(args(&["--state_file", &path])).expect("parse");
        let state = ChatState::from_chat_args(&chat_args).expect("bridge should load");
        // Sanity: the passphrase really was surfaced into the config (so the assertion below is meaningful).
        assert_eq!(
            state.opts.algo_passphrase.as_deref(),
            Some(passphrase.as_str())
        );
    });

    let logs =
        String::from_utf8(buffer.lock().expect("log buffer poisoned").clone()).expect("utf8 logs");
    assert!(
        !logs.contains(&passphrase),
        "passphrase must never appear in logs"
    );
}
