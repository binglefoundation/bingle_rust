//! The Bingle messaging engine API: the [`BingleApi`](crate::api::bingle_api::BingleApi) trait, its
//! [`BingleApiImpl`](crate::api::bingle_api_impl::BingleApiImpl) implementation, and supporting
//! endpoint types.

/// The messaging engine trait and its associated error, option, and callback types.
pub mod bingle_api;
/// The concrete [`BingleApi`](crate::api::bingle_api::BingleApi) implementation,
/// [`BingleApiImpl`](crate::api::bingle_api_impl::BingleApiImpl).
pub mod bingle_api_impl;
/// Endpoint addressing types used by the engine.
pub mod network_endpoint;
// Internal public-key-infrastructure helpers; reachable for the test tree, not a supported API.
#[doc(hidden)]
pub mod pki;
