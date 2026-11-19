//! The sealed state file (issue #459): the state JSON encrypted at rest, so a copy of the file is
//! useless without this device's key or the account's recovery phrase.
//!
//! The file is a [`SealedStore`] with up to two slots, each wrapping the one data key:
//!
//! - `device`: a [`SecretWrapper`] over a 32-byte secret the [`KeyStore`](crate::api::KeyStore)
//!   keeps on this device only (on iOS a Keychain item, `WhenUnlockedThisDeviceOnly`, never synced),
//!   for normal use.
//! - `recovery`: a [`SecretWrapper`] over a secret derived from the current account's recovery
//!   phrase, so a file restored from a backup to a new device opens once the user enters the
//!   phrase. Slot labels are stored in the clear, so the label doesn't name the account; instead
//!   the slot is rebuilt for the current account on the first save after a load, and again
//!   whenever the account changes. A file saved with no keypair has no recovery slot.
//!
//! The data key is kept in memory between saves, so a save in the background just after the
//! device locks still succeeds; [`forget_data_key`](SealedState::forget_data_key) wipes it, and the
//! next save reads the device secret again. A plain JSON file (from before sealing) still loads,
//! and the next save seals it.

use blockchain_account_vault::recovery::recovery_secret;
use blockchain_account_vault::sealed_store::{SealedStore, SealedStoreError};
use blockchain_keystore::secret::SecretWrapper;
use blockchain_keystore::{DataKey, KeystoreError, SecurityLevel};
use zeroize::Zeroizing;

use serde::{Deserialize, Serialize};

use crate::api::Keypair;

/// What the last [`load`](crate::api::BingleLocalApi::load) found, for the app to decide what to
/// show (issue #459).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateFileStatus {
    /// No state file has been loaded or saved yet.
    NotLoaded,
    /// A plain JSON state file is in use.
    Plain,
    /// A sealed state file is open.
    Sealed,
    /// The sealed state file couldn't be opened because the device is locked. Load again after
    /// the next unlock; saves to it are refused until then.
    Locked,
    /// The sealed state file doesn't open with this device's key: open it with the recovery phrase
    /// ([`open_state_with_recovery_phrase`](crate::api::BingleLocalApi::open_state_with_recovery_phrase)).
    /// Saves to it are refused until then.
    RecoveryPhraseRequired,
}

/// The device slot's label.
pub const DEVICE_SLOT: &str = "device";
/// The recovery slot's label.
pub const RECOVERY_SLOT: &str = "recovery";
/// The info string the recovery secret is derived with. Part of the file format: changing it stops
/// existing recovery slots opening.
pub const RECOVERY_SECRET_LABEL: &str = "bingle state file recovery v1";

/// Whether `bytes` are a sealed state file rather than plain JSON. A sealed store starts with its
/// format version byte; JSON starts with `{` or whitespace.
pub fn is_sealed(bytes: &[u8]) -> bool {
    bytes.first() == Some(&blockchain_account_vault::sealed_store::SEALED_STORE_VERSION)
}

/// Why a sealed state file didn't open or seal.
#[derive(Debug)]
pub enum SealError {
    /// The device secret doesn't open the file, or there is none: the file opens with the
    /// recovery phrase.
    NeedsRecoveryPhrase(String),
    /// The recovery phrase isn't valid, or doesn't open this file.
    WrongPhrase(String),
    /// The file is damaged or in a format this version doesn't read.
    Unreadable(String),
}

impl std::fmt::Display for SealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SealError::NeedsRecoveryPhrase(reason) => {
                write!(f, "this device's key doesn't open the state file: {reason}")
            }
            SealError::WrongPhrase(reason) => {
                write!(
                    f,
                    "the recovery phrase doesn't open the state file: {reason}"
                )
            }
            SealError::Unreadable(reason) => write!(f, "the state file can't be read: {reason}"),
        }
    }
}

/// A sealed state file's header and, while held, its data key.
#[derive(Default)]
pub struct SealedState {
    store: Option<SealedStore>,
    data_key: Option<DataKey>,
    // The address the recovery slot was made for this session; `None` until this session makes
    // one, so the first save after a load rebuilds it for the current account.
    recovery_for: Option<String>,
}

/// The device slot's wrapper for `secret`.
fn device_wrapper(secret: &[u8; 32]) -> SecretWrapper {
    SecretWrapper::new(secret, SecurityLevel::Software)
}

/// The recovery slot's wrapper for a 25-word `phrase`.
fn recovery_wrapper(phrase: &str) -> Result<SecretWrapper, SealError> {
    let secret = recovery_secret(phrase, RECOVERY_SECRET_LABEL)
        .map_err(|e| SealError::WrongPhrase(e.to_string()))?;
    Ok(SecretWrapper::new(&secret, SecurityLevel::Software))
}

fn unreadable(e: SealedStoreError) -> SealError {
    SealError::Unreadable(e.to_string())
}

/// Checks that the account's recovery `phrase` opens the sealed file `bytes`, changing nothing.
pub fn check_phrase(bytes: &[u8], phrase: &str) -> Result<(), SealError> {
    let store = SealedStore::from_bytes(bytes).map_err(unreadable)?;
    open_with_phrase(&store, phrase).map(|_| ())
}

/// Opens the sealed file `bytes` with the account's recovery `phrase` and gives it a new device
/// slot for `new_device_secret` in place of the old one, so it opens on this device from now on.
/// Returns the file's new bytes; the payload ciphertext is unchanged.
pub fn rekey_with_phrase(
    bytes: &[u8],
    phrase: &str,
    new_device_secret: &[u8; 32],
) -> Result<Vec<u8>, SealError> {
    let mut store = SealedStore::from_bytes(bytes).map_err(unreadable)?;
    let data_key = open_with_phrase(&store, phrase)?;
    if store.slots().iter().any(|s| s.label() == DEVICE_SLOT) {
        store
            .remove_slot(&data_key, DEVICE_SLOT)
            .map_err(unreadable)?;
    }
    store
        .add_slot(&data_key, DEVICE_SLOT, &device_wrapper(new_device_secret))
        .map_err(unreadable)?;
    store.to_bytes().map_err(unreadable)
}

