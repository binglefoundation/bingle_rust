//! The interface the key store uses to reach one Keychain item.

use std::fmt;

/// When the item can be read, and whether it may leave the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accessibility {
    /// Readable only while the device is unlocked; never leaves this device (not synced, not in
    /// backups, not transferred to a new phone).
    WhenUnlockedThisDeviceOnly,
    /// Readable only while the device is unlocked; may sync through iCloud Keychain.
    WhenUnlocked,
    /// Readable after the first unlock since boot; never leaves this device.
    AfterFirstUnlockThisDeviceOnly,
}

/// What can go wrong with a Keychain item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemError {
    /// An item with this account and sync setting already exists.
    Duplicate,
    /// The Keychain refused the operation, with its status code.
    Status(i32),
    /// Anything else, such as unreadable item data.
    Other(String),
}

impl fmt::Display for ItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemError::Duplicate => write!(f, "the Keychain item already exists"),
            ItemError::Status(code) => write!(f, "the Keychain returned status {code}"),
            ItemError::Other(reason) => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for ItemError {}

/// Adds, reads, updates and deletes one generic-password Keychain item.
///
/// An item is identified by its account and by whether it is synced: the Keychain keeps synced
/// and unsynced items apart, so the same account can have one of each. Changing an item's sync
/// setting is therefore an add of the other kind followed by a delete, never an update.
///
/// No default methods: each backend states all of its behaviour.
pub trait SecureItems: Send + Sync {
    /// The item's data, or `None` if there is no such item.
    fn read(&self, account: &str, synced: bool) -> Result<Option<Vec<u8>>, ItemError>;

    /// Adds a new item. Fails with [`ItemError::Duplicate`] if it already exists. A synced item
    /// must use [`Accessibility::WhenUnlocked`]: a `ThisDeviceOnly` item can't sync.
    fn add(
        &self,
        account: &str,
        synced: bool,
        accessibility: Accessibility,
        data: &[u8],
    ) -> Result<(), ItemError>;

    /// Replaces an existing item's data, keeping its accessibility.
    fn update(&self, account: &str, synced: bool, data: &[u8]) -> Result<(), ItemError>;

    /// Deletes the item. Not an error if there is none.
    fn delete(&self, account: &str, synced: bool) -> Result<(), ItemError>;
}
