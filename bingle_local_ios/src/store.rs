//! [`KeychainKeyStore`]: the recovery phrase in the Keychain, with the sync choice made before the
//! key ever reaches iCloud.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bingle_local::api::{
    KeySlot, KeyStorageBackend, KeyStorageStatus, KeyStore, KeyStoreError, Keypair,
};

use crate::items::{Accessibility, ItemError, SecureItems};
use crate::record::{InstallRecord, KeyRecord, RECORD_VERSION, decode_install, decode_key, encode};

/// The account of the per-device install item.
const INSTALL_ACCOUNT: &str = "install";
/// The install marker's file name in the state directory. iOS deletes the app container on
/// uninstall but keeps the Keychain, so a missing marker with a stored key means a reinstall.
const INSTALL_MARKER: &str = ".bingle_install_marker";

/// Keeps the recovery phrase in the Keychain, one item per [`KeySlot`] and state-file scope.
///
/// - A new key is always created device-only (`WhenUnlockedThisDeviceOnly`, not synced).
/// - [`set_device_only(false)`](KeyStore::set_device_only) moves it to a synced item; `true`
///   moves a synced key back, once the user has confirmed the phrase is backed up.
/// - A switch writes the new item, reads it back, and only then deletes the old one, so a failure
///   leaves the old item in place.
pub struct KeychainKeyStore {
    items: Arc<dyn SecureItems>,
    scope: String,
    reinstall_detected: AtomicBool,
}

impl KeychainKeyStore {
    /// A store whose items are named after `scope` (the state file's stem, so each network's state
    /// file has its own key), using `items` for the Keychain.
    ///
    /// Also records this launch: if `state_dir` has no install marker, the install count goes up,
    /// a stored key means the app was reinstalled, and the marker is written. A failure here is
    /// logged and doesn't stop the store working.
    pub fn new(items: Arc<dyn SecureItems>, scope: &str, state_dir: &Path) -> Self {
        let store = Self {
            items,
            scope: scope.to_string(),
            reinstall_detected: AtomicBool::new(false),
        };
        store.note_launch(state_dir);
        store
    }

