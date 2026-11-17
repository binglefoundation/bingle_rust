//! The data the key store keeps in each Keychain item. Each record carries a version, so a later
//! form (for example, a PIN-wrapped phrase) can be added without migrating old items.

use serde::{Deserialize, Serialize};

use crate::items::ItemError;

/// The record format version this crate writes.
pub const RECORD_VERSION: u32 = 1;

/// A stored key: the `key/<scope>` and `recovery/<scope>` items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRecord {
    /// The record format version.
    pub version: u32,
    /// The keypair's Algorand address.
    pub address: String,
    /// The 25-word recovery phrase.
    pub phrase: String,
    /// The user has confirmed the phrase is backed up.
    pub phrase_backed_up: bool,
    /// The user has chosen between iCloud Keychain sync and this device only.
    pub sync_chosen: bool,
    /// The key is kept on this device only.
    pub device_only: bool,
}

/// How many times the app has been installed on this device: the `install` item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRecord {
    /// The record format version.
    pub version: u32,
    /// Number of installs seen.
    pub count: u32,
    /// The first install, in seconds since the Unix epoch.
    pub first_at: u64,
    /// The latest install, in seconds since the Unix epoch.
    pub last_at: u64,
}

/// Encodes a record as item data.
pub fn encode<T: Serialize>(record: &T) -> Result<Vec<u8>, ItemError> {
    serde_json::to_vec(record).map_err(|e| ItemError::Other(format!("encoding a record: {e}")))
}

/// Decodes item data, refusing a version this crate doesn't read.
pub fn decode_key(data: &[u8]) -> Result<KeyRecord, ItemError> {
    let record: KeyRecord = serde_json::from_slice(data)
        .map_err(|e| ItemError::Other(format!("reading a key record: {e}")))?;
    if record.version != RECORD_VERSION {
        return Err(ItemError::Other(format!(
            "unsupported key record version {}",
            record.version
        )));
    }
    Ok(record)
}

/// Decodes the install item's data.
pub fn decode_install(data: &[u8]) -> Result<InstallRecord, ItemError> {
    serde_json::from_slice(data)
        .map_err(|e| ItemError::Other(format!("reading the install record: {e}")))
}
