// Single top-level integration test crate that groups all tests under tests/messages/* as submodules

// Each module points to the existing test file path so Cargo can compile them as part of this one crate.
// This resolves IDE "Module declaration missing" warnings and keeps tests organized under a single binary.

#[path = "messages/marshal_unit.rs"]
mod marshal_unit;

#[path = "messages/router_from_id.rs"]
mod router_from_id;

#[path = "messages/triangle_response_routing.rs"]
mod triangle_response_routing;

#[path = "messages/marshal_triangle_response.rs"]
mod marshal_triangle_response;

#[path = "messages/on_plain_text_delegate.rs"]
mod on_plain_text_delegate;

#[path = "messages/relay_ping_handler_unit.rs"]
mod relay_ping_handler_unit;

#[path = "messages/relay_ping_handlers.rs"]
mod relay_ping_handlers;

#[path = "messages/marshal_ping.rs"]
mod marshal_ping;

#[path = "messages/mutex_messages_json.rs"]
mod mutex_messages_json;

#[path = "messages/ping_routing.rs"]
mod ping_routing;

#[path = "messages/triangle_test1_response_sets_state.rs"]
mod triangle_test1_response_sets_state;

#[path = "messages/marshal_relay_call.rs"]
mod marshal_relay_call;

#[path = "messages/triangle_test3_registers.rs"]
mod triangle_test3_registers;

#[path = "messages/relay_listen.rs"]
mod relay_listen;

#[path = "messages/relay_call.rs"]
mod relay_call;

#[path = "messages/relay_called_handler.rs"]
mod relay_called_handler;

#[path = "messages/ddb_messages_json.rs"]
mod ddb_messages_json;

#[path = "messages/ddb_signon_handler.rs"]
mod ddb_signon_handler;

#[path = "messages/listening_notifications.rs"]
mod listening_notifications;

// Some repositories include an additional test file for TriangleTest3 state setting.
// If present in this repo, include it; otherwise harmless if missing when not referenced.
// Note: We cannot conditionally include based on file existence in Rust at build time.
// If this file does not exist in your checkout, comment out the following line.
// #[path = "messages/triangle_test3_sets_state.rs"]
// mod triangle_test3_sets_state;
