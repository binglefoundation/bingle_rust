// Grouped relay tests

#[path = "relay_finder.rs"]
pub mod relay_finder;

#[path = "relay_finder_unit.rs"]
pub mod relay_finder_unit;

#[path = "relay_info_cache.rs"]
pub mod relay_info_cache;

#[path = "relay_keep_alive_sender.rs"]
pub mod relay_keep_alive_sender;

#[path = "relay_updater.rs"]
pub mod relay_updater;

#[path = "discovery.rs"]
pub mod discovery;

#[path = "relay_client_unit.rs"]
pub mod relay_client_unit;

#[path = "relay_states.rs"]
pub mod relay_states;

#[path = "clear_state_cache.rs"]
pub mod clear_state_cache;

#[path = "exclude_self_from_ddb.rs"]
pub mod exclude_self_from_ddb;

#[path = "lookup_root_id.rs"]
pub mod lookup_root_id;

#[path = "list_all_relays_one_root.rs"]
pub mod list_all_relays_one_root;

#[path = "list_root_relays.rs"]
pub mod list_root_relays;

#[path = "unavailable_relays.rs"]
pub mod unavailable_relays;
