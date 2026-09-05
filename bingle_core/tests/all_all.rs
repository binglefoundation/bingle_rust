// All tests: unit + integration + flaky combined.
// Run with: cargo test --test all
//
// (number of all) = (number of unit) + (number of integration) + (number of flaky)
// No test appears in more than one of unit, integration, flaky.
//
// Localnet tests fail if algokit localnet is not running.
// Flaky tests always run; they are target-driven.
// Internet tests require live network access.

// Unit tests (local, no external resources) — uses full util with test files
pub mod api;
pub mod blockchain;
pub mod crypto;
pub mod dtls;
pub mod engine;
#[path = "messages.rs"]
pub mod messages;
pub mod packet_transport;
pub mod protocol;
pub mod relay;
pub mod stun;
#[macro_use]
pub mod util;
pub mod ddb;
#[path = "dev_arc4_selector.rs"]
pub mod dev_arc4_selector;
pub mod distributed_mutex;
pub mod module_version;
pub mod security;
pub mod setup_localnet;
pub mod turn;

// Integration tests (localnet + internet) — included via integration_tests submodule
// Note: these share crate::util (above), crate::api, crate::relay, crate::ddb
pub mod integration_tests;

// Flaky tests — included via flaky submodule
pub mod flaky;
