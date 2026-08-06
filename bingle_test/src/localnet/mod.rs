//! Reusable algokit-localnet integration harness.
//!
//! Ported from `bingle_core`'s test tree so integration tests in **other** crates (e.g. the
//! `bingle_cli` chat e2e) can provision the same localnet environment — funded accounts, a deployed
//! Bingle app + asset, registered handles, root relays and STUN servers — and drive real flows
//! against it. Requires a running `algokit localnet` (algod `localhost:4001`, indexer
//! `localhost:8980`) and the `algokit`/`goal` CLIs on `PATH` for account funding.
//!
//! Gated behind the `localnet` feature (which enables `bingle_core/test-hooks` for the in-process
//! relay/client control used here).

pub mod provision;
pub mod relay_test_util;
pub mod setup_localnet;
pub mod test_util;
