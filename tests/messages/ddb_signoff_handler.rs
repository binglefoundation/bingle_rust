use crate::util::reusable_mock_api::{
    InnerBingleApi, InnerBingleApiInternal, MockApiBoth, to_weak_api_both,
};
use crate::util::test_util::init_test_logging;
use rust_comms::api::bingle_api::NetworkEndpoint;
use rust_comms::ddb::{AdvertRecord, DdbBackend, InMemoryDdbBackend};
use rust_comms::messages::handlers::{DefaultPrintingHandler, FromStruct, MessageHandler};
use rust_comms::messages::router::Router;
use rust_comms::messages::types::{DdbSignoff, InetSocketAddress};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Captures ripple and relay-finder-cache-clear calls so tests can assert on them.
struct SignoffCaptureApi {
    ripple_count: Arc<AtomicU32>,
    clear_count: Arc<AtomicU32>,
}

impl InnerBingleApi for SignoffCaptureApi {}
impl InnerBingleApiInternal for SignoffCaptureApi {
    fn ripple_message(
        &self,
        _message: serde_json::Value,
        _originator_id: String,
        _ddb_backend: &dyn DdbBackend,
    ) {
        self.ripple_count.fetch_add(1, Ordering::SeqCst);
    }
    fn relay_finder_clear_state_cache(&self) {
        self.clear_count.fetch_add(1, Ordering::SeqCst);
    }
}

const REGISTERED_ADDR: &str = "127.0.0.1:1234";

fn registered_endpoint() -> InetSocketAddress {
    InetSocketAddress {
        host: "127.0.0.1".to_string(),
        port: 1234,
    }
}

