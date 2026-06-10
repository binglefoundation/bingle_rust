// Tests that require live internet access (external STUN servers, testnet, etc).
// Run with: cargo test --test internet
//
// These tests are NOT run by default (cargo test).
// All #[ignore] annotations have been removed from the tests in this target.

// util module needed by stun_live_udp_mux (uses crate::util::test_util::init_test_logging)
#[macro_use]
pub mod util;

pub mod internet;
