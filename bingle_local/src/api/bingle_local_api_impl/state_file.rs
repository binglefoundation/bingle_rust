//! The state file on disk for the [`BingleApiLocalImpl`] (issue #459): reading it (opening a
//! sealed file with this device's key), the bytes a save writes (sealed or plain), refusing to save
//! over a sealed file that didn't open, restoring a sealed file with the recovery phrase, and the
//! atomic write. Split out of `bingle_local_api_impl` to keep that file's size down; as a child
//! module it still reads the implementation's private state.

use super::BingleApiLocalImpl;
use crate::api::sealed_state::{self, SealError};
use crate::api::{BingleLocalApi, KeyStoreError, Keypair, StateFileStatus};
use bingle_core::api::bingle_api::BingleError;
use std::path::PathBuf;
use zeroize::Zeroizing;

impl BingleApiLocalImpl {
    /// Records the state file last loaded or saved, and what was found (issue #459).
    pub(super) fn set_state_file(&self, path: &str, status: StateFileStatus) {
        if let Ok(mut g) = self.state_file.lock() {
            *g = (Some(PathBuf::from(path)), status);
        }
    }

    /// The error for saving over `path` while its sealed file hasn't opened, if that is so.
    fn refuse_unopened(&self, path: &str) -> Result<(), BingleError> {
        let Ok(g) = self.state_file.lock() else {
            return Ok(());
        };
        let (Some(loaded), status) = &*g else {
            return Ok(());
        };
        if loaded.as_path() != std::path::Path::new(path) {
            return Ok(());
        }
        match status {
            StateFileStatus::Locked => Err(BingleError::StateLocked(format!(
                "'{path}' is sealed and the device is locked; it isn't saved over until it loads"
            ))),
            StateFileStatus::RecoveryPhraseRequired => {
                Err(BingleError::RecoveryPhraseRequired(format!(
                    "'{path}' is sealed and needs the recovery phrase; it isn't saved over until it opens"
                )))
            }
            _ => Ok(()),
        }
    }

    /// This device's state file secret, made if `create` and missing.
    fn device_secret(&self, create: bool) -> Result<Option<Zeroizing<[u8; 32]>>, BingleError> {
        self.key_store.state_secret(create).map_err(|e| match e {
            KeyStoreError::Locked => BingleError::StateLocked(e.to_string()),
            other => BingleError::Other(other.to_string()),
        })
    }

    /// Reads the state file at `path` as JSON, opening it with this device's key if it is sealed.
    /// Returns the JSON and whether the file was sealed. A sealed file that doesn't open is
    /// recorded as [`StateFileStatus::Locked`] or [`StateFileStatus::RecoveryPhraseRequired`],
    /// with the matching error.
    pub(super) fn read_state_json(
        &self,
        path: &str,
    ) -> Result<(Zeroizing<Vec<u8>>, bool), BingleError> {
        let bytes = std::fs::read(path).map_err(|e| {
            tracing::error!("[load] Failed to read file '{}': {}", path, e);
            BingleError::Other(e.to_string())
        })?;
        if !sealed_state::is_sealed(&bytes) {
            if let Ok(mut sealed) = self.sealed.lock() {
                sealed.clear();
            }
            return Ok((Zeroizing::new(bytes), false));
        }
        if !self.key_store.holds_state_secret() {
            return Err(BingleError::Other(format!(
                "'{path}' is sealed, and this key store can't open it"
            )));
        }
        let secret = match self.device_secret(false) {
            Ok(Some(secret)) => secret,
            Ok(None) => {
                self.set_state_file(path, StateFileStatus::RecoveryPhraseRequired);
                return Err(BingleError::RecoveryPhraseRequired(format!(
                    "'{path}' is sealed and this device has no key for it"
                )));
            }
            Err(e) => {
                if matches!(e, BingleError::StateLocked(_)) {
                    self.set_state_file(path, StateFileStatus::Locked);
                }
                return Err(e);
            }
        };
        let mut sealed = self
            .sealed
            .lock()
            .map_err(|e| BingleError::Other(format!("mutex poisoned: {e}")))?;
        match sealed.open_with_device(&bytes, &secret) {
            Ok(json) => Ok((json, true)),
            Err(SealError::NeedsRecoveryPhrase(reason)) => {
                self.set_state_file(path, StateFileStatus::RecoveryPhraseRequired);
                Err(BingleError::RecoveryPhraseRequired(reason))
            }
            Err(e) => Err(BingleError::Other(e.to_string())),
        }
    }

