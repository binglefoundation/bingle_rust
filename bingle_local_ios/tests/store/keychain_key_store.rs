//! `KeychainKeyStore` against the in-memory Keychain: device-only creation, the sync choice,
//! switching, records, and reinstall detection.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bingle_local::api::{KeySlot, KeyStorageBackend, KeyStore, KeyStoreError, Keypair};
use bingle_local_ios::KeychainKeyStore;
use bingle_local_ios::items::{Accessibility, SecureItems};
use bingle_local_ios::memory::MemoryItems;
use bingle_local_ios::record::{KeyRecord, RECORD_VERSION, decode_key, encode};
use bingle_test::temp_file_helpers::project_tmp_dir_path;

const SCOPE: &str = "bingle_state_testnet";
const KEY: &str = "key/bingle_state_testnet";
const RECOVERY: &str = "recovery/bingle_state_testnet";

fn keypair(id: &str) -> Keypair {
    Keypair {
        id: id.to_string(),
        passphrase: format!("{id} phrase words"),
    }
}

fn state_dir(name: &str) -> PathBuf {
    project_tmp_dir_path(name)
}

fn store(items: &Arc<MemoryItems>, dir: &Path) -> KeychainKeyStore {
    KeychainKeyStore::new(items.clone(), SCOPE, dir)
}

fn fresh() -> (Arc<MemoryItems>, KeychainKeyStore) {
    let items = Arc::new(MemoryItems::new());
    let dir = state_dir("keychain-store");
    let s = store(&items, &dir);
    (items, s)
}

fn record(items: &MemoryItems, account: &str, synced: bool) -> Option<KeyRecord> {
    items
        .read(account, synced)
        .unwrap()
        .map(|data| decode_key(&data).unwrap())
}

#[test]
fn a_new_key_is_device_only_with_the_choice_pending() {
    let (items, s) = fresh();
    s.store(KeySlot::Current, &keypair("A")).unwrap();

    assert_eq!(
        items.accessibility(KEY, false),
        Some(Accessibility::WhenUnlockedThisDeviceOnly)
    );
    assert_eq!(items.accessibility(KEY, true), None, "nothing synced yet");
    let status = s.status().unwrap();
    assert_eq!(status.backend, KeyStorageBackend::Keychain);
    assert!(status.device_only);
    assert!(status.sync_choice_pending);
    assert!(!status.phrase_backed_up);
    assert_eq!(status.stored_address.as_deref(), Some("A"));
    assert_eq!(s.fetch(KeySlot::Current).unwrap(), Some(keypair("A")));
}

#[test]
fn choosing_sync_moves_the_key_to_a_synced_item() {
    let (items, s) = fresh();
    s.store(KeySlot::Current, &keypair("A")).unwrap();
    s.set_device_only(false).unwrap();

    assert_eq!(
        items.accessibility(KEY, true),
        Some(Accessibility::WhenUnlocked)
    );
    assert_eq!(
        items.accessibility(KEY, false),
        None,
        "device-only copy removed"
    );
    let status = s.status().unwrap();
    assert!(!status.device_only);
    assert!(!status.sync_choice_pending);
    assert_eq!(s.fetch(KeySlot::Current).unwrap(), Some(keypair("A")));
}

#[test]
fn choosing_device_only_for_a_never_synced_key_needs_no_backup() {
    let (items, s) = fresh();
    s.store(KeySlot::Current, &keypair("A")).unwrap();
    s.set_device_only(true).unwrap();

    assert_eq!(
        items.accessibility(KEY, false),
        Some(Accessibility::WhenUnlockedThisDeviceOnly)
    );
    let status = s.status().unwrap();
    assert!(status.device_only);
    assert!(!status.sync_choice_pending, "the choice is recorded");
}

#[test]
fn making_a_synced_key_device_only_needs_the_backup_confirmed() {
    let (items, s) = fresh();
    s.store(KeySlot::Current, &keypair("A")).unwrap();
    s.set_device_only(false).unwrap();

    assert_eq!(
        s.set_device_only(true).err(),
        Some(KeyStoreError::PhraseNotBackedUp)
    );
    assert_eq!(
        items.accessibility(KEY, true),
        Some(Accessibility::WhenUnlocked),
        "unchanged"
    );

    s.mark_phrase_backed_up().unwrap();
    assert!(s.status().unwrap().phrase_backed_up);
    s.set_device_only(true).unwrap();
    assert_eq!(items.accessibility(KEY, true), None);
    assert_eq!(
        items.accessibility(KEY, false),
        Some(Accessibility::WhenUnlockedThisDeviceOnly)
    );
    let stored = record(&items, KEY, false).unwrap();
    assert!(stored.device_only && stored.phrase_backed_up && stored.sync_chosen);
}

