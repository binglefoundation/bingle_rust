// Flaky or currently broken tests.
// Run with: cargo test --test flaky
//
// These tests are NOT run by default (cargo test).
// They may fail intermittently or require investigation before they can be promoted.
// #[ignore] annotations have been removed; separation is via this target.

#[macro_use]
pub mod util;

// ddb module needed by dtls_app_layer_verification
#[path = "ddb.rs"]
pub mod ddb;

// relay module needed by ddb/client/register_relay (referenced from ddb module)
pub mod relay;

pub mod flaky;
