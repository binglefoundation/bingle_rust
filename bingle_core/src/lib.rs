// Public API surface: `api` (engine), `blockchain` (Algorand), and the supported `util`
// submodules — see docs/api-surface.md for the contract. Modules marked `#[doc(hidden)]` remain
// `pub` only because the Java-style test tree (`tests/`) and other workspace crates import them;
// they are not part of the supported external API and are hidden from the published reference.
#[macro_use]
pub mod util;
pub mod api;
pub mod blockchain;
#[doc(hidden)]
pub mod ddb;
#[doc(hidden)]
pub mod distributed_mutex;
#[doc(hidden)]
pub mod dtls;
#[doc(hidden)]
pub mod engine;
#[doc(hidden)]
pub mod messages;
pub mod module_version;
#[doc(hidden)]
pub mod packet_transport;
#[doc(hidden)]
pub mod protocol;
#[doc(hidden)]
pub mod relay;
#[doc(hidden)]
pub mod stun;
// Logging theme constants — internal catalog, not a supported API.
#[doc(hidden)]
pub mod themes;
#[doc(hidden)]
pub mod turn;

// Backward-compatible module re-exports
pub use blockchain::algo_ops;

pub use blockchain::algo_bingle;

// New: export primary types for external users so they can import directly from the crate root
pub use crate::blockchain::algo_ops::AlgoOps;

pub use crate::blockchain::algo_bingle::AlgoBingle;
