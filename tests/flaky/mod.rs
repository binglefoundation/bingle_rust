// Flaky or currently broken tests.
// These tests are known to be unreliable or to need fixes.
// Referenced from flaky_all.rs.

// DTLS app-layer verification — flaky timing, marked broken
#[path = "../dtls/dtls_app_layer_verification.rs"]
pub mod dtls_app_layer_verification;

// Distributed mutex tests that need fixes for failing-node scenarios.
// Note: dynamic_add.rs and islanding.rs each declare `pub mod common`
// which resolves to tests/distributed_mutex/common.rs via the #[path] mechanism.
#[path = "../distributed_mutex/dynamic_add.rs"]
pub mod dynamic_add;
#[path = "../distributed_mutex/islanding.rs"]
pub mod islanding;
