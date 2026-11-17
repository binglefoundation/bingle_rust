//! Where the account's recovery phrase is kept (issue #385).
//!
//! The local API keeps the keypair's address in its state file. The 25-word phrase goes through a
//! [`KeyStore`]: [`FileKeyStore`] keeps it in the state file, as before, and a platform store (the
//! iOS Keychain, [`KeychainKeyStore`](crate::api::keychain::KeychainKeyStore)) keeps it out of the
//! file altogether. The caller
//! picks the store when it builds the API, with
//! [`BingleApiLocalImpl::with_key_store`](crate::api::BingleApiLocalImpl::with_key_store).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::api::Keypair;

/// Which keypair a stored phrase belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeySlot {
    /// The current account.
    Current,
    /// The fresh account a handle clash recovery moves to, until it completes (issue #344).
    Recovery,
}

/// Which kind of store holds the phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStorageBackend {
    /// In the state file ([`FileKeyStore`]).
    File,
    /// In the iOS Keychain.
    Keychain,
}

/// What the key store reports about the stored phrase, for the app's settings and onboarding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyStorageStatus {
    /// Which store holds the phrase.
    pub backend: KeyStorageBackend,
    /// The phrase is kept on this device only: not synced to iCloud Keychain, not in backups.
    pub device_only: bool,
    /// The user hasn't yet chosen between iCloud Keychain sync and this device only. The key stays
    /// device-only until they do, and the app should ask.
    pub sync_choice_pending: bool,
    /// The user has confirmed they backed up the phrase.
    pub phrase_backed_up: bool,
    /// The app was reinstalled on this device and a key from before is still stored; see
    /// [`BingleLocalApi::restore_stored_key`](crate::api::BingleLocalApi::restore_stored_key).
    pub reinstall_detected: bool,
    /// The address of the stored current key, if there is one.
    pub stored_address: Option<String>,
    /// How many times the app has been installed on this device, as far as the store knows.
    pub install_count: u32,
    /// When the app was last installed on this device, in seconds since the Unix epoch.
    pub last_install_at: Option<u64>,
}

/// What can go wrong with a key store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyStoreError {
    /// This store can't do that: the sync and backup settings exist only for a platform store.
    Unavailable,
    /// Turning a synced key device-only removes it from the user's other devices, so the user must
    /// confirm the phrase is backed up first ([`BingleLocalApi::mark_phrase_backed_up`]).
    ///
    /// [`BingleLocalApi::mark_phrase_backed_up`]: crate::api::BingleLocalApi::mark_phrase_backed_up
    PhraseNotBackedUp,
    /// There is no stored key to act on.
    NoKey,
    /// The platform store failed.
    Storage(String),
}

impl fmt::Display for KeyStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyStoreError::Unavailable => write!(f, "key storage settings are not available here"),
            KeyStoreError::PhraseNotBackedUp => write!(
                f,
                "confirm the recovery phrase is backed up before keeping it on this device only"
            ),
            KeyStoreError::NoKey => write!(f, "no key is stored"),
            KeyStoreError::Storage(reason) => write!(f, "key storage failed: {reason}"),
        }
    }
}

impl std::error::Error for KeyStoreError {}

/// Keeps the recovery phrase for each [`KeySlot`].
///
/// No default methods: each store states all of its behaviour.
pub trait KeyStore: Send + Sync {
    /// Which kind of store this is.
    fn backend(&self) -> KeyStorageBackend;

    /// Whether the state file should carry the phrase. `true` for [`FileKeyStore`]; a platform
    /// store returns `false`, and the state file then keeps only the address.
    fn keeps_phrase_in_state_file(&self) -> bool;

    /// Stores `keypair` for `slot`, replacing what was there. A platform store creates a new key
    /// on this device only, never synced, until the user chooses sync.
    fn store(&self, slot: KeySlot, keypair: &Keypair) -> Result<(), KeyStoreError>;

    /// The stored keypair for `slot`, if there is one.
    fn fetch(&self, slot: KeySlot) -> Result<Option<Keypair>, KeyStoreError>;

    /// Removes the stored keypair for `slot`. Not an error if there is none.
    fn delete(&self, slot: KeySlot) -> Result<(), KeyStoreError>;

    /// What the store reports about the stored phrase.
    fn status(&self) -> Result<KeyStorageStatus, KeyStoreError>;

    /// Keeps the phrase on this device only (`true`) or syncs it through iCloud Keychain
    /// (`false`), and records that the user has chosen.
    fn set_device_only(&self, enabled: bool) -> Result<(), KeyStoreError>;

    /// Records that the user confirmed the phrase is backed up.
    fn mark_phrase_backed_up(&self) -> Result<(), KeyStoreError>;
}

/// Keeps the phrase in the state file, as the local API always has. Stores nothing itself.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileKeyStore;

impl KeyStore for FileKeyStore {
    fn backend(&self) -> KeyStorageBackend {
        KeyStorageBackend::File
    }

    fn keeps_phrase_in_state_file(&self) -> bool {
        true
    }

    fn store(&self, _slot: KeySlot, _keypair: &Keypair) -> Result<(), KeyStoreError> {
        Ok(())
    }

    fn fetch(&self, _slot: KeySlot) -> Result<Option<Keypair>, KeyStoreError> {
        Ok(None)
    }

    fn delete(&self, _slot: KeySlot) -> Result<(), KeyStoreError> {
        Ok(())
    }

    fn status(&self) -> Result<KeyStorageStatus, KeyStoreError> {
        Ok(KeyStorageStatus {
            backend: KeyStorageBackend::File,
            device_only: false,
            sync_choice_pending: false,
            phrase_backed_up: false,
            reinstall_detected: false,
            stored_address: None,
            install_count: 0,
            last_install_at: None,
        })
    }

    fn set_device_only(&self, _enabled: bool) -> Result<(), KeyStoreError> {
        Err(KeyStoreError::Unavailable)
    }

    fn mark_phrase_backed_up(&self) -> Result<(), KeyStoreError> {
        Err(KeyStoreError::Unavailable)
    }
}
