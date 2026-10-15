// Flaky or currently broken tests.
// These tests are known to be unreliable or to need fixes.
// Referenced from flaky_all.rs.

// Extracted from security/renegotiation_test.rs — only the live-handshake test is
// flaky; its companion config-only test stays in the unit suite.
pub mod renegotiation_handshake;
// Extracted from security/weak_key_exchange_test.rs — the client-cert handshake test
// was flagged leaky under load; its two sibling tests stay in the unit suite.
pub mod weak_key_exchange_rsa_client;
