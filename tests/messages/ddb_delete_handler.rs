use crate::util::reusable_mock_api::{
    InnerBingleApi, InnerBingleApiInternal, MockApiBoth, to_weak_api_both,
};
use crate::util::test_util::init_test_logging;
use rust_comms::api::bingle_api::NetworkEndpoint;
use rust_comms::ddb::{AdvertRecord, DdbBackend, InMemoryDdbBackend};
use rust_comms::messages::handlers::{DefaultPrintingHandler, FromStruct, MessageHandler};
use rust_comms::messages::router::Router;
use rust_comms::messages::types::DdbDeleteResolve;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

struct RippleCaptureApi {
    ripple_count: Arc<AtomicU32>,
}

impl InnerBingleApi for RippleCaptureApi {}
impl InnerBingleApiInternal for RippleCaptureApi {
    fn ripple_message(
        &self,
        _message: serde_json::Value,
        _originator_id: String,
        _ddb_backend: &dyn DdbBackend,
    ) {
        self.ripple_count.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn test_on_ddb_delete_resolve_auth() {
    init_test_logging();

    // Setup DDB with some records
    let mut backend = InMemoryDdbBackend::new();
    let alice_id = "ALICE".to_string();
    let bob_id = "BOB".to_string();

    let alice_record = AdvertRecord::new_unsigned(
        alice_id.clone(),
        None,
        None,
        None,
        None,
        "2023-01-01T00:00:00Z".to_string(),
    );
    let bob_record = AdvertRecord::new_unsigned(
        bob_id.clone(),
        None,
        None,
        None,
        None,
        "2023-01-01T00:00:00Z".to_string(),
    );

    backend.upsert(alice_record);
    backend.upsert(bob_record);

    let backend_arc = Arc::new(Mutex::new(backend));

    // Setup API and Router
    let ripple_count = Arc::new(AtomicU32::new(0));
    let inner_api = Arc::new(RippleCaptureApi {
        ripple_count: ripple_count.clone(),
    });
    let mock_api = MockApiBoth::new_with_both_overrides(inner_api.clone(), inner_api.clone());
    let api_arc = to_weak_api_both(mock_api);
    let api_strong = api_arc.upgrade().expect("MockApiBoth upgrade failed");

    let router = Arc::new(Router::new(api_arc.clone()));
    router.set_am_relay(true);
    router.set_ddb_backend(Some(backend_arc.clone()));

    let handler = DefaultPrintingHandler;
    let endpoint = NetworkEndpoint::new_direct("127.0.0.1:1234".parse().unwrap());

    // 1. Alice deletes her own record (NOT rippled)
    {
        let msg = DdbDeleteResolve {
            app: "ddb".to_string(),
            start_id: alice_id.clone(),
            epoch: 0,
            original_signature: "".to_string(),
            rippled: false,
            tag: Some("tag1".to_string()),
            response_tag: None,
            text: None,
            data: None,
        };

        let from = FromStruct::new("ALICE.".to_string(), endpoint.clone(), router.clone());
        handler.on_ddb_delete_resolve(api_strong.clone(), &from, &msg);

        // Should be deleted
        {
            let lock = backend_arc.lock().unwrap();
            assert!(
                lock.lookup(&alice_id).is_none(),
                "Alice's record should be deleted"
            );
            assert!(lock.lookup(&bob_id).is_some());
        }

        // Should have a response
        assert!(
            !from.take_responses().is_empty(),
            "Should have sent a response to Alice"
        );

        // Should have rippled
        assert_eq!(
            ripple_count.load(Ordering::SeqCst),
            1,
            "Should have rippled Alice's delete"
        );
    }

    // 2. Bob tries to delete Alice's record (NOT rippled)
    {
        // Re-insert Alice
        let alice_record = AdvertRecord::new_unsigned(
            alice_id.clone(),
            None,
            None,
            None,
            None,
            "2023-01-01T00:00:00Z".to_string(),
        );
        backend_arc.lock().unwrap().upsert(alice_record);
        ripple_count.store(0, Ordering::SeqCst);

        let msg = DdbDeleteResolve {
            app: "ddb".to_string(),
            start_id: alice_id.clone(),
            epoch: 0,
            original_signature: "".to_string(),
            rippled: false,
            tag: Some("tag2".to_string()),
            response_tag: None,
            text: None,
            data: None,
        };

        let from = FromStruct::new("BOB.".to_string(), endpoint.clone(), router.clone());
        handler.on_ddb_delete_resolve(api_strong.clone(), &from, &msg);

        // Should NOT be deleted
        {
            let lock = backend_arc.lock().unwrap();
            assert!(
                lock.lookup(&alice_id).is_some(),
                "Alice's record should NOT have been deleted by Bob"
            );
        }

        // Should not have sent a response (current implementation)
        assert!(
            from.take_responses().is_empty(),
            "Should not have sent a response for unauthorized delete"
        );

        // Should NOT have rippled
        assert_eq!(
            ripple_count.load(Ordering::SeqCst),
            0,
            "Should NOT have rippled unauthorized delete"
        );
    }

    // 3. Rippled delete for Bob's record (from Charlie)
    {
        ripple_count.store(0, Ordering::SeqCst);
        let msg = DdbDeleteResolve {
            app: "ddb".to_string(),
            start_id: bob_id.clone(),
            epoch: 0,
            original_signature: "".to_string(),
            rippled: true,
            tag: Some("tag3".to_string()),
            response_tag: None,
            text: None,
            data: None,
        };

        let from = FromStruct::new("CHARLIE.".to_string(), endpoint.clone(), router.clone());
        handler.on_ddb_delete_resolve(api_strong.clone(), &from, &msg);

        // Should be deleted (rippled messages are trusted)
        {
            let lock = backend_arc.lock().unwrap();
            assert!(
                lock.lookup(&bob_id).is_none(),
                "Bob's record should be deleted by rippled message"
            );
        }

        // Should have a response (confirming it's gone)
        assert!(!from.take_responses().is_empty());

        // Should NOT ripple a rippled message
        assert_eq!(
            ripple_count.load(Ordering::SeqCst),
            0,
            "Should NOT ripple an already rippled message"
        );
    }
}
