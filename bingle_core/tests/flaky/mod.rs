// Flaky or currently broken tests.
// These tests are known to be unreliable or to need fixes.
// Referenced from flaky_all.rs.

// Distributed mutex tests that need fixes for failing-node scenarios.
// Note: dynamic_add.rs and islanding.rs each declare `pub mod common`
// which resolves to tests/distributed_mutex/common.rs via the #[path] mechanism.
#[path = "../distributed_mutex/dynamic_add.rs"]
pub mod dynamic_add;
#[path = "../distributed_mutex/islanding.rs"]
pub mod islanding;

// Extracted from security/renegotiation_test.rs — only the live-handshake test is
// flaky; its companion config-only test stays in the unit suite.
pub mod renegotiation_handshake;
// Extracted from security/weak_key_exchange_test.rs — the client-cert handshake test
// was flagged leaky under load; its two sibling tests stay in the unit suite.
pub mod weak_key_exchange_rsa_client;

// Extracted from distributed_mutex/modified_lamport.rs — the 3-node mutual-exclusion
// test is timing-sensitive and fails intermittently; its sibling stays in the unit suite.
pub mod modified_lamport_mutex_3_nodes;
