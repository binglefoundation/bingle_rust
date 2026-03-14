// Grouped tests for distributed_mutex module

#[path = "basic.rs"]
pub mod basic;

#[path = "modified_lamport.rs"]
pub mod modified_lamport;

#[path = "dynamic_add.rs"]
pub mod dynamic_add;

#[path = "islanding.rs"]
pub mod islanding;