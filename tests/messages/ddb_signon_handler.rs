use std::sync::{Arc, Mutex};
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use rust_comms::messages::handlers::{DefaultPrintingHandler, FromStruct, MessageHandler};
use rust_comms::messages::types::{DdbMessage, DdbSignon, Message, AdvertRecord, InetSocketAddress};
use rust_comms::api::bingle_api::{NetworkEndpoint};
use crate::util::reusable_mock_api::{MockApiBoth, to_weak_api_both, InnerBingleApiInternal};

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_on_ddb_signon_updates_backend_and_sends_response() {
    let handler = DefaultPrintingHandler;
    
    let upserts = Arc::new(Mutex::new(Vec::new()));
    struct TrackUpsertInternal {
        upserts: Arc<Mutex<Vec<AdvertRecord>>>,
    }
    impl InnerBingleApiInternal for TrackUpsertInternal {
        fn ddb_upsert_record(&self, record: AdvertRecord) {
            self.upserts.lock().unwrap().push(record);
        }
    }
    
    let internal = Arc::new(TrackUpsertInternal { upserts: upserts.clone() });
    let api_weak = to_weak_api_both(MockApiBoth::new_with_internal_override(internal));
    let api = api_weak.upgrade().expect("upgrade");
    let router = Arc::new(rust_comms::messages::router::Router::new(api_weak.clone()));
    
    let sender_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234);
    let from = FromStruct {
        id: "NEWNODE".to_string() + rust_comms::protocol::ISSUER_SUFFIX,
        network_source_key: NetworkEndpoint::new_direct(sender_addr),
        router: router.clone(),
    };
    
    let signon = DdbSignon {
        app: "ddb".to_string(),
        start_id: "NEWNODE".to_string(),
        original_signature: Some("sig-123".to_string()),
        rippled: None,
        tag: Some("my-tag".to_string()),
        response_tag: None,
        text: None,
        data: None,
    };
    
    router.set_am_relay(true);
    
    // Set last response tag in router manually
    router.set_last_response_tag(Some("my-tag".to_string()));
    
    handler.on_ddb_signon(api.clone(), &from, &signon);
    
    // Check upsert
    {
        let upserted = upserts.lock().unwrap();
        assert_eq!(upserted.len(), 1, "Should have exactly one upserted record");
        let rec = &upserted[0];
        assert_eq!(rec.id, "NEWNODE");
        assert_eq!(rec.am_relay, Some(true));
        assert_eq!(rec.sig, Some("sig-123".to_string()));
        assert_eq!(rec.endpoint, Some(InetSocketAddress { host: "1.2.3.4".to_string(), port: 1234 }));
    }

    let resp_json = router.take_outbound_response().expect("should have outbound response");
    let resp_msg = rust_comms::messages::marshal::from_json_value(resp_json).expect("valid message");
    
    if let Message::Ddb(DdbMessage::SignonResponse(resp)) = resp_msg {
        assert_eq!(resp.app, "ddb");
        assert_eq!(resp.response_tag, Some("my-tag".to_string()));
    } else {
        panic!("Expected SignonResponse, got {:?}", resp_msg);
    }
}
