use bingle_core::messages::router::only_from_relay;
use bingle_core::messages::types::*;

#[test]
pub fn test_only_from_relay_cases() {
    // Simple non-relay messages
    assert!(!only_from_relay(&Message::PlainText(PlainTextMessage {
        text: "hi".into(),
        app: None,
        r#type: None,
        cipher_suite: None
    })));

    // Relay messages
    assert!(only_from_relay(&Message::Relay(RelayMessage::RelayCalled(
        RelayCalled {
            app: None,
            channel: 1
        }
    ))));
    assert!(only_from_relay(&Message::Relay(
        RelayMessage::TriangleTest2(RelayTriangleTest2 {
            app: None,
            checking_id: "id".into(),
            checking_endpoint: "1.2.3.4:5555".parse().unwrap()
        })
    )));
    assert!(only_from_relay(&Message::Relay(
        RelayMessage::TriangleTest3(RelayTriangleTest3 { app: None })
    )));

    assert!(!only_from_relay(&Message::Relay(RelayMessage::Call(
        RelayCall {
            app: None,
            called_id: "id".into(),
            tag: None
        }
    ))));

    // DDB messages
    // Signon is always true
    assert!(only_from_relay(&Message::Ddb(DdbMessage::Signon(
        DdbSignon {
            app: "ddb".into(),
            start_id: "id".into(),
            original_signature: None,
            rippled: Some(true),
            tag: None,
            response_tag: None,
            text: None,
            data: None
        }
    ))));
    assert!(only_from_relay(&Message::Ddb(DdbMessage::Signon(
        DdbSignon {
            app: "ddb".into(),
            start_id: "id".into(),
            original_signature: None,
            rippled: Some(false),
            tag: None,
            response_tag: None,
            text: None,
            data: None
        }
    ))));
    assert!(only_from_relay(&Message::Ddb(DdbMessage::Signon(
        DdbSignon {
            app: "ddb".into(),
            start_id: "id".into(),
            original_signature: None,
            rippled: None,
            tag: None,
            response_tag: None,
            text: None,
            data: None
        }
    ))));

    // rippled: true -> true
    assert!(only_from_relay(&Message::Ddb(DdbMessage::UpsertResolve(
        DdbUpsertResolve {
            app: "ddb".into(),
            start_id: "id".into(),
            epoch: 1,
            record: AdvertRecord::new_unsigned(
                "id".into(),
                None,
                None,
                None,
                None,
                "2024-01-01".into()
            ),
            original_signature: "sig".into(),
            rippled: true,
            tag: None,
            response_tag: None,
            text: None,
            data: None
        }
    ))));
    // rippled: false -> false
    assert!(!only_from_relay(&Message::Ddb(DdbMessage::UpsertResolve(
        DdbUpsertResolve {
            app: "ddb".into(),
            start_id: "id".into(),
            epoch: 1,
            record: AdvertRecord::new_unsigned(
                "id".into(),
                None,
                None,
                None,
                None,
                "2024-01-01".into()
            ),
            original_signature: "sig".into(),
            rippled: false,
            tag: None,
            response_tag: None,
            text: None,
            data: None
        }
    ))));

    assert!(only_from_relay(&Message::Ddb(DdbMessage::InitResolve(
        DdbInitResolve {
            app: "ddb".into(),
            tag: None,
            response_tag: None,
            text: None,
            data: None
        }
    ))));

    // DDB Responses
    assert!(only_from_relay(&Message::Ddb(DdbMessage::UpdateResponse(
        DdbUpdateResponse {
            app: "ddb".into(),
            response_tag: None,
            text: None,
            data: None
        }
    ))));

    // Mutex
    assert!(only_from_relay(&Message::Mutex(MutexMessage::Request(
        MutexRequest {
            app: "mutex".into(),
            lamport_timestamp: 1,
            tag: None,
            known_ids: None
        }
    ))));
    assert!(only_from_relay(&Message::Mutex(MutexMessage::Response(
        MutexResponse {
            app: "mutex".into(),
            response_tag: None,
            known_ids: None
        }
    ))));
    assert!(only_from_relay(&Message::Mutex(MutexMessage::Release(
        MutexRelease {
            app: "mutex".into(),
            tag: None,
            known_ids: None
        }
    ))));

    // DdbDumpResolve
    assert!(only_from_relay(&Message::Ddb(DdbMessage::DumpResolve(
        DdbDumpResolve {
            app: "ddb".into(),
            record: AdvertRecord::new_unsigned(
                "id".into(),
                None,
                None,
                None,
                None,
                "2024-01-01".into()
            ),
            tag: None,
            response_tag: None,
            text: None,
            data: None
        }
    ))));
}
