// Single top-level integration test crate that groups all tests under tests/messages/* as submodules

// Each module points to the existing test file path so Cargo can compile them as part of this one crate.
// This resolves IDE "Module declaration missing" warnings and keeps tests organized under a single binary.

#[path = "messages/marshal_unit.rs"]
pub mod marshal_unit;

#[path = "messages/router_from_id.rs"]
pub mod router_from_id;

#[path = "messages/router_route_with_network_background.rs"]
pub mod router_route_with_network_background;

#[path = "messages/outbound_response_race.rs"]
pub mod outbound_response_race;

#[path = "messages/triangle_response_routing.rs"]
pub mod triangle_response_routing;

#[path = "messages/marshal_triangle_response.rs"]
pub mod marshal_triangle_response;

#[path = "messages/on_plain_text_delegate.rs"]
pub mod on_plain_text_delegate;

#[path = "messages/on_plain_text_reverse_lookup.rs"]
pub mod on_plain_text_reverse_lookup;

#[path = "messages/relay_ping_handler_unit.rs"]
pub mod relay_ping_handler_unit;

#[path = "messages/relay_ping_handlers.rs"]
pub mod relay_ping_handlers;

#[path = "messages/marshal_ping.rs"]
pub mod marshal_ping;

#[path = "messages/mutex_messages_json.rs"]
pub mod mutex_messages_json;

#[path = "messages/ping_routing.rs"]
pub mod ping_routing;

#[path = "messages/triangle_test1_response_sets_state.rs"]
pub mod triangle_test1_response_sets_state;

#[path = "messages/marshal_relay_call.rs"]
pub mod marshal_relay_call;

#[path = "messages/triangle_test3_registers.rs"]
pub mod triangle_test3_registers;

#[path = "messages/relay_listen.rs"]
pub mod relay_listen;

#[path = "messages/relay_keep_alive.rs"]
pub mod relay_keep_alive;

#[path = "messages/relay_call.rs"]
pub mod relay_call;

#[path = "messages/relay_called_handler.rs"]
pub mod relay_called_handler;

#[path = "messages/ddb_messages_json.rs"]
pub mod ddb_messages_json;

#[path = "messages/ddb_signon_handler.rs"]
pub mod ddb_signon_handler;

#[path = "messages/listening_notifications.rs"]
pub mod listening_notifications;

#[path = "messages/relay_triangle_test1_ext.rs"]
pub mod relay_triangle_test1_ext;

#[path = "messages/ping_response_handler.rs"]
pub mod ping_response_handler;

#[path = "messages/ddb_get_relays_status_handler.rs"]
pub mod ddb_get_relays_status_handler;

#[path = "messages/ddb_init_handler.rs"]
pub mod ddb_init_handler;

#[path = "messages/marshalling_and_routing.rs"]
pub mod marshalling_and_routing;

#[path = "messages/ddb_delete_handler.rs"]
pub mod ddb_delete_handler;

// Some repositories include an additional test file for TriangleTest3 state setting.
// If present in this repo, include it; otherwise harmless if missing when not referenced.
// Note: We cannot conditionally include based on file existence in Rust at build time.
// If this file does not exist in your checkout, comment out the following line.
#[path = "messages/triangle_test3_sets_state.rs"]
pub mod triangle_test3_sets_state;

#[path = "messages/triangle_test1_response_no_corner_node.rs"]
pub mod triangle_test1_response_no_corner_node;

#[path = "messages/report_fail_messages_json.rs"]
pub mod report_fail_messages_json;

#[path = "messages/relay_report_failed_handler.rs"]
pub mod relay_report_failed_handler;

#[path = "messages/report_failed_ripple_handler.rs"]
pub mod report_failed_ripple_handler;

#[path = "messages/only_from_relay_test.rs"]
pub mod only_from_relay_test;

#[path = "messages/triangle_test1_handler_delay.rs"]
pub mod triangle_test1_handler_delay;