fn make_msg(start_id: &str, rippled: bool) -> DdbSignoff {
    DdbSignoff {
        app: "ddb".to_string(),
        start_id: start_id.to_string(),
        rippled,
        tag: Some("tag".to_string()),
        response_tag: None,
        text: None,
        data: None,
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn test_on_ddb_signoff() {
    init_test_logging();

    let alice_id = "ALICE".to_string();
    let bob_id = "BOB".to_string();

    let ripple_count = Arc::new(AtomicU32::new(0));
    let clear_count = Arc::new(AtomicU32::new(0));
    let inner_api = Arc::new(SignoffCaptureApi {
        ripple_count: ripple_count.clone(),
        clear_count: clear_count.clone(),
    });
    let mock_api = MockApiBoth::new_with_both_overrides(inner_api.clone(), inner_api.clone());
    let api_arc = to_weak_api_both(mock_api);
    let api_strong = api_arc.upgrade().expect("MockApiBoth upgrade failed");

    let router = Arc::new(Router::new(api_arc.clone()));
    router.set_am_relay(true);

    let handler = DefaultPrintingHandler;
    let registered = NetworkEndpoint::new_direct(REGISTERED_ADDR.parse().unwrap());

    // Fresh backend per case keeps the assertions independent.
    let fresh_backend = || {
        let mut backend = InMemoryDdbBackend::new();
        backend.upsert(AdvertRecord::new_unsigned(
            alice_id.clone(),
            Some(registered_endpoint()),
            Some(true),
            None,
            None,
            "2023-01-01T00:00:00Z".to_string(),
        ));
        backend.upsert(AdvertRecord::new_unsigned(
            bob_id.clone(),
            Some(registered_endpoint()),
            Some(true),
            None,
            None,
            "2023-01-01T00:00:00Z".to_string(),
        ));
        Arc::new(Mutex::new(backend))
    };

    // 1. Alice signs off from her registered endpoint (not rippled)
    {
        let backend = fresh_backend();
        router.set_ddb_backend(Some(backend.clone()));
        ripple_count.store(0, Ordering::SeqCst);
        clear_count.store(0, Ordering::SeqCst);

        let from = FromStruct::new("ALICE.".to_string(), registered.clone(), router.clone());
        handler.on_ddb_signoff(api_strong.clone(), &from, &make_msg(&alice_id, false));

        assert!(
            backend.lock().unwrap().lookup(&alice_id).is_none(),
            "Alice's record should be deleted"
        );
        assert!(
            from.take_responses().is_empty(),
            "signoff should not push a response"
        );
        assert_eq!(
            clear_count.load(Ordering::SeqCst),
            1,
            "relay pool state cache should be cleared"
        );
        assert_eq!(
            ripple_count.load(Ordering::SeqCst),
            1,
            "signoff should ripple"
        );
    }

    // 2. Bob tries to sign off Alice's record (start_id != sender)
    {
        let backend = fresh_backend();
        router.set_ddb_backend(Some(backend.clone()));
        ripple_count.store(0, Ordering::SeqCst);
        clear_count.store(0, Ordering::SeqCst);

        let from = FromStruct::new("BOB.".to_string(), registered.clone(), router.clone());
        handler.on_ddb_signoff(api_strong.clone(), &from, &make_msg(&alice_id, false));

        assert!(
            backend.lock().unwrap().lookup(&alice_id).is_some(),
            "Alice's record must not be removed by Bob"
        );
        assert_eq!(
            clear_count.load(Ordering::SeqCst),
            0,
            "no cache clear on unauthorized signoff"
        );
        assert_eq!(
            ripple_count.load(Ordering::SeqCst),
            0,
            "no ripple on unauthorized signoff"
        );
    }

    // 3. Alice signs off but from a different endpoint than registered
    {
        let backend = fresh_backend();
        router.set_ddb_backend(Some(backend.clone()));
        ripple_count.store(0, Ordering::SeqCst);
        clear_count.store(0, Ordering::SeqCst);

        let wrong = NetworkEndpoint::new_direct("127.0.0.1:9999".parse().unwrap());
        let from = FromStruct::new("ALICE.".to_string(), wrong, router.clone());
        handler.on_ddb_signoff(api_strong.clone(), &from, &make_msg(&alice_id, false));

        assert!(
            backend.lock().unwrap().lookup(&alice_id).is_some(),
            "signoff from wrong endpoint must be rejected"
        );
        assert_eq!(
            clear_count.load(Ordering::SeqCst),
            0,
            "no cache clear on endpoint mismatch"
        );
        assert_eq!(
            ripple_count.load(Ordering::SeqCst),
            0,
            "no ripple on endpoint mismatch"
        );
    }

    // 4. Rippled signoff for Bob (trusted, from a peer relay; not re-rippled)
    {
        let backend = fresh_backend();
        router.set_ddb_backend(Some(backend.clone()));
        ripple_count.store(0, Ordering::SeqCst);
        clear_count.store(0, Ordering::SeqCst);

        let from = FromStruct::new("CHARLIE.".to_string(), registered.clone(), router.clone());
        handler.on_ddb_signoff(api_strong.clone(), &from, &make_msg(&bob_id, true));

        assert!(
            backend.lock().unwrap().lookup(&bob_id).is_none(),
            "rippled signoff should delete Bob's record"
        );
        assert_eq!(
            clear_count.load(Ordering::SeqCst),
            1,
            "rippled signoff should also clear the pool cache"
        );
        assert_eq!(
            ripple_count.load(Ordering::SeqCst),
            0,
            "an already-rippled signoff must not ripple again"
        );
    }

    // 5. Not a relay: signoff is ignored entirely
    {
        let backend = fresh_backend();
        router.set_ddb_backend(Some(backend.clone()));
        router.set_am_relay(false);
        ripple_count.store(0, Ordering::SeqCst);
        clear_count.store(0, Ordering::SeqCst);

        let from = FromStruct::new("ALICE.".to_string(), registered.clone(), router.clone());
        handler.on_ddb_signoff(api_strong.clone(), &from, &make_msg(&alice_id, false));

        assert!(
            backend.lock().unwrap().lookup(&alice_id).is_some(),
            "non-relay must not process signoff"
        );
        assert_eq!(clear_count.load(Ordering::SeqCst), 0);
        assert_eq!(ripple_count.load(Ordering::SeqCst), 0);
        router.set_am_relay(true);
    }
}