fn open_with_phrase(store: &SealedStore, phrase: &str) -> Result<DataKey, SealError> {
    match store.open_any(&recovery_wrapper(phrase)?) {
        Ok((_, opened)) => Ok(opened.data_key),
        Err(SealedStoreError::NoSlotOpened) => Err(SealError::WrongPhrase(
            "it belongs to a different account".to_string(),
        )),
        Err(e) => Err(unreadable(e)),
    }
}

impl SealedState {
    /// Whether a file has been opened or sealed this session.
    pub fn is_open(&self) -> bool {
        self.store.is_some()
    }

    /// Whether the data key is held, so a save needs no device secret.
    pub fn holds_data_key(&self) -> bool {
        self.data_key.is_some()
    }

    /// Wipes the data key from memory. The next save reads the device secret again.
    pub fn forget_data_key(&mut self) {
        self.data_key = None;
    }

    /// Forgets the file altogether, as after the account is deleted.
    pub fn clear(&mut self) {
        self.store = None;
        self.data_key = None;
        self.recovery_for = None;
    }

    /// Opens `bytes` with the device `secret`, keeping the header and data key. Returns the JSON.
    pub fn open_with_device(
        &mut self,
        bytes: &[u8],
        secret: &[u8; 32],
    ) -> Result<Zeroizing<Vec<u8>>, SealError> {
        let store = SealedStore::from_bytes(bytes).map_err(unreadable)?;
        let opened = match store.open(DEVICE_SLOT, &device_wrapper(secret)) {
            Ok(opened) => opened,
            Err(SealedStoreError::Keystore(KeystoreError::WrongKeyOrTampered))
            | Err(SealedStoreError::NoSuchSlot(_)) => {
                return Err(SealError::NeedsRecoveryPhrase(
                    "the file was sealed on another device or install".to_string(),
                ));
            }
            Err(e) => return Err(unreadable(e)),
        };
        self.store = Some(store);
        self.data_key = Some(opened.data_key);
        self.recovery_for = None;
        Ok(opened.payload)
    }

    /// Gets the data key back after [`forget_data_key`](Self::forget_data_key), from the device
    /// `secret`.
    pub fn reload_data_key(&mut self, secret: &[u8; 32]) -> Result<(), SealError> {
        let Some(store) = &self.store else {
            return Err(SealError::Unreadable("no sealed file is open".to_string()));
        };
        let opened = store
            .open(DEVICE_SLOT, &device_wrapper(secret))
            .map_err(|e| SealError::NeedsRecoveryPhrase(e.to_string()))?;
        self.data_key = Some(opened.data_key);
        Ok(())
    }

    /// Seals `json` as a new file with a device slot for `secret` and, with a `current` keypair, a
    /// recovery slot for its phrase. The result is opened back with the device secret before it
    /// is returned, so a file that wouldn't open is never written.
    pub fn seal_new(
        &mut self,
        json: &[u8],
        secret: &[u8; 32],
        current: Option<&Keypair>,
    ) -> Result<Vec<u8>, SealError> {
        let device = device_wrapper(secret);
        let (store, data_key) = match current {
            Some(kp) => {
                let recovery = recovery_wrapper(&kp.passphrase)?;
                SealedStore::seal(json, &[(DEVICE_SLOT, &device), (RECOVERY_SLOT, &recovery)])
            }
            None => SealedStore::seal(json, &[(DEVICE_SLOT, &device)]),
        }
        .map_err(unreadable)?;
        let bytes = store.to_bytes().map_err(unreadable)?;
        let check = SealedStore::from_bytes(&bytes)
            .and_then(|s| s.open(DEVICE_SLOT, &device))
            .map_err(unreadable)?;
        if check.payload.as_slice() != json {
            return Err(SealError::Unreadable(
                "the sealed file didn't read back".to_string(),
            ));
        }
        self.store = Some(store);
        self.data_key = Some(data_key);
        self.recovery_for = current.map(|kp| kp.id.clone());
        Ok(bytes)
    }

    /// Seals `json` under the held data key and the open file's slots, first making the recovery
    /// slot match `current`: rebuilt when this session hasn't made it for that account (the first
    /// save after a load, or a changed account), and removed when there is no keypair. Errors if no
    /// file is open or the data key isn't held.
    pub fn reseal(&mut self, json: &[u8], current: Option<&Keypair>) -> Result<Vec<u8>, SealError> {
        let (Some(store), Some(data_key)) = (&mut self.store, &self.data_key) else {
            return Err(SealError::Unreadable(
                "no sealed file is open with its data key".to_string(),
            ));
        };
        let wanted = current.map(|kp| kp.id.clone());
        if self.recovery_for != wanted {
            if store.slots().iter().any(|s| s.label() == RECOVERY_SLOT) {
                store
                    .remove_slot(data_key, RECOVERY_SLOT)
                    .map_err(unreadable)?;
            }
            if let Some(kp) = current {
                store
                    .add_slot(data_key, RECOVERY_SLOT, &recovery_wrapper(&kp.passphrase)?)
                    .map_err(unreadable)?;
            }
            self.recovery_for = wanted;
        }
        let resealed = store.reseal(data_key, json).map_err(unreadable)?;
        let bytes = resealed.to_bytes().map_err(unreadable)?;
        *store = resealed;
        Ok(bytes)
    }
}
