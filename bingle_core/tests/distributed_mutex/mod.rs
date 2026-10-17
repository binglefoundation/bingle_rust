// Grouped tests for distributed_mutex module

#[path = "basic.rs"]
pub mod basic;

#[path = "modified_lamport.rs"]
pub mod modified_lamport;

#[path = "lease_renewal.rs"]
pub mod lease_renewal;

#[path = "dynamic_add.rs"]
pub mod dynamic_add;

#[path = "divergent_views.rs"]
pub mod divergent_views;
