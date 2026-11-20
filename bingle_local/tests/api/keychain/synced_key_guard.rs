//! `bingle_local` refuses to replace a key the user syncs through iCloud Keychain unless the caller
//! says the user agreed (issue #452): replacing it removes that account from every device on their
//! Apple Account. Over the in-memory Keychain.

use std::path::Path;
use std::sync::Arc;

use bingle_core::api::bingle_api::BingleError;
use bingle_local::api::keychain::KeychainKeyStore;
use bingle_local::api::keychain::items::Accessibility;
use bingle_local::api::keychain::memory::MemoryItems;
use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};
use bingle_test::temp_file_helpers::project_tmp_dir_path;

const SCOPE: &str = "bingle_state_testnet";
const KEY: &str = "key/bingle_state_testnet";

fn keychain_api(items: &Arc<MemoryItems>, dir: &Path) -> BingleApiLocalImpl {
    let store = Arc::new(KeychainKeyStore::new(items.clone(), SCOPE, dir));
    BingleApiLocalImpl::with_key_store(LocalApiConfig::default(), store)
}

/// A local API whose current key is synced through iCloud Keychain; returns it with that key.
fn with_synced_key(name: &str) -> (Arc<MemoryItems>, BingleApiLocalImpl, String) {
    let items = Arc::new(MemoryItems::new());
    let mut api = keychain_api(&items, &project_tmp_dir_path(name));
    let kp = api.generate_keypair(false).expect("generate");
    api.set_device_only(false).expect("choose sync");
    assert_eq!(
        items.accessibility(KEY, true),
        Some(Accessibility::WhenUnlocked)
    );
    (items, api, kp.id)
}

fn another_phrase() -> String {
    let mut other = BingleApiLocalImpl::new(LocalApiConfig::default());
    other.generate_keypair(false).expect("generate").passphrase
}

#[test]
#[cfg(not(target_os = "ios"))]
fn generating_over_a_synced_key_is_refused_and_changes_nothing() {
    let (items, mut api, synced) = with_synced_key("synced-guard-generate");

    match api.generate_keypair(false) {
        Err(BingleError::SyncedKeyWouldBeReplaced(address)) => assert_eq!(address, synced),
        other => panic!("expected SyncedKeyWouldBeReplaced, got {other:?}"),
    }
    assert_eq!(api.get_keypair().unwrap().map(|kp| kp.id), Some(synced));
    assert_eq!(
        items.accessibility(KEY, true),
        Some(Accessibility::WhenUnlocked)
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn importing_over_a_synced_key_needs_the_override() {
    let (items, mut api, synced) = with_synced_key("synced-guard-import");
    let phrase = another_phrase();

    match api.import_keypair(phrase.clone(), false) {
        Err(BingleError::SyncedKeyWouldBeReplaced(address)) => assert_eq!(address, synced),
        other => panic!("expected SyncedKeyWouldBeReplaced, got {other:?}"),
    }
    assert_eq!(api.get_keypair().unwrap().map(|kp| kp.id), Some(synced));

    let imported = api.import_keypair(phrase, true).expect("replace");
    assert_eq!(api.get_keypair().unwrap(), Some(imported));
    assert_eq!(
        items.accessibility(KEY, true),
        None,
        "the synced item is gone"
    );
    let status = api.key_storage_status().unwrap();
    assert!(status.device_only && status.sync_choice_pending);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn re_importing_the_synced_key_needs_no_override() {
    let (items, mut api, synced) = with_synced_key("synced-guard-same");
    let phrase = api.get_keypair().unwrap().expect("a key").passphrase;

    let kp = api.import_keypair(phrase, false).expect("same key");
    assert_eq!(kp.id, synced);
    assert_eq!(
        items.accessibility(KEY, true),
        Some(Accessibility::WhenUnlocked)
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn completing_a_recovery_replaces_the_synced_key_without_the_override() {
    let (items, mut api, synced) = with_synced_key("synced-guard-recovery");
    let pending = api.begin_recovery_keypair().expect("begin");
    assert_ne!(pending.id, synced);

    let kp = api.complete_recovery_keypair().expect("complete");
    assert_eq!(kp.id, pending.id);
    assert_eq!(api.get_keypair().unwrap().map(|k| k.id), Some(pending.id));
    assert_eq!(items.accessibility(KEY, true), None);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_file_store_ignores_the_override() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.generate_keypair(false).expect("first");
    let second = api.generate_keypair(false).expect("second");
    assert_eq!(api.get_keypair().unwrap(), Some(second));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn migrating_a_state_file_never_replaces_a_synced_key() {
    let dir = project_tmp_dir_path("synced-guard-migration");
    let path = dir.join(format!("{SCOPE}.json"));
    let path_str = path.to_str().unwrap().to_string();
    // a plain state file holding another account's phrase.
    let mut old = BingleApiLocalImpl::new(LocalApiConfig::default());
    let file_key = old.generate_keypair(false).expect("generate");
    old.save(&path_str).expect("save");
    // this device's Keychain syncs a different key.
    let items = Arc::new(MemoryItems::new());
    let mut synced_api = keychain_api(&items, &dir);
    let synced = synced_api.generate_keypair(false).expect("generate");
    synced_api.set_device_only(false).expect("choose sync");

    let mut api = keychain_api(&items, &dir);
    api.load(&path_str).expect("load");

    // the file's key stays in use, and in the file; the synced key is untouched.
    assert_eq!(api.get_keypair().unwrap(), Some(file_key.clone()));
    api.save(&path_str).expect("save");
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains(&file_key.passphrase)
    );
    assert_eq!(
        api.key_storage_status().unwrap().stored_address,
        Some(synced.id)
    );
    assert_eq!(
        items.accessibility(KEY, true),
        Some(Accessibility::WhenUnlocked)
    );
}
