use bingle_core::api::bingle_api::{BingleApiInternal, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn turn_client_handle_listen_response_registers_client_mapping() {
    // Build API instance (no need to start engine/mux for this mapping update)
    let api = BingleApiImpl::new(&StartOptions::new("".into()));

    // Prepare a relay id and address
    let relay_id = "TESTRELAYID".to_string();
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 54321);

    // Call the internal method to register the ListenResponse mapping on the client
    <BingleApiImpl as BingleApiInternal>::turn_client_handle_listen_response(
        &api,
        relay_addr,
        relay_id.clone(),
    );

    // Validate the client handler now resolves id <-> addr
    let th = api.engine_turn_client_handler_for_tests();
    let got_addr = th.lookup_addr_by_id(&relay_id);
    assert_eq!(got_addr, Some(relay_addr), "id should map to address");

    let got_id = th.lookup_id_by_addr(&relay_addr);
    assert_eq!(
        got_id.as_deref(),
        Some(relay_id.as_str()),
        "address should map back to id"
    );
}
