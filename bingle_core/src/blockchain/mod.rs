//! Bingle's Algorand integration: app/asset operations ([`AlgoBingle`]) such as handle
//! registration and lookup, built on the generic [`AlgoOps`](algo_ops::AlgoOps) helpers from the
//! external `algo_ops` crate.

/// Bingle's Algorand application and asset operations,
/// [`AlgoBingle`](crate::blockchain::algo_bingle::AlgoBingle).
pub mod algo_bingle;

/// One account's on-chain registration facts (Bingle$ opt-in, `Handle`, balance against the
/// required funding), read through [`AlgoBingle`](crate::blockchain::algo_bingle::AlgoBingle)
/// (issue #388).
pub mod account_status;

/// The shared Sidewinder node-endpoint record codec
/// ([`SidewinderEndpointRecord`](crate::blockchain::sidewinder_endpoint::SidewinderEndpointRecord)),
/// byte-compatible with Sidewinder `sw-membership`'s `EndpointRecord` (issue #237).
pub mod sidewinder_endpoint;

/// Checking and co-signing the registration service's unsigned registration and handle clash
/// recovery groups (issue #344).
pub mod registration_group;