    /// The real Keychain store for the state file at `state_file`: the items' service is the app's
    /// bundle id with `.bingle` appended, and their scope is the file's stem.
    #[cfg(target_vendor = "apple")]
    pub fn for_state_file(state_file: &Path) -> Self {
        let scope = state_file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "bingle_state".to_string());
        let state_dir = state_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let items = crate::keychain::KeychainItems::for_main_bundle();
        Self::new(Arc::new(items), &scope, &state_dir)
    }

    fn account(&self, slot: KeySlot) -> String {
        match slot {
            KeySlot::Current => format!("key/{}", self.scope),
            KeySlot::Recovery => format!("recovery/{}", self.scope),
        }
    }

    /// The stored record for `slot` and whether it is the synced item. The device-only item wins
    /// if both exist (a switch that stopped between its write and its delete); the next switch
    /// removes the other.
    fn read_record(&self, slot: KeySlot) -> Result<Option<(KeyRecord, bool)>, KeyStoreError> {
        let account = self.account(slot);
        for synced in [false, true] {
            if let Some(data) = self.items.read(&account, synced).map_err(storage)? {
                return Ok(Some((decode_key(&data).map_err(storage)?, synced)));
            }
        }
        Ok(None)
    }

    /// Writes `record` as the item of the given sync setting: an update if it exists, otherwise an
    /// add with the matching accessibility. Then reads it back.
    fn write_record(
        &self,
        slot: KeySlot,
        synced: bool,
        record: &KeyRecord,
    ) -> Result<(), KeyStoreError> {
        let account = self.account(slot);
        let data = encode(record).map_err(storage)?;
        let accessibility = if synced {
            Accessibility::WhenUnlocked
        } else {
            Accessibility::WhenUnlockedThisDeviceOnly
        };
        match self.items.read(&account, synced).map_err(storage)? {
            Some(_) => self
                .items
                .update(&account, synced, &data)
                .map_err(storage)?,
            None => self
                .items
                .add(&account, synced, accessibility, &data)
                .map_err(storage)?,
        }
        match self.items.read(&account, synced).map_err(storage)? {
            Some(stored) if decode_key(&stored).map_err(storage)? == *record => Ok(()),
            _ => Err(KeyStoreError::Storage(
                "the Keychain item did not read back the same".to_string(),
            )),
        }
    }

    /// Moves `slot`'s key to the given mode, recording that the user has chosen. `check_backup`
    /// refuses to make a synced key device-only before the phrase is backed up.
    fn switch(
        &self,
        slot: KeySlot,
        device_only: bool,
        check_backup: bool,
    ) -> Result<(), KeyStoreError> {
        let Some((record, synced)) = self.read_record(slot)? else {
            return Err(KeyStoreError::NoKey);
        };
        let wanted_synced = !device_only;
        let updated = KeyRecord {
            sync_chosen: true,
            device_only,
            ..record.clone()
        };
        if synced == wanted_synced {
            // Already in the wanted mode: record the choice, and clear any leftover of the other.
            self.write_record(slot, synced, &updated)?;
            return self
                .items
                .delete(&self.account(slot), !synced)
                .map_err(storage);
        }
        if device_only && check_backup && !record.phrase_backed_up {
            return Err(KeyStoreError::PhraseNotBackedUp);
        }
        self.write_record(slot, wanted_synced, &updated)?;
        self.items
            .delete(&self.account(slot), synced)
            .map_err(storage)
    }

    /// Counts this launch if it is the first since install, and detects a reinstall.
    fn note_launch(&self, state_dir: &Path) {
        let marker: PathBuf = state_dir.join(INSTALL_MARKER);
        if marker.exists() {
            return;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Err(e) = self.count_install(now) {
            tracing::warn!("[keychain] couldn't update the install count: {}", e);
        }
        match self.read_record(KeySlot::Current) {
            Ok(Some((record, _))) => {
                tracing::info!(
                    "[keychain] reinstall detected; a key for {} is still stored",
                    record.address
                );
                self.reinstall_detected.store(true, Ordering::SeqCst);
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("[keychain] couldn't check for a stored key: {}", e),
        }
        if let Err(e) =
            std::fs::create_dir_all(state_dir).and_then(|()| std::fs::write(&marker, b""))
        {
            tracing::warn!(
                "[keychain] couldn't write the install marker {}: {}",
                marker.display(),
                e
            );
        }
    }

    fn count_install(&self, now: u64) -> Result<(), ItemError> {
        match self.items.read(INSTALL_ACCOUNT, false)? {
            Some(data) => {
                let previous = decode_install(&data)?;
                let record = InstallRecord {
                    version: RECORD_VERSION,
                    count: previous.count.saturating_add(1),
                    first_at: previous.first_at,
                    last_at: now,
                };
                self.items.update(INSTALL_ACCOUNT, false, &encode(&record)?)
            }
            None => {
                let record = InstallRecord {
                    version: RECORD_VERSION,
                    count: 1,
                    first_at: now,
                    last_at: now,
                };
                self.items.add(
                    INSTALL_ACCOUNT,
                    false,
                    Accessibility::AfterFirstUnlockThisDeviceOnly,
                    &encode(&record)?,
                )
            }
        }
    }

    fn install_record(&self) -> Option<InstallRecord> {
        match self.items.read(INSTALL_ACCOUNT, false) {
            Ok(Some(data)) => decode_install(&data).ok(),
            _ => None,
        }
    }
}

impl KeyStore for KeychainKeyStore {
    fn backend(&self) -> KeyStorageBackend {
        KeyStorageBackend::Keychain
    }

    fn keeps_phrase_in_state_file(&self) -> bool {
        false
    }

    fn store(&self, slot: KeySlot, keypair: &Keypair) -> Result<(), KeyStoreError> {
        let existing = self.read_record(slot)?;
        if let Some((record, _)) = &existing
            && record.address == keypair.id
            && record.phrase == keypair.passphrase
        {
            // Already stored, in whatever mode the user chose.
            return Ok(());
        }
        // A new key starts device-only, with no choice made, so it never reaches iCloud before the
        // user chooses sync.
        let record = KeyRecord {
            version: RECORD_VERSION,
            address: keypair.id.clone(),
            phrase: keypair.passphrase.clone(),
            phrase_backed_up: false,
            sync_chosen: false,
            device_only: true,
        };
        self.write_record(slot, false, &record)?;
        if let Some((old, true)) = existing {
            // The slot held a different, synced key (the account a completed recovery left behind,
            // or one replaced by importing another). Remove it, which also removes it from the
            // user's other devices.
            tracing::warn!(
                "[keychain] replacing the synced {:?} key for {} with {}",
                slot,
                old.address,
                keypair.id
            );
            self.items
                .delete(&self.account(slot), true)
                .map_err(storage)?;
        }
        Ok(())
    }

    fn fetch(&self, slot: KeySlot) -> Result<Option<Keypair>, KeyStoreError> {
        Ok(self.read_record(slot)?.map(|(record, _)| Keypair {
            id: record.address,
            passphrase: record.phrase,
        }))
    }

    fn delete(&self, slot: KeySlot) -> Result<(), KeyStoreError> {
        let account = self.account(slot);
        for synced in [false, true] {
            self.items.delete(&account, synced).map_err(storage)?;
        }
        Ok(())
    }

    fn status(&self) -> Result<KeyStorageStatus, KeyStoreError> {
        let current = self.read_record(KeySlot::Current)?;
        let install = self.install_record();
        Ok(KeyStorageStatus {
            backend: KeyStorageBackend::Keychain,
            device_only: current.as_ref().map(|(_, synced)| !synced).unwrap_or(true),
            sync_choice_pending: current
                .as_ref()
                .map(|(record, _)| !record.sync_chosen)
                .unwrap_or(false),
            phrase_backed_up: current
                .as_ref()
                .map(|(record, _)| record.phrase_backed_up)
                .unwrap_or(false),
            reinstall_detected: self.reinstall_detected.load(Ordering::SeqCst),
            stored_address: current.map(|(record, _)| record.address),
            install_count: install.as_ref().map(|i| i.count).unwrap_or(0),
            last_install_at: install.map(|i| i.last_at),
        })
    }

    fn set_device_only(&self, enabled: bool) -> Result<(), KeyStoreError> {
        // The current key decides, including the backup check; a pending recovery key follows it.
        self.switch(KeySlot::Current, enabled, true)?;
        if self.read_record(KeySlot::Recovery)?.is_some() {
            self.switch(KeySlot::Recovery, enabled, false)?;
        }
        Ok(())
    }

    fn mark_phrase_backed_up(&self) -> Result<(), KeyStoreError> {
        let Some((record, synced)) = self.read_record(KeySlot::Current)? else {
            return Err(KeyStoreError::NoKey);
        };
        let updated = KeyRecord {
            phrase_backed_up: true,
            ..record
        };
        self.write_record(KeySlot::Current, synced, &updated)
    }
}

fn storage(e: ItemError) -> KeyStoreError {
    KeyStoreError::Storage(e.to_string())
}
