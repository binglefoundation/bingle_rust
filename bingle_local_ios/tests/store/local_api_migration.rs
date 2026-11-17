//! `bingle_local` with the Keychain store: an existing state file's phrase moves into a
//! device-only Keychain item, with the sync choice still to make.

use std::sync::Arc;

use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};
use bingle_local_ios::KeychainKeyStore;
use bingle_local_ios::items::Accessibility;
use bingle_local_ios::memory::MemoryItems;
use bingle_test::temp_file_helpers::project_tmp_dir_path;

#[test]
#[cfg(not(target_os = "ios"))]
fn an_existing_state_file_migrates_to_a_device_only_keychain_item() {
    let dir = project_tmp_dir_path("keychain-migration");
    let path = dir.join("bingle_state_testnet.json");
    let path_str = path.to_str().unwrap().to_string();

    // today's install: the phrase is in the state file.
    let mut old = BingleApiLocalImpl::new(LocalApiConfig::default());
    let kp = old.generate_keypair().expect("generate");
    old.save(&path_str).expect("save");

    // the update: the same file, now with the Keychain store.
    let items = Arc::new(MemoryItems::new());
    let store = Arc::new(KeychainKeyStore::new(
        items.clone(),
        "bingle_state_testnet",
        &dir,
    ));
    let mut api = BingleApiLocalImpl::with_key_store(LocalApiConfig::default(), store);
    api.load(&path_str).expect("load");

    assert_eq!(api.get_keypair().unwrap(), Some(kp.clone()));
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains(&kp.passphrase)
    );
    assert_eq!(
        items.accessibility("key/bingle_state_testnet", false),
        Some(Accessibility::WhenUnlockedThisDeviceOnly)
    );
    assert_eq!(items.accessibility("key/bingle_state_testnet", true), None);
    let status = api.key_storage_status().unwrap();
    assert!(status.sync_choice_pending && status.device_only);

    // the user chooses sync.
    api.set_device_only(false).unwrap();
    assert_eq!(
        items.accessibility("key/bingle_state_testnet", true),
        Some(Accessibility::WhenUnlocked)
    );
    assert!(!api.key_storage_status().unwrap().sync_choice_pending);
    let _ = std::fs::remove_dir_all(&dir);
}
