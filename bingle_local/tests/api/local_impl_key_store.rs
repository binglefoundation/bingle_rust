//! Keeping the recovery phrase in a key store instead of the state file (issue #385).

use bingle_local::api::{
    BingleApiLocalImpl, BingleLocalApi, KeySlot, KeyStorageBackend, KeyStorageStatus, KeyStore,
    KeyStoreError, Keypair, LocalApiConfig,
};
use bingle_test::temp_file_helpers::project_tmp_file_path;
use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

/// An in-memory key store that keeps phrases out of the state file, and can be made to fail.
#[derive(Default)]
struct MemoryKeyStore {
    keys: Mutex<HashMap<KeySlot, Keypair>>,
    fail_store: AtomicBool,
}

impl KeyStore for MemoryKeyStore {
    fn backend(&self) -> KeyStorageBackend {
        KeyStorageBackend::Keychain
    }

    fn keeps_phrase_in_state_file(&self) -> bool {
        false
    }

    fn store(&self, slot: KeySlot, keypair: &Keypair) -> Result<(), KeyStoreError> {
        if self.fail_store.load(Ordering::SeqCst) {
            return Err(KeyStoreError::Storage("simulated failure".into()));
        }
        self.keys.lock().unwrap().insert(slot, keypair.clone());
        Ok(())
    }

    fn fetch(&self, slot: KeySlot) -> Result<Option<Keypair>, KeyStoreError> {
        Ok(self.keys.lock().unwrap().get(&slot).cloned())
    }

    fn delete(&self, slot: KeySlot) -> Result<(), KeyStoreError> {
        self.keys.lock().unwrap().remove(&slot);
        Ok(())
    }

    fn status(&self) -> Result<KeyStorageStatus, KeyStoreError> {
        Ok(KeyStorageStatus {
            backend: KeyStorageBackend::Keychain,
            device_only: true,
            sync_choice_pending: true,
            phrase_backed_up: false,
            reinstall_detected: false,
            stored_address: self.fetch(KeySlot::Current)?.map(|kp| kp.id),
            install_count: 1,
            last_install_at: None,
        })
    }

    fn set_device_only(&self, _enabled: bool) -> Result<(), KeyStoreError> {
        Ok(())
    }

    fn mark_phrase_backed_up(&self) -> Result<(), KeyStoreError> {
        Ok(())
    }
    fn holds_state_secret(&self) -> bool {
        false
    }

    fn state_secret(&self, _create: bool) -> Result<Option<Zeroizing<[u8; 32]>>, KeyStoreError> {
        Ok(None)
    }

    fn delete_state_secret(&self) -> Result<(), KeyStoreError> {
        Ok(())
    }
}

fn state_path(name: &str) -> String {
    project_tmp_file_path(name, ".json")
        .to_str()
        .expect("project tmp path should be valid utf-8")
        .to_string()
}

fn api_with(store: &Arc<MemoryKeyStore>) -> BingleApiLocalImpl {
    BingleApiLocalImpl::with_key_store(LocalApiConfig::default(), store.clone())
}

