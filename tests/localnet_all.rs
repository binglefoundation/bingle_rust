// Tests that require a running Algorand localnet (algokit localnet start).
// Run with: cargo test --test localnet
//
// These tests are NOT run by default (cargo test) — they need localnet infrastructure.
// All #[ignore] annotations have been removed from the tests in this target.

#[macro_use]
pub mod util;

pub mod setup_localnet;

// api module needed because some localnet tests reference crate::api::bingle_api_handle_tests
pub mod api;

// relay module needed by relay_updater_localnet and api tests that use crate::relay::relay_states
pub mod relay;

// ddb module needed by relay/unavailable_relays (uses crate::ddb::ddb_client_lookup)
#[path = "ddb.rs"]
pub mod ddb;

// All localnet-requiring tests
pub mod localnet;
