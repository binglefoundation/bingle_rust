//! Keychain storage for the recovery phrase (issue #385).
//!
//! [`KeychainKeyStore`] implements [`KeyStore`](crate::api::KeyStore), so the 25-word phrase lives
//! in the Keychain and the state file keeps only the address. bingle_jsi uses it on iOS.
//!
//! - **New keys start on this device only.** Every new key (generated, imported, or moved out of
//!   an existing state file) is a `WhenUnlockedThisDeviceOnly` item that never syncs, with the
//!   sync choice pending. It reaches iCloud Keychain only when the app calls
//!   `set_device_only(false)` after the user chooses sync, so it is never uploaded before they can
//!   say no.
//! - **Device-only mode.** `set_device_only(true)` moves a synced key back to this device only.
//!   That deletes the synced item, which removes it from the user's other devices, so it is
//!   refused until the user has confirmed the phrase is backed up. A key that has never synced
//!   needs no such check.
//! - **A synced key isn't replaced silently.** Storing a different key over a synced one would
//!   delete the synced item, removing that account from every device on the user's Apple Account,
//!   so `store` refuses with `WouldReplaceSyncedKey` unless the caller passes `replace_synced`
//!   after the user agrees (issue #452). Completing a handle clash recovery passes it: the user
//!   already confirmed that move.
//! - **Safe switching.** The sync setting is part of a Keychain item's identity, so a switch
//!   writes the new item, reads it back, and only then deletes the old one.
//! - **Reinstall detection.** iOS deletes the app container on uninstall but keeps the Keychain. A
//!   missing install marker in the state directory increments the device-only `install` item's
//!   count, and a key that is still stored is reported as `reinstall_detected`.
//! - **A pending recovery key** (issue #344) is a separate item that follows the current key's
//!   sync setting.
//!
//! | Account | Contents | Synced | Accessibility |
//! |---|---|---|---|
//! | `key/<scope>` | versioned JSON: address, phrase, `phrase_backed_up`, `sync_chosen`, `device_only` | only after the user chooses sync | `WhenUnlockedThisDeviceOnly`, or `WhenUnlocked` when synced |
//! | `recovery/<scope>` | the pending recovery key, same format | follows `key` | follows `key` |
//! | `install` | versioned JSON: install count, first and last install time | never | `AfterFirstUnlockThisDeviceOnly` |
//!
//! The service is the app's bundle id with `.bingle` appended; the scope is the state file's stem,
//! so each network's state file has its own key.
//!
//! The store is built on [`SecureItems`], an add, read, update and delete interface over one
//! generic-password item. The real backend, `apple::KeychainItems`, is built only for Apple
//! targets; `memory::MemoryItems` (feature `test-support`) is an in-memory fake, so the logic is
//! tested on any host.
//!
//! The generic item layer (`SecureItems` and the Apple backend) is a candidate to move into
//! `blockchain_keystore` once the vault and the command-line tool need platform storage; see the
//! desktop backend spike, blockchain_ops_rust issue 141.

#[cfg(target_vendor = "apple")]
pub mod apple;
pub mod items;
#[cfg(feature = "test-support")]
pub mod memory;
pub mod record;
pub mod store;

pub use items::{Accessibility, ItemError, SecureItems};
pub use store::KeychainKeyStore;
