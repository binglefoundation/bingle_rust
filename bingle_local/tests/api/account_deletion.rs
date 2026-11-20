//! Account deletion (issue #100): the opt-out-then-clear sequence, against a recording opt-out and
//! an in-memory key store, so no chain is needed. The on-chain opt-out itself is covered by
//! `account_deletion_localnet` in bingle_core's integration suite.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

use algo_ops::error::{AlgoError, AlgoErrorKind};
use bingle_core::api::bingle_api::BingleError;
use bingle_local::api::{
    AccountOptOut, BingleApiLocalImpl, BingleLocalApi, ContactSource, KeySlot, KeyStorageBackend,
    KeyStorageStatus, KeyStore, KeyStoreError, Keypair, LocalApiConfig,
};

/// An in-memory key store that keeps phrases out of the state file, and whose deletes can fail.
#[derive(Default)]
struct MemoryKeyStore {
    keys: Mutex<HashMap<KeySlot, Keypair>>,
    fail_delete: AtomicBool,
}

impl KeyStore for MemoryKeyStore {
    fn backend(&self) -> KeyStorageBackend {
        KeyStorageBackend::Keychain
    }

    fn keeps_phrase_in_state_file(&self) -> bool {
        false
    }

    fn store(
        &self,
        slot: KeySlot,
        keypair: &Keypair,
        _replace_synced: bool,
    ) -> Result<(), KeyStoreError> {
        self.keys.lock().unwrap().insert(slot, keypair.clone());
        Ok(())
    }

    fn fetch(&self, slot: KeySlot) -> Result<Option<Keypair>, KeyStoreError> {
        Ok(self.keys.lock().unwrap().get(&slot).cloned())
    }

