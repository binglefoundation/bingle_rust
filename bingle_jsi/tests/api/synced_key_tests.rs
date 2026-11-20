//! A key synced through iCloud Keychain isn't replaced without the app's say-so (issue #452):
//! `generate_keypair` and `import_keypair` fail with `SyncedKeyWouldBeReplaced`, carrying the stored
//! address, until called with `replace_synced_key`. Over bingle_local's Keychain store on the
//! in-memory Keychain fake; the real Keychain is covered by the iOS XCTests.

use std::sync::{Arc, Mutex};

use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::error::BingleJsiError;
use bingle_local::api::keychain::KeychainKeyStore;
use bingle_local::api::keychain::memory::MemoryItems;
use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};
use bingle_test::temp_file_helpers::project_tmp_dir_path;

use crate::api::message_queue_integration_tests::mock_api;

fn keychain_jsi(name: &str) -> Arc<BingleJsiApiImpl> {
    let items = Arc::new(MemoryItems::new());
    let store = Arc::new(KeychainKeyStore::new(
        items,
        "bingle_state_testnet",
        &project_tmp_dir_path(name),
    ));
    let local: Box<dyn BingleLocalApi> = Box::new(BingleApiLocalImpl::with_key_store(
        LocalApiConfig::default(),
        store,
    ));
    BingleJsiApiImpl::init_for_tests(mock_api(vec![], 0), Some(Arc::new(Mutex::new(local))))
}

#[test]
#[cfg(not(target_os = "ios"))]
fn replacing_a_synced_key_reports_its_address_until_the_app_agrees() {
    let api = keychain_jsi("jsi-synced-key");
    let synced = api.generate_keypair(false).expect("generate");
    api.set_device_only(false).expect("choose sync");

    match api.generate_keypair(false) {
        Err(BingleJsiError::SyncedKeyWouldBeReplaced { stored_address }) => {
            assert_eq!(stored_address, synced.id)
        }
        other => panic!("expected SyncedKeyWouldBeReplaced, got {other:?}"),
    }
    let other_phrase = BingleApiLocalImpl::new(LocalApiConfig::default())
        .generate_keypair(false)
        .expect("generate")
        .passphrase;
    assert!(matches!(
        api.import_keypair(other_phrase.clone(), false),
        Err(BingleJsiError::SyncedKeyWouldBeReplaced { .. })
    ));
    assert_eq!(api.export_keypair().expect("export").id, synced.id);

    let imported = api.import_keypair(other_phrase, true).expect("replace");
    assert_eq!(api.export_keypair().expect("export").id, imported.id);
    assert!(
        api.key_storage_status()
            .expect("status")
            .sync_choice_pending
    );
}
