pub mod blockchain;
pub mod dtls;
pub mod api;
pub mod relay;
pub mod stun;
pub mod messages;
pub mod engine;
pub mod protocol;
pub mod util;

// Backward-compatible module re-exports
#[cfg(not(target_os = "ios"))]
pub use blockchain::algo_ops;

#[cfg(not(target_os = "ios"))]
pub use blockchain::algo_bingle;

// New: export primary types for external users so they can import directly from the crate root
#[cfg(not(target_os = "ios"))]
pub use crate::blockchain::algo_ops::AlgoOps;

#[cfg(not(target_os = "ios"))]
pub use crate::blockchain::algo_bingle::AlgoBingle;