    fn delete(&self, slot: KeySlot) -> Result<(), KeyStoreError> {
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err(KeyStoreError::Storage("simulated failure".into()));
        }
        self.keys.lock().unwrap().remove(&slot);
        Ok(())
    }

    fn status(&self) -> Result<KeyStorageStatus, KeyStoreError> {
        Ok(KeyStorageStatus {
            backend: KeyStorageBackend::Keychain,
            device_only: true,
            sync_choice_pending: false,
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

/// Records which accounts it was asked to opt out, and can be made to fail with a given error.
#[derive(Default)]
struct RecordingOptOut {
    calls: RefCell<Vec<String>>,
    // Builds the error to fail with: `BingleError` isn't `Clone`.
    failure: Option<fn() -> BingleError>,
}

impl RecordingOptOut {
    fn failing() -> Self {
        Self::failing_with(|| BingleError::Other("clear-state rejected".into()))
    }

    fn failing_with(failure: fn() -> BingleError) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            failure: Some(failure),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl AccountOptOut for RecordingOptOut {
    fn opt_out(&self, keypair: &Keypair) -> Result<Vec<u64>, BingleError> {
        self.calls.borrow_mut().push(keypair.id.clone());
        if let Some(failure) = self.failure {
            return Err(failure());
        }
        Ok(vec![1])
    }
}

/// An account with a key in the store, a contact, a block, a message and a memoized handle.
fn populated_account(store: &Arc<MemoryKeyStore>) -> (BingleApiLocalImpl, Keypair) {
    let mut api = BingleApiLocalImpl::with_key_store(LocalApiConfig::default(), store.clone());
    let kp = api.generate_keypair().expect("generate");
    api.seed_own_handle_for_tests("deleter".into());
    api.add_contact("friend".into(), "FRIEND_ID".into(), ContactSource::Manual)
        .expect("add contact");
    api.add_contact("pest".into(), "PEST_ID".into(), ContactSource::Manual)
        .expect("add contact");
    api.block_contact("PEST_ID".into()).expect("block");
    api.add_message(
        "deleter".into(),
        vec!["friend".into()],
        1,
        "hello".into(),
        None,
    )
    .expect("add message");
    (api, kp)
}

fn assert_cleared(api: &BingleApiLocalImpl, store: &MemoryKeyStore) {
    assert_eq!(api.get_keypair().unwrap(), None, "no keypair");
    assert_eq!(
        api.get_recovery_keypair().unwrap(),
        None,
        "no recovery keypair"
    );
    assert!(api.get_contacts().unwrap().is_empty(), "no contacts");
    assert!(api.get_blocked_contacts().unwrap().is_empty(), "no blocks");
    assert!(api.get_messages().unwrap().is_empty(), "no messages");
    assert_eq!(api.own_handle(), None, "no memoized handle");
    assert_eq!(
        store.fetch(KeySlot::Current).unwrap(),
        None,
        "no stored key"
    );
    assert_eq!(
        store.fetch(KeySlot::Recovery).unwrap(),
        None,
        "no stored recovery key"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn deletion_opts_out_then_clears_the_device() {
    let store = Arc::new(MemoryKeyStore::default());
    let (api, kp) = populated_account(&store);
    let opt_out = RecordingOptOut::default();

    api.delete_account_with(&opt_out).expect("delete");

    assert_eq!(opt_out.calls(), vec![kp.id]);
    assert_cleared(&api, &store);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_pending_recovery_account_is_opted_out_too() {
    let store = Arc::new(MemoryKeyStore::default());
    let (mut api, kp) = populated_account(&store);
    let recovery = api.begin_recovery_keypair().expect("begin recovery");
    let opt_out = RecordingOptOut::default();

    api.delete_account_with(&opt_out).expect("delete");

    assert_eq!(opt_out.calls(), vec![kp.id, recovery.id]);
    assert_cleared(&api, &store);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_failed_opt_out_leaves_the_device_untouched_and_can_be_retried() {
    let store = Arc::new(MemoryKeyStore::default());
    let (api, kp) = populated_account(&store);

    let err = api
        .delete_account_with(&RecordingOptOut::failing())
        .expect_err("the opt-out failure is returned");
    assert!(
        matches!(&err, BingleError::Retryable(reason) if reason.contains("clear-state rejected")),
        "an opt-out failure is reported as retryable, with its cause: {err:?}"
    );
    assert_eq!(api.get_keypair().unwrap(), Some(kp.clone()), "key kept");
    assert_eq!(store.fetch(KeySlot::Current).unwrap(), Some(kp.clone()));
    assert_eq!(api.get_contacts().unwrap().len(), 1, "contacts kept");
    assert_eq!(api.get_messages().unwrap().len(), 1, "messages kept");
    assert_eq!(api.own_handle(), Some("deleter".into()));

    let opt_out = RecordingOptOut::default();
    api.delete_account_with(&opt_out).expect("retry");
    assert_eq!(opt_out.calls(), vec![kp.id]);
    assert_cleared(&api, &store);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_failed_key_store_delete_keeps_the_key_so_a_retry_resumes() {
    let store = Arc::new(MemoryKeyStore::default());
    let (api, kp) = populated_account(&store);
    store.fail_delete.store(true, Ordering::SeqCst);

    assert!(
        api.delete_account_with(&RecordingOptOut::default())
            .is_err()
    );
    assert_eq!(
        api.get_keypair().unwrap(),
        Some(kp.clone()),
        "the key stays, so the retry can sign the opt-out again"
    );

    store.fail_delete.store(false, Ordering::SeqCst);
    let opt_out = RecordingOptOut::default();
    api.delete_account_with(&opt_out).expect("retry");
    assert_eq!(opt_out.calls(), vec![kp.id], "the opt-out runs again");
    assert_cleared(&api, &store);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn with_no_keypair_deletion_only_clears_the_device() {
    let store = Arc::new(MemoryKeyStore::default());
    let api = BingleApiLocalImpl::with_key_store(LocalApiConfig::default(), store.clone());
    api.add_contact_shared("friend".into(), "FRIEND_ID".into(), ContactSource::Manual)
        .expect("add contact");
    let opt_out = RecordingOptOut::default();

    api.delete_account_with(&opt_out).expect("delete");
    api.delete_account_with(&opt_out).expect("delete again");

    assert!(opt_out.calls().is_empty(), "nothing to opt out");
    assert_cleared(&api, &store);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_trait_method_with_no_keypair_needs_no_network() {
    let store = Arc::new(MemoryKeyStore::default());
    let mut api = BingleApiLocalImpl::with_key_store(LocalApiConfig::default(), store.clone());
    api.delete_account()
        .expect("delete with nothing to opt out");
    assert_cleared(&api, &store);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn an_unreachable_node_stays_a_host_unreachable_error() {
    let store = Arc::new(MemoryKeyStore::default());
    let (api, kp) = populated_account(&store);
    let err = api
        .delete_account_with(&RecordingOptOut::failing_with(|| {
            BingleError::Algo(AlgoError::unreachable("status", "connection refused"))
        }))
        .expect_err("the failure is returned");

    assert!(
        matches!(&err, BingleError::Algo(ae) if ae.kind == AlgoErrorKind::HostUnreachable),
        "an unreachable node is reported as such (NoBlockchain in bingle_jsi): {err:?}"
    );
    assert_eq!(api.get_keypair().unwrap(), Some(kp), "nothing changed");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn other_chain_errors_become_retryable() {
    let store = Arc::new(MemoryKeyStore::default());
    let (api, _kp) = populated_account(&store);
    let err = api
        .delete_account_with(&RecordingOptOut::failing_with(|| {
            BingleError::Algo(AlgoError::transient("send_raw", "503 from algod"))
        }))
        .expect_err("the failure is returned");

    assert!(
        matches!(&err, BingleError::Retryable(reason) if reason.contains("503 from algod")),
        "a chain error other than an unreachable node is retryable: {err:?}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_key_store_failure_after_the_opt_out_is_an_internal_error() {
    let store = Arc::new(MemoryKeyStore::default());
    let (api, _kp) = populated_account(&store);
    store.fail_delete.store(true, Ordering::SeqCst);

    let err = api
        .delete_account_with(&RecordingOptOut::default())
        .expect_err("the failure is returned");

    assert!(
        matches!(&err, BingleError::Other(reason) if reason.contains("simulated failure")),
        "a key store failure is reported as an internal error: {err:?}"
    );
}