fn file_text(path: &str) -> String {
    fs::read_to_string(path).expect("read state file")
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_file_store_keeps_the_phrase_in_the_state_file() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let kp = api.generate_keypair().expect("generate");
    let path = state_path("key-store-file-backend");
    api.save(&path).expect("save");
    assert!(file_text(&path).contains(&kp.passphrase));
    assert_eq!(
        api.key_storage_status().expect("status").backend,
        KeyStorageBackend::File
    );
    assert!(
        api.set_device_only(false).is_err(),
        "file store has no sync setting"
    );
    let _ = fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_platform_store_keeps_the_phrase_out_of_the_file_and_reloads_it() {
    let store = Arc::new(MemoryKeyStore::default());
    let mut api = api_with(&store);
    let kp = api.generate_keypair().expect("generate");
    assert_eq!(store.fetch(KeySlot::Current).unwrap(), Some(kp.clone()));

    let path = state_path("key-store-platform-backend");
    api.save(&path).expect("save");
    let text = file_text(&path);
    assert!(!text.contains(&kp.passphrase), "no phrase in the file");
    assert!(text.contains(&kp.id), "the address stays in the file");

    let mut reloaded = api_with(&store);
    reloaded.load(&path).expect("load");
    assert_eq!(reloaded.get_keypair().unwrap(), Some(kp));
    let _ = fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn loading_an_old_file_moves_the_phrase_into_the_store_and_rewrites_the_file() {
    // A state file written by the file store, as every install has today.
    let mut old = BingleApiLocalImpl::new(LocalApiConfig::default());
    let kp = old.generate_keypair().expect("generate");
    let path = state_path("key-store-migration-success");
    old.save(&path).expect("save");
    assert!(file_text(&path).contains(&kp.passphrase));

    let store = Arc::new(MemoryKeyStore::default());
    let mut api = api_with(&store);
    api.load(&path).expect("load");
    assert_eq!(api.get_keypair().unwrap(), Some(kp.clone()));
    assert_eq!(store.fetch(KeySlot::Current).unwrap(), Some(kp.clone()));
    assert!(
        !file_text(&path).contains(&kp.passphrase),
        "rewritten without the phrase"
    );
    let _ = fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_failed_move_keeps_the_phrase_in_the_file() {
    let mut old = BingleApiLocalImpl::new(LocalApiConfig::default());
    let kp = old.generate_keypair().expect("generate");
    let path = state_path("key-store-migration-failure");
    old.save(&path).expect("save");

    let store = Arc::new(MemoryKeyStore::default());
    store.fail_store.store(true, Ordering::SeqCst);
    let mut api = api_with(&store);
    api.load(&path).expect("load");
    assert_eq!(api.get_keypair().unwrap(), Some(kp.clone()), "still usable");
    assert!(file_text(&path).contains(&kp.passphrase));

    // and a later save still writes it, so the key is never lost.
    api.save(&path).expect("save");
    assert!(file_text(&path).contains(&kp.passphrase));
    let _ = fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_file_whose_key_is_missing_from_the_store_loads_without_a_keypair() {
    let store = Arc::new(MemoryKeyStore::default());
    let mut api = api_with(&store);
    api.generate_keypair().expect("generate");
    let path = state_path("key-store-missing-key");
    api.save(&path).expect("save");

    // the same file on a device whose store doesn't have the key, as after a backup restore.
    let elsewhere = Arc::new(MemoryKeyStore::default());
    let mut restored = api_with(&elsewhere);
    restored.load(&path).expect("load");
    assert_eq!(restored.get_keypair().unwrap(), None);
    let _ = fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_key_that_cant_be_stored_is_not_used() {
    let store = Arc::new(MemoryKeyStore::default());
    store.fail_store.store(true, Ordering::SeqCst);
    let mut api = api_with(&store);
    assert!(api.generate_keypair().is_err());
    assert_eq!(api.get_keypair().unwrap(), None);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_keys_go_through_the_store() {
    let store = Arc::new(MemoryKeyStore::default());
    let mut api = api_with(&store);
    api.generate_keypair().expect("generate");
    let pending = api.begin_recovery_keypair().expect("begin");
    assert_eq!(
        store.fetch(KeySlot::Recovery).unwrap(),
        Some(pending.clone())
    );

    let path = state_path("key-store-recovery");
    api.save(&path).expect("save");
    assert!(!file_text(&path).contains(&pending.passphrase));
    let mut reloaded = api_with(&store);
    reloaded.load(&path).expect("load");
    assert_eq!(
        reloaded.get_recovery_keypair().unwrap(),
        Some(pending.clone())
    );

    let completed = reloaded.complete_recovery_keypair().expect("complete");
    assert_eq!(completed, pending);
    assert_eq!(store.fetch(KeySlot::Current).unwrap(), Some(pending));
    assert_eq!(store.fetch(KeySlot::Recovery).unwrap(), None);
    let _ = fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn restore_and_remove_use_the_stored_key() {
    let store = Arc::new(MemoryKeyStore::default());
    let mut first = api_with(&store);
    let kp = first.generate_keypair().expect("generate");

    // a reinstall: no state file, the store still has the key.
    let mut reinstalled = api_with(&store);
    assert_eq!(reinstalled.get_keypair().unwrap(), None);
    assert_eq!(
        reinstalled.key_storage_status().unwrap().stored_address,
        Some(kp.id.clone())
    );
    assert_eq!(reinstalled.restore_stored_key().expect("restore"), kp);
    assert_eq!(reinstalled.get_keypair().unwrap(), Some(kp));

    reinstalled.remove_key_from_device().expect("remove");
    assert_eq!(reinstalled.get_keypair().unwrap(), None);
    assert_eq!(store.fetch(KeySlot::Current).unwrap(), None);
    assert!(reinstalled.restore_stored_key().is_err());
}