    /// The bytes to write for the state `json` at `path`: sealed when sealing is on or the file is
    /// already sealed, plain JSON otherwise. Refuses a sealed file that didn't open, and never
    /// falls back to plain JSON for one that should be sealed.
    pub(super) fn state_bytes_for_save(
        &self,
        path: &str,
        json: Zeroizing<Vec<u8>>,
        current: Option<&Keypair>,
    ) -> Result<Vec<u8>, BingleError> {
        self.refuse_unopened(path)?;
        let mut sealed = self
            .sealed
            .lock()
            .map_err(|e| BingleError::Other(format!("mutex poisoned: {e}")))?;
        if !self.config().encrypt_state && !sealed.is_open() {
            return Ok(json.to_vec());
        }
        if !self.key_store.holds_state_secret() {
            return Err(BingleError::Other(
                "encrypt_state needs a key store that holds a device secret (the Keychain)"
                    .to_string(),
            ));
        }
        let result = if sealed.is_open() {
            if !sealed.holds_data_key() {
                let Some(secret) = self.device_secret(false)? else {
                    return Err(BingleError::RecoveryPhraseRequired(
                        "this device's state file key is gone".to_string(),
                    ));
                };
                sealed
                    .reload_data_key(&secret)
                    .map_err(|e| BingleError::Other(e.to_string()))?;
            }
            sealed.reseal(&json, current)
        } else {
            let Some(secret) = self.device_secret(true)? else {
                return Err(BingleError::Other(
                    "the key store made no state file secret".to_string(),
                ));
            };
            sealed.seal_new(&json, &secret, current)
        };
        let bytes = result.map_err(|e| BingleError::Other(e.to_string()))?;
        drop(sealed);
        self.set_state_file(path, StateFileStatus::Sealed);
        Ok(bytes)
    }

    /// The body of [`open_state_with_recovery_phrase`](BingleLocalApi::open_state_with_recovery_phrase).
    pub(super) fn restore_with_phrase(
        &mut self,
        path: &str,
        phrase: String,
    ) -> Result<Keypair, BingleError> {
        let bytes = std::fs::read(path).map_err(|e| BingleError::Other(e.to_string()))?;
        if !sealed_state::is_sealed(&bytes) {
            return Err(BingleError::Other(format!("'{path}' isn't sealed")));
        }
        // Check the phrase before touching this device's key, so a wrong one changes nothing.
        let phrase_error = |e: SealError| match e {
            SealError::WrongPhrase(reason) => BingleError::RecoveryPhraseRequired(reason),
            other => BingleError::Other(other.to_string()),
        };
        sealed_state::check_phrase(&bytes, &phrase).map_err(phrase_error)?;
        self.key_store
            .delete_state_secret()
            .map_err(|e| BingleError::Other(e.to_string()))?;
        let Some(secret) = self.device_secret(true)? else {
            return Err(BingleError::Other(
                "the key store made no state file secret".to_string(),
            ));
        };
        let rekeyed =
            sealed_state::rekey_with_phrase(&bytes, &phrase, &secret).map_err(phrase_error)?;
        write_atomically(path, &rekeyed)?;
        self.load(path)?;
        // The phrase's file is open on this device now, so its key becomes current here, as a
        // restore with the phrase does.
        let keypair = self.import_keypair_shared(phrase, true)?;
        self.save(path)?;
        tracing::info!(
            "[BingleLocalApi] Opened '{}' with the recovery phrase",
            path
        );
        Ok(keypair)
    }
}

/// Writes `bytes` to a uniquely named temp file beside `path`, then renames it over `path`. The
/// rename is atomic, so a reader never sees a half-written file and two threads saving at once (the
/// pending-message sender and the client, issue #283) each land a whole snapshot.
pub(super) fn write_atomically(path: &str, bytes: &[u8]) -> Result<(), BingleError> {
    if let Some(parent) = std::path::Path::new(path).parent()
        && !parent.as_os_str().is_empty()
        && !parent.is_dir()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        let msg = format!(
            "Failed to create parent directory '{}' for '{}': {}",
            parent.display(),
            path,
            e
        );
        tracing::error!("[save] {}", msg);
        return Err(BingleError::Other(msg));
    }
    static SAVE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SAVE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp_path = format!("{path}.tmp.{}.{seq}", std::process::id());
    if let Err(e) = std::fs::write(&tmp_path, bytes) {
        let msg = e.to_string();
        tracing::error!("[save] Failed to write '{}': {}", tmp_path, msg);
        let _ = std::fs::remove_file(&tmp_path);
        return Err(BingleError::Other(msg));
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let msg = e.to_string();
        tracing::error!("[save] Failed to replace '{}': {}", path, msg);
        let _ = std::fs::remove_file(&tmp_path);
        return Err(BingleError::Other(msg));
    }
    Ok(())
}
