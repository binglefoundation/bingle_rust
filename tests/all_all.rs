// All tests: unit + integration + flaky combined.
// Run with: cargo test --test all
//
// (number of all) = (number of unit) + (number of integration) + (number of flaky)
// No test appears in more than one of unit, integration, flaky.
//
// Requires RUST_COMMS_RUN_LOCALNET=true for localnet tests to execute.
// Requires RUST_COMMS_RUN_FLAKY=true for flaky tests to execute.
// Internet tests require live network access.

// Unit tests (local, no external resources) — uses full util with test files
#[path = "messages.rs"]
pub mod messages;
pub mod api;
pub mod blockchain;
pub mod dtls;
pub mod engine;
pub mod protocol;
pub mod packet_transport;
pub mod relay;
pub mod stun;
pub mod cli;
#[macro_use]
pub mod util;
pub mod ddb;
pub mod turn;
pub mod distributed_mutex;
pub mod setup_localnet;
pub mod module_version;
pub mod security;
#[path = "dev_arc4_selector.rs"]
pub mod dev_arc4_selector;

// Integration tests (localnet + internet) — included via integration_tests submodule
// Note: these share crate::util (above), crate::api, crate::relay, crate::ddb
pub mod integration_tests;

// Flaky tests — included via flaky submodule
pub mod flaky;
