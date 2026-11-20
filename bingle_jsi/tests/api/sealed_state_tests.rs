//! The sealed state file through bingle_jsi (issue #459), over bingle_local's Keychain store on the
//! in-memory Keychain fake: `state_file_status` and the `StateLocked` / `RecoveryPhraseRequired`
//! errors the app sees. The real Keychain is covered by the iOS XCTests.

use std::path::Path;
use std::sync::{Arc, Mutex};

use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::error::BingleJsiError;
use bingle_jsi::api::types::StateFileStatus;
use bingle_local::api::keychain::KeychainKeyStore;
use bingle_local::api::keychain::memory::MemoryItems;
use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};
use bingle_test::temp_file_helpers::project_tmp_dir_path;

use crate::api::message_queue_integration_tests::mock_api;

const SCOPE: &str = "bingle_state_testnet";

/// A sealing local API on the Keychain `items`, with its state file in `dir`.
fn sealing_local(items: &Arc<MemoryItems>, dir: &Path) -> BingleApiLocalImpl {
    let store = Arc::new(KeychainKeyStore::new(items.clone(), SCOPE, dir));
    BingleApiLocalImpl::with_key_store(LocalApiConfig::default().with_encrypt_state(true), store)
}

fn jsi_over(local: BingleApiLocalImpl) -> Arc<BingleJsiApiImpl> {
    let boxed: Box<dyn BingleLocalApi> = Box::new(local);
    BingleJsiApiImpl::init_for_tests(mock_api(vec![], 0), Some(Arc::new(Mutex::new(boxed))))
}

/// Writes a sealed state file on one "device" and returns its path.
fn sealed_file(dir: &Path) -> String {
    let items = Arc::new(MemoryItems::new());
    let mut local = sealing_local(&items, dir);
    local.generate_keypair(false).expect("generate");
    let path = dir
        .join(format!("{SCOPE}.json"))
        .to_string_lossy()
        .into_owned();
    local.save(&path).expect("save");
    path
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_restored_file_reports_recovery_phrase_required() {
    let dir = project_tmp_dir_path("jsi-sealed-restored");
    let path = sealed_file(&dir);

    // Another device: its Keychain has no key for the file.
    let jsi = jsi_over(sealing_local(&Arc::new(MemoryItems::new()), &dir));
    assert_eq!(jsi.state_file_status().unwrap(), StateFileStatus::NotLoaded);
    let err = jsi.load(path).expect_err("no key on this device");

    assert!(
        matches!(err, BingleJsiError::RecoveryPhraseRequired { .. }),
        "{err:?}"
    );
    assert_eq!(
        jsi.state_file_status().unwrap(),
        StateFileStatus::RecoveryPhraseRequired
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_locked_device_reports_state_locked_then_loads_after_the_unlock() {
    let dir = project_tmp_dir_path("jsi-sealed-locked");
    let items = Arc::new(MemoryItems::new());
    let mut writer = sealing_local(&items, &dir);
    writer.generate_keypair(false).expect("generate");
    let path = dir
        .join(format!("{SCOPE}.json"))
        .to_string_lossy()
        .into_owned();
    writer.save(&path).expect("save");

    items.set_locked(true);
    let jsi = jsi_over(sealing_local(&items, &dir));
    let err = jsi.load(path.clone()).expect_err("locked");
    assert!(matches!(err, BingleJsiError::StateLocked { .. }), "{err:?}");
    assert_eq!(jsi.state_file_status().unwrap(), StateFileStatus::Locked);

    items.set_locked(false);
    jsi.load(path).expect("load after the unlock");
    assert_eq!(jsi.state_file_status().unwrap(), StateFileStatus::Sealed);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn export_keypair_returns_the_phrase_the_sealed_file_leaves_out() {
    let dir = project_tmp_dir_path("jsi-sealed-export");
    let items = Arc::new(MemoryItems::new());
    let mut local = sealing_local(&items, &dir);
    let kp = local.generate_keypair(false).expect("generate");
    let path = dir.join(format!("{SCOPE}.json"));
    local.save(path.to_string_lossy().as_ref()).expect("save");
    let raw = std::fs::read(&path).expect("read the sealed file");
    assert!(
        !raw.windows(kp.passphrase.len())
            .any(|w| w == kp.passphrase.as_bytes()),
        "the phrase isn't in the file"
    );

    let api = jsi_over(local);
    let exported = api.export_keypair().expect("export");
    assert_eq!(exported.id, kp.id);
    assert_eq!(exported.passphrase, kp.passphrase);
    let _ = std::fs::remove_dir_all(&dir);
}
