//! Tests for `inspect_passphrase` (issue #388). The chain resolution it shares with
//! `keypair_status` is tested in bingle_core (`tests/blockchain/account_status.rs`).

use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};

// --- inspect_passphrase on the real implementation ---

fn temp_state_path(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "bingle_test_{}_{}_{}.json",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    p.to_string_lossy().to_string()
}

#[test]
fn inspect_rejects_invalid_phrase_as_import_does() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let inspect_err = api
        .inspect_passphrase("not a valid mnemonic".to_string())
        .expect_err("an invalid phrase must be rejected");
    let import_err = api
        .import_keypair("not a valid mnemonic".to_string())
        .expect_err("an invalid phrase must be rejected");
    assert_eq!(inspect_err.to_string(), import_err.to_string());
}

#[test]
fn inspect_saves_no_keypair() {
    let mut source = BingleApiLocalImpl::new(LocalApiConfig::default());
    let phrase = source.generate_keypair().expect("keypair").passphrase;

    let api = BingleApiLocalImpl::new(LocalApiConfig::default());
    // With no node the chain read fails; either way nothing may be stored.
    let _ = api.inspect_passphrase(phrase);
    assert!(api.get_keypair().expect("get_keypair").is_none());
    assert_eq!(api.keypair_status().expect("status").status, "None");
}

/// Inspecting another phrase leaves the stored keypair and its memoized ACTIVE status as they were.
#[test]
fn inspect_leaves_stored_keypair_and_memo_unchanged() {
    let path = temp_state_path("inspect_memo");
    std::fs::write(
        &path,
        r#"{ "keypair": { "id": "MEMOID", "passphrase": "seed" }, "contacts": [], "messages": [], "own_handle": "alice" }"#,
    )
    .expect("write state");
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.load(&path).expect("load");

    let mut source = BingleApiLocalImpl::new(LocalApiConfig::default());
    let other = source.generate_keypair().expect("keypair");
    let _ = api.inspect_passphrase(other.passphrase);

    let kp = api
        .get_keypair()
        .expect("get_keypair")
        .expect("keypair kept");
    assert_eq!(kp.id, "MEMOID");
    assert_eq!(kp.passphrase, "seed");
    let status = api.keypair_status().expect("served from the memo");
    assert_eq!(status.status, "ACTIVE");
    assert_eq!(status.handle.as_deref(), Some("alice"));

    let _ = std::fs::remove_file(&path);
}

/// A failed chain read is an error, never another account's last-known status.
#[test]
fn inspect_offline_is_an_error_not_a_cached_status() {
    let path = temp_state_path("inspect_offline");
    std::fs::write(
        &path,
        r#"{ "keypair": { "id": "MEMOID", "passphrase": "seed" }, "contacts": [], "messages": [], "own_handle": "alice" }"#,
    )
    .expect("write state");
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.load(&path).expect("load");
    assert_eq!(api.keypair_status().expect("memo").status, "ACTIVE");

    let mut source = BingleApiLocalImpl::new(LocalApiConfig::default());
    let other = source.generate_keypair().expect("keypair");
    match api.inspect_passphrase(other.passphrase) {
        Ok(status) => {
            // Only possible with a reachable node; then it must describe the inspected account.
            assert_eq!(status.id.as_deref(), Some(other.id.as_str()));
            assert_ne!(status.status, "ACTIVE");
        }
        Err(_) => {}
    }

    let _ = std::fs::remove_file(&path);
}
