// Grouped relay tests

#[path = "relay_finder.rs"]
mod relay_finder;

#[path = "relay_finder_unit.rs"]
mod relay_finder_unit;

#[path = "relay_client_unit.rs"]
mod relay_client_unit;

#[path = "relay_states.rs"]
mod relay_states;

#[path = "relay_states_own.rs"]
mod relay_states_own;

#[path = "clear_state_cache.rs"]
mod clear_state_cache;

#[path = "exclude_self_from_ddb.rs"]
mod exclude_self_from_ddb;

#[path = "lookup_root_id.rs"]
mod lookup_root_id;
