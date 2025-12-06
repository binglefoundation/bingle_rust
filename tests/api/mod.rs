// Grouped API tests as submodules of the single all-tests crate

#[path = "bingle_api_impl_integration.rs"]
mod bingle_api_impl_integration;

#[path = "bingle_api_impl_unit.rs"]
mod bingle_api_impl_unit;

#[path = "pki_generate_pki_from_ops.rs"]
mod pki_generate_pki_from_ops;

#[path = "endpoint_identify_integration.rs"]
mod endpoint_identify_integration;

#[path = "bingle_api_start_fail.rs"]
mod bingle_api_start_fail;

#[path = "bingle_api_relay_check_two_nodes.rs"]
mod bingle_api_relay_check_two_nodes;
