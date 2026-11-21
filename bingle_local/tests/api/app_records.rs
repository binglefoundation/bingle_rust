//! The app's own records in the state file (issue #471): kept across save and load, plain and
//! sealed, validated, and removed with the account.

use std::path::PathBuf;
use std::sync::Arc;

use bingle_core::api::bingle_api::BingleError;
use bingle_local::api::keychain::KeychainKeyStore;
use bingle_local::api::keychain::memory::MemoryItems;
use bingle_local::api::{
    APP_RECORD_NAME_MAX, APP_RECORD_VALUE_MAX, AccountOptOut, BingleApiLocalImpl, BingleLocalApi,
    Keypair, LocalApiConfig, validate_app_record,
};
use bingle_test::temp_file_helpers::project_tmp_dir_path;

const SCOPE: &str = "bingle_state_testnet";
const READ_STATE: &str = r#"{"since":1,"lastRead":{"friend_handle":2}}"#;

/// A state directory, removed afterwards.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        Self(project_tmp_dir_path(name))
    }

    fn path(&self) -> String {
        self.0
            .join(format!("{SCOPE}.json"))
            .to_str()
            .expect("utf-8 path")
            .to_string()
    }

    /// The local API over an in-memory Keychain, sealing when `encrypt_state`.
    fn api(&self, items: &Arc<MemoryItems>, encrypt_state: bool) -> BingleApiLocalImpl {
        let store = Arc::new(KeychainKeyStore::new(items.clone(), SCOPE, &self.0));
        BingleApiLocalImpl::with_key_store(
            LocalApiConfig::default().with_encrypt_state(encrypt_state),
            store,
        )
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

/// Saves `api`, then loads the file into a fresh API on the same device.
fn reloaded(
    dir: &Dir,
    items: &Arc<MemoryItems>,
    api: &BingleApiLocalImpl,
    sealed: bool,
) -> BingleApiLocalImpl {
    api.save(&dir.path()).expect("save");
    let mut fresh = dir.api(items, sealed);
    fresh.load(&dir.path()).expect("load");
    fresh
}

#[test]
#[cfg(not(target_os = "ios"))]
fn records_survive_save_and_load_in_a_plain_file() {
    let dir = Dir::new("app-records-plain");
    let items = Arc::new(MemoryItems::new());
    let mut api = dir.api(&items, false);
    api.generate_keypair().expect("generate");
    api.set_app_record("read_state", READ_STATE.into())
        .expect("set read_state");
    api.set_app_record("report_log", "[]".into())
        .expect("set report_log");

    let fresh = reloaded(&dir, &items, &api, false);

    assert_eq!(
        fresh.app_record("read_state").expect("get").as_deref(),
        Some(READ_STATE)
    );
    assert_eq!(
        fresh.app_record("report_log").expect("get").as_deref(),
        Some("[]")
    );
    assert_eq!(fresh.app_record("messaging_settings").expect("get"), None);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn records_survive_save_and_load_in_a_sealed_file_and_are_not_readable_in_it() {
    let dir = Dir::new("app-records-sealed");
    let items = Arc::new(MemoryItems::new());
    let mut api = dir.api(&items, true);
    api.generate_keypair().expect("generate");
    api.set_app_record("read_state", READ_STATE.into())
        .expect("set");

    let fresh = reloaded(&dir, &items, &api, true);

    let bytes = std::fs::read(dir.path()).expect("read state file");
    assert!(!contains(&bytes, "friend_handle"), "the record is readable");
    assert!(
        !contains(&bytes, "read_state"),
        "the record's name is readable"
    );
    assert_eq!(
        fresh.app_record("read_state").expect("get").as_deref(),
        Some(READ_STATE)
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn setting_replaces_and_removing_deletes() {
    let dir = Dir::new("app-records-replace");
    let items = Arc::new(MemoryItems::new());
    let mut api = dir.api(&items, false);
    api.set_app_record("read_state", "1".into()).expect("set");
    api.set_app_record("read_state", "2".into())
        .expect("replace");
    assert_eq!(
        api.app_record("read_state").expect("get").as_deref(),
        Some("2")
    );

    api.remove_app_record("read_state").expect("remove");
    api.remove_app_record("read_state")
        .expect("removing a missing record is fine");

    let fresh = reloaded(&dir, &items, &api, false);
    assert_eq!(fresh.app_record("read_state").expect("get"), None);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_file_without_records_is_unchanged_by_them() {
    let dir = Dir::new("app-records-absent");
    let items = Arc::new(MemoryItems::new());
    let mut api = dir.api(&items, false);
    api.generate_keypair().expect("generate");
    api.save(&dir.path()).expect("save");

    let text = std::fs::read_to_string(dir.path()).expect("read");
    assert!(
        !text.contains("app_records"),
        "no records, so no field for an older engine to drop"
    );
}

#[test]
fn names_and_sizes_are_checked() {
    let max_name = "a".repeat(APP_RECORD_NAME_MAX);
    for good in [
        "read_state",
        "messaging_settings",
        "a",
        "v1.read-state_2",
        max_name.as_str(),
    ] {
        assert_eq!(validate_app_record(good, Some("{}")), Ok(()), "'{good}'");
    }
    let long_name = "a".repeat(APP_RECORD_NAME_MAX + 1);
    for bad in [
        "",
        "Read_State",
        "read state",
        "read/state",
        "é",
        long_name.as_str(),
    ] {
        assert!(validate_app_record(bad, None).is_err(), "'{bad}'");
    }
    let at_limit = "x".repeat(APP_RECORD_VALUE_MAX);
    assert_eq!(validate_app_record("big", Some(&at_limit)), Ok(()));
    let over = "x".repeat(APP_RECORD_VALUE_MAX + 1);
    assert!(validate_app_record("big", Some(&over)).is_err());
}

#[test]
fn an_invalid_record_is_refused_and_changes_nothing() {
    let dir = Dir::new("app-records-invalid");
    let items = Arc::new(MemoryItems::new());
    let mut api = dir.api(&items, false);
    api.set_app_record("read_state", "kept".into())
        .expect("set");

    let err = api
        .set_app_record("read_state", "x".repeat(APP_RECORD_VALUE_MAX + 1))
        .expect_err("oversized");
    assert!(matches!(err, BingleError::Other(_)));
    assert!(api.set_app_record("Bad Name", "{}".into()).is_err());
    assert!(api.app_record("Bad Name").is_err());
    assert!(api.remove_app_record("").is_err());

    assert_eq!(
        api.app_record("read_state").expect("get").as_deref(),
        Some("kept")
    );
}

/// Opts every account out without a blockchain.
struct NoChainOptOut;

impl AccountOptOut for NoChainOptOut {
    fn opt_out(&self, _keypair: &Keypair) -> Result<Vec<u64>, BingleError> {
        Ok(vec![])
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn deleting_the_account_removes_every_record() {
    let dir = Dir::new("app-records-delete");
    let items = Arc::new(MemoryItems::new());
    let mut api = dir.api(&items, true);
    api.generate_keypair().expect("generate");
    api.set_app_record("read_state", READ_STATE.into())
        .expect("set");
    api.save(&dir.path()).expect("save");

    api.delete_account_with(&NoChainOptOut).expect("delete");

    assert_eq!(api.app_record("read_state").expect("get"), None);
}
