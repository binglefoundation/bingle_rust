//! `unit` test target: the key store logic against the in-memory Keychain. No device needed.
//!
//! Each file under `tests/<module>/` mirrors a `src` module and is included here as a submodule.

#[path = "store/keychain_key_store.rs"]
mod store_keychain_key_store;

#[path = "store/local_api_migration.rs"]
mod store_local_api_migration;
