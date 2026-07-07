// Flaky or currently broken tests.
// These tests are known to be unreliable or to need fixes.
// Referenced from flaky_all.rs.
// DTLS app-layer verification — flaky timing, marked broken
#[path = "../dtls/dtls_app_layer_verification.rs"]
pub mod dtls_app_layer_verification;
#[path = "../dtls/dtls_client_peer_cert_rejection.rs"]
pub mod dtls_client_peer_cert_rejection;

// Distributed mutex tests that need fixes for failing-node scenarios.
// Note: dynamic_add.rs and islanding.rs each declare `pub mod common`
// which resolves to tests/distributed_mutex/common.rs via the #[path] mechanism.
#[path = "../distributed_mutex/dynamic_add.rs"]
pub mod dynamic_add;
#[path = "../distributed_mutex/islanding.rs"]
pub mod islanding;

// Relay/engine and DTLS networking tests found to intermittently time out, fail,
// or segfault under load (identified via 20x repeated runs of the unit suite).
// These bind sockets, sleep, and do live loopback handshakes.
#[path = "../engine/relay_keep_alive_engine.rs"]
pub mod relay_keep_alive_engine;
#[path = "../engine/stun_inconsistent_relay.rs"]
pub mod stun_inconsistent_relay;
#[path = "../security/dtls_session_randomness_test.rs"]
pub mod dtls_session_randomness_test;
#[path = "../api/dtls_via_relay_integration.rs"]
pub mod dtls_via_relay_integration;
// Extracted from security/renegotiation_test.rs — only the live-handshake test is
// flaky; its companion config-only test stays in the unit suite.
pub mod renegotiation_handshake;
// Extracted from security/weak_key_exchange_test.rs — the client-cert handshake test
// was flagged leaky under load; its two sibling tests stay in the unit suite.
pub mod weak_key_exchange_rsa_client;
