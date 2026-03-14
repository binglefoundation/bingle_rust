// Grouped relay tests

#[path = "relay_finder.rs"]
pub mod relay_finder;

#[path = "relay_finder_unit.rs"]
pub mod relay_finder_unit;

#[path = "relay_client_unit.rs"]
pub mod relay_client_unit;

#[path = "relay_states.rs"]
pub mod relay_states;

#[path = "relay_states_own.rs"]
pub mod relay_states_own;

#[path = "clear_state_cache.rs"]
pub mod clear_state_cache;

#[path = "exclude_self_from_ddb.rs"]
pub mod exclude_self_from_ddb;

#[path = "lookup_root_id.rs"]
pub mod lookup_root_id;

#[path = "list_all_relays_one_root.rs"]
pub mod list_all_relays_one_root;
