//! iOS Keychain storage for the Bingle recovery phrase (issue #385).
//!
//! [`KeychainKeyStore`] implements `bingle_local`'s
//! [`KeyStore`](bingle_local::api::KeyStore), so the 25-word phrase lives in the Keychain and the
//! state file keeps only the address.
//!
//! - Every new key is created on this device only (`WhenUnlockedThisDeviceOnly`, never synced),
//!   and moves to iCloud Keychain only once the user chooses sync, so it never reaches iCloud
//!   before they can say no.
//! - "Keep key material on this device only" moves it back, but only after the user confirms the
//!   phrase is backed up, since that removes it from their other devices.
//! - An install marker in the state directory and a device-only `install` item detect a reinstall
//!   and count installs.
//!
//! The store is built on [`SecureItems`], a small add, read, update and delete interface over one
//! Keychain item. [`KeychainItems`] is the real backend on Apple platforms; `MemoryItems` (feature
//! `test-support`) is an in-memory fake, so the logic is tested on the host.

#![warn(missing_docs)]

pub mod items;
#[cfg(target_vendor = "apple")]
pub mod keychain;
#[cfg(feature = "test-support")]
pub mod memory;
pub mod record;
pub mod store;

pub use items::{Accessibility, ItemError, SecureItems};
#[cfg(target_vendor = "apple")]
pub use keychain::KeychainItems;
pub use store::KeychainKeyStore;