#[test]
fn a_failed_switch_leaves_the_old_item() {
    let (items, s) = fresh();
    s.store(KeySlot::Current, &keypair("A")).unwrap();
    items.set_fail_writes(true);
    assert!(s.set_device_only(false).is_err());
    items.set_fail_writes(false);

    assert_eq!(
        items.accessibility(KEY, false),
        Some(Accessibility::WhenUnlockedThisDeviceOnly)
    );
    assert_eq!(items.accessibility(KEY, true), None);
    assert_eq!(s.fetch(KeySlot::Current).unwrap(), Some(keypair("A")));
    assert!(
        s.status().unwrap().sync_choice_pending,
        "no choice recorded"
    );
}

#[test]
fn a_pending_recovery_key_follows_the_current_one() {
    let (items, s) = fresh();
    s.store(KeySlot::Current, &keypair("A")).unwrap();
    s.store(KeySlot::Recovery, &keypair("R")).unwrap();
    assert_eq!(
        items.accessibility(RECOVERY, false),
        Some(Accessibility::WhenUnlockedThisDeviceOnly)
    );

    s.set_device_only(false).unwrap();
    assert_eq!(
        items.accessibility(RECOVERY, true),
        Some(Accessibility::WhenUnlocked)
    );
    assert_eq!(s.fetch(KeySlot::Recovery).unwrap(), Some(keypair("R")));

    s.delete(KeySlot::Recovery).unwrap();
    assert_eq!(s.fetch(KeySlot::Recovery).unwrap(), None);
    assert_eq!(s.fetch(KeySlot::Current).unwrap(), Some(keypair("A")));
}

#[test]
fn storing_the_same_key_again_keeps_the_users_choice() {
    let (items, s) = fresh();
    s.store(KeySlot::Current, &keypair("A")).unwrap();
    s.set_device_only(false).unwrap();
    s.store(KeySlot::Current, &keypair("A")).unwrap();
    assert_eq!(
        items.accessibility(KEY, true),
        Some(Accessibility::WhenUnlocked)
    );
    assert!(!s.status().unwrap().sync_choice_pending);
}

#[test]
fn storing_a_different_key_starts_device_only_and_removes_the_old_synced_one() {
    let (items, s) = fresh();
    s.store(KeySlot::Current, &keypair("A")).unwrap();
    s.set_device_only(false).unwrap();
    s.store(KeySlot::Current, &keypair("B")).unwrap();

    assert_eq!(items.accessibility(KEY, true), None);
    assert_eq!(s.fetch(KeySlot::Current).unwrap(), Some(keypair("B")));
    assert!(s.status().unwrap().sync_choice_pending);
}

#[test]
fn settings_need_a_stored_key() {
    let (_, s) = fresh();
    assert_eq!(s.set_device_only(false).err(), Some(KeyStoreError::NoKey));
    assert_eq!(s.mark_phrase_backed_up().err(), Some(KeyStoreError::NoKey));
    let status = s.status().unwrap();
    assert!(!status.sync_choice_pending);
    assert_eq!(status.stored_address, None);
}

#[test]
fn the_record_round_trips_and_carries_a_version() {
    let r = KeyRecord {
        version: RECORD_VERSION,
        address: "A".into(),
        phrase: "words".into(),
        phrase_backed_up: true,
        sync_chosen: false,
        device_only: true,
    };
    let data = encode(&r).unwrap();
    assert_eq!(decode_key(&data).unwrap(), r);
    assert!(String::from_utf8(data).unwrap().contains("\"version\":1"));

    let future = encode(&KeyRecord { version: 99, ..r }).unwrap();
    assert!(decode_key(&future).is_err());
}

#[test]
fn installs_are_counted_once_per_install_and_a_reinstall_is_detected() {
    let items = Arc::new(MemoryItems::new());
    let dir = state_dir("keychain-install");

    let first = store(&items, &dir);
    let status = first.status().unwrap();
    assert_eq!(status.install_count, 1);
    assert!(status.last_install_at.is_some());
    assert!(!status.reinstall_detected, "no key stored yet");
    assert_eq!(
        items.accessibility("install", false),
        Some(Accessibility::AfterFirstUnlockThisDeviceOnly)
    );
    first.store(KeySlot::Current, &keypair("A")).unwrap();

    // a relaunch or app update: the marker is still there.
    let relaunched = store(&items, &dir);
    assert_eq!(relaunched.status().unwrap().install_count, 1);
    assert!(!relaunched.status().unwrap().reinstall_detected);

    // an uninstall deletes the app container (and the marker), but not the Keychain.
    std::fs::remove_dir_all(&dir).unwrap();
    let reinstalled = store(&items, &dir);
    let status = reinstalled.status().unwrap();
    assert_eq!(status.install_count, 2);
    assert!(status.reinstall_detected);
    assert_eq!(status.stored_address.as_deref(), Some("A"));
    assert_eq!(
        reinstalled.fetch(KeySlot::Current).unwrap(),
        Some(keypair("A"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}
