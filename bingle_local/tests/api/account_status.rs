//! Tests for on-chain account status resolution and `inspect_passphrase` (issue #388).

use std::cell::RefCell;

use bingle_core::api::bingle_api::BingleError;
use bingle_local::api::REQUIRED_ALGO;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use bingle_local::api::{AccountStatusReads, resolve_account_status};

const APP_ID: u64 = 11;
const ASSET_ID: u64 = 22;

/// Canned chain reads that record which reads were made.
struct MockReads {
    opted_in: Result<bool, String>,
    handle: Option<String>,
    balance: Option<f64>,
    required: Result<f64, String>,
    calls: RefCell<Vec<String>>,
}

impl MockReads {
    fn new(opted_in: bool, handle: Option<&str>, balance: Option<f64>) -> Self {
        MockReads {
            opted_in: Ok(opted_in),
            handle: handle.map(str::to_string),
            balance,
            required: Ok(1.0),
            calls: RefCell::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl AccountStatusReads for MockReads {
    fn is_opted_in_to_asset(&self, address: &str) -> Result<bool, BingleError> {
        self.calls
            .borrow_mut()
            .push(format!("opted_in:{}", address));
        self.opted_in.clone().map_err(BingleError::Other)
    }

    fn handle(&self, address: &str) -> Result<Option<String>, BingleError> {
        self.calls.borrow_mut().push(format!("handle:{}", address));
        Ok(self.handle.clone())
    }

    fn balance_algos(&self) -> Result<Option<f64>, BingleError> {
        self.calls.borrow_mut().push("balance".to_string());
        Ok(self.balance)
    }

    fn required_funding_algos(&self) -> Result<f64, BingleError> {
        self.calls.borrow_mut().push("required".to_string());
        self.required.clone().map_err(BingleError::Other)
    }
}

#[test]
fn registered_account_is_active_without_balance_reads() {
    let reads = MockReads::new(true, Some("alice"), Some(5.0));
    let status = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID)
        .expect("status should resolve");
    assert_eq!(status.status, "ACTIVE");
    assert_eq!(status.id.as_deref(), Some("ADDR"));
    assert_eq!(status.handle.as_deref(), Some("alice"));
    assert!(!status.stale);
    assert_eq!(reads.calls(), vec!["opted_in:ADDR", "handle:ADDR"]);
}

#[test]
fn unregistered_unfunded_account_is_unfunded() {
    let reads = MockReads::new(false, None, Some(0.25));
    let status = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID)
        .expect("status should resolve");
    assert_eq!(status.status, "UNFUNDED");
    assert_eq!(status.id.as_deref(), Some("ADDR"));
    assert!(status.handle.is_none());
    assert_eq!(status.required_algo, Some(0.75));
    // Not opted in: the handle is never read.
    assert_eq!(reads.calls(), vec!["opted_in:ADDR", "balance", "required"]);
}

#[test]
fn unregistered_unknown_account_counts_as_zero_balance() {
    let reads = MockReads::new(false, None, None);
    let status = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID)
        .expect("status should resolve");
    assert_eq!(status.status, "UNFUNDED");
    assert_eq!(status.required_algo, Some(1.0));
}

#[test]
fn asset_without_handle_is_not_active() {
    let reads = MockReads::new(true, None, Some(2.0));
    let status = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID)
        .expect("status should resolve");
    assert_eq!(status.status, "FUNDED");
    assert!(status.handle.is_none());
}

#[test]
fn unconfigured_app_skips_handle_read() {
    let reads = MockReads::new(true, Some("alice"), Some(2.0));
    let status = resolve_account_status(&reads, "ADDR".to_string(), 0, ASSET_ID)
        .expect("status should resolve");
    assert_eq!(status.status, "FUNDED");
    assert!(!reads.calls().iter().any(|c| c.starts_with("handle")));
}

#[test]
fn unconfigured_asset_skips_opt_in_read() {
    let reads = MockReads::new(true, Some("alice"), Some(2.0));
    let status =
        resolve_account_status(&reads, "ADDR".to_string(), APP_ID, 0).expect("status resolves");
    assert_eq!(status.status, "FUNDED");
    assert_eq!(reads.calls(), vec!["balance", "required"]);
}

#[test]
fn failed_required_funding_falls_back_to_flat_target() {
    let mut reads = MockReads::new(false, None, Some(0.0));
    reads.required = Err("price read failed".to_string());
    let status = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID)
        .expect("status should resolve");
    assert_eq!(status.status, "UNFUNDED");
    assert_eq!(status.required_algo, Some(REQUIRED_ALGO));
}

#[test]
fn failed_opt_in_read_is_a_failure_with_context() {
    let mut reads = MockReads::new(false, None, Some(0.0));
    reads.opted_in = Err("node down".to_string());
    let failure = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID)
        .expect_err("a failed read should fail");
    assert!(failure.context.contains("asset opt-in"));
    assert!(failure.error.to_string().contains("node down"));
}

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
