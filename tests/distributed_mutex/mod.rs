// Grouped tests for distributed_mutex module

#[path = "basic.rs"]
mod basic;

#[path = "modified_lamport.rs"]
mod modified_lamport;

#[path = "dynamic_add.rs"]
mod dynamic_add;

#[path = "islanding.rs"]
mod islanding;