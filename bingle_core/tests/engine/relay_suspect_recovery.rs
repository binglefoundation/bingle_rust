// Issue #516: a peer relay reported failed stays in the relay set as suspect, and stops being
// suspect when it is heard from again, without a restart.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bingle_core::api::bingle_api::{NetworkEndpoint, StartOptions};
use bingle_core::ddb::{AdvertRecord, InetSocketAddress};
use bingle_core::dtls::{Dtls, HandleMessage};
use bingle_core::engine::Engine;
use bingle_core::messages::router::Router;

use crate::engine::sender_auth::MinimalFakeDtls;
use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth};
use crate::util::test_util::{self, init_test_logging};

const PEER_RELAY: &str = test_util::ADDRESS_RECEIVE;

/// Authenticates every sender, and has no chain configured, so a relay's on-chain permission
/// reads as unknown.
struct AuthAll;

impl InnerBingleApi for AuthAll {
    fn handle_lookup_by_id(
        &self,
        _user_id: &bingle_core::api::bingle_api::UserId,
    ) -> Option<bingle_core::api::bingle_api::Handle> {
        Some("peer_relay".to_string())
    }
}

fn peer_relay_record() -> AdvertRecord {
    let addr: SocketAddr = "127.0.0.1:12121".parse().expect("valid addr");
    AdvertRecord::new_unsigned(
        PEER_RELAY.to_string(),
        Some(InetSocketAddress::from(addr)),
        Some(true),
        None,
        None,
        "1970-01-01T00:00:00Z".to_string(),
    )
}

fn relay_engine() -> (Engine, HandleMessage, Arc<Router>) {
    let api = crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(
        Arc::new(AuthAll),
    ));
    let mut options = StartOptions::new("".into());
    options.am_relay = true;
    let engine = Engine::new_with_dtls(&options, api.clone(), Box::new(MinimalFakeDtls::new()));
    // Keep the re-check loop idle: these tests drive recovery by hand.
    engine.test_set_relay_suspect_check_interval(Duration::from_secs(3600));
    let router = Arc::new(Router::new(api));
    engine.set_router(router.clone());
    engine
        .install_dtls_handler_for_tests()
        .expect("install_dtls_handler_for_tests failed");
    let mut handler: Option<HandleMessage> = None;
    engine.with_dtls(|dtls| handler = dtls.get_handle_message());
    (engine, handler.expect("DTLS handler installed"), router)
}

/// Deliver an authenticated message from `issuer`, as the DTLS layer would.
fn message_from(handler: &HandleMessage, router: Arc<Router>, issuer: &str) {
    let server = MinimalFakeDtls::new();
    let from_ep = NetworkEndpoint::new_direct("127.0.0.1:12121".parse::<SocketAddr>().unwrap());
    let ping =
        serde_json::to_vec(&serde_json::json!({"app": "ping", "type": "ping"})).expect("serialize");
    Router::with_current_router(router, || {
        handler(&server as &dyn Dtls, &from_ep, issuer, &ping);
    });
}

/// The acceptance case: after a report, the relay keeps its place in the relay set (so ripples
/// still go to it), and a message from it ends the suspicion.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn reported_relay_is_kept_and_recovers_when_heard_from() {
    init_test_logging();
    let (engine, handler, router) = relay_engine();
    engine.ddb_backend_upsert_for_tests(peer_relay_record());

    assert!(engine.relay_mark_suspect(PEER_RELAY));
    assert!(engine.relay_suspects().is_suspect(PEER_RELAY));
    let kept = engine
        .ddb_backend_lookup_for_tests(PEER_RELAY)
        .expect("a reported relay stays in the DDB");
    assert_eq!(kept.am_relay, Some(true), "still in the relay set");

    assert!(
        !engine.relay_mark_suspect(PEER_RELAY),
        "a second report finds it already suspect"
    );

    let issuer = format!("{}{}", PEER_RELAY, bingle_core::protocol::ISSUER_SUFFIX);
    message_from(&handler, router, &issuer);
    assert!(
        !engine.relay_suspects().is_suspect(PEER_RELAY),
        "hearing from the relay ends the suspicion"
    );
    assert!(engine.ddb_backend_lookup_for_tests(PEER_RELAY).is_some());

    assert!(
        engine.relay_mark_suspect(PEER_RELAY),
        "a later report marks it suspect afresh"
    );
}

/// A message from a relay that is not suspect changes nothing.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn message_from_a_relay_that_is_not_suspect_changes_nothing() {
    init_test_logging();
    let (engine, handler, router) = relay_engine();
    let issuer = format!("{}{}", PEER_RELAY, bingle_core::protocol::ISSUER_SUFFIX);
    message_from(&handler, router, &issuer);
    assert!(engine.relay_suspects().is_empty());
}
