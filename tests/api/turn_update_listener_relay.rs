use rust_comms::api::bingle_api::{BingleApiInternal, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[test]
fn update_turn_listener_registers_mappings() {
    // Build API instance (no need to start engine/mux for this mapping update)
    let api = BingleApiImpl::new(&StartOptions::default());

    // Prepare a relay id and address
    let relay_id = "TESTRELAYID".to_string();
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 54321);

    // Call the internal method to update TURN listener relay mappings
    let res = <BingleApiImpl as BingleApiInternal>::update_turn_listener_relay(&api, relay_id.clone(), relay_addr);
    assert!(res.is_ok(), "update_turn_listener_relay should return Ok");

    // Validate the handler now resolves id <-> addr
    let th = api.engine_turn_handler_for_tests();
    let got_addr = th.lookup_addr_by_id(&relay_id);
    assert_eq!(got_addr, Some(relay_addr), "id should map to address");

    let got_id = th.lookup_id_by_addr(&relay_addr);
    assert_eq!(got_id.as_deref(), Some(relay_id.as_str()), "address should map back to id");
}
