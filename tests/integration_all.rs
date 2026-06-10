// Integration tests: tests that require external resources (algokit localnet or internet).
// Run with: cargo test --test integration
//
// Localnet tests fail if algokit localnet is not running.
// Internet tests require live network access.
//
// Uses util_support (helpers only, no util test files) so util tests don't appear here.
#[macro_use]
#[path = "util_support/mod.rs"]
pub mod util;
pub mod setup_localnet;
pub mod integration_tests;
