// Sample messages for every variant of the Message enum.
// Used in tests to ensure all message types are covered in routing and filtering scenarios.

use bingle_core::ddb::{AdvertRecord, InetSocketAddress};
use bingle_core::engine::RelayState;
use bingle_core::messages::types::{
    DdbDeleteResolve, DdbDumpResolve, DdbGetRelaysStatus, DdbInitResolve, DdbInitResponse,
    DdbMessage, DdbQueryResolve, DdbQueryResponse, DdbRelaysStatusResponse, DdbSignoff, DdbSignon,
    DdbSignonResponse, DdbUpdateResponse, DdbUpsertResolve, FailVote, Message, MutexMessage,
    MutexRelease, MutexRequest, MutexResponse, PingMessage, PingPing, PingResponse,
    PlainTextMessage, RelayCall, RelayCalled, RelayCheck, RelayCheckResponse, RelayKeepAlive,
    RelayListen, RelayListenResponse, RelayMessage, RelayReportFailed, RelayResponse,
    RelayTriangleTest1, RelayTriangleTest1Response, RelayTriangleTest2, RelayTriangleTest3,
    ReportFailMessage, ReportFailedComplete, ReportFailedRipple, ReportFailedRippleResponse,
};

fn sample_advert_record() -> AdvertRecord {
    AdvertRecord::new_unsigned(
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5HFKQ".to_string(),
        None,
        None,
        None,
        None,
        "2024-01-01T00:00:00Z".to_string(),
    )
}

fn sample_inet_addr() -> InetSocketAddress {
    InetSocketAddress {
        host: "127.0.0.1".to_string(),
        port: 4433,
    }
}

fn sample_fail_vote() -> FailVote {
    FailVote {
        confirming_id: "confirmer1".to_string(),
        signature: "sig1".to_string(),
    }
}

/// One sample message for every variant of the top-level Message enum, plus every
/// sub-variant of Relay, Ddb, Ping, Mutex, and ReportFail.
pub fn all_message_samples() -> Vec<(&'static str, Message)> {
    vec![
        // PlainText
        (
            "PlainText",
            Message::PlainText(PlainTextMessage {
                text: "hello".to_string(),
                app: None,
                r#type: None,
                cipher_suite: None,
            }),
        ),
        // Relay::Call
        (
            "Relay::Call",
            Message::Relay(RelayMessage::Call(RelayCall {
                app: None,
                called_id: "callee1".to_string(),
                tag: None,
            })),
        ),
        // Relay::RelayResponse
        (
            "Relay::RelayResponse",
            Message::Relay(RelayMessage::RelayResponse(RelayResponse {
                app: None,
                channel: Some(1),
                response_tag: None,
            })),
        ),
        // Relay::TriangleTest1
        (
            "Relay::TriangleTest1",
            Message::Relay(RelayMessage::TriangleTest1(RelayTriangleTest1 {
                app: None,
                checking_endpoint: sample_inet_addr(),
                do_not_use_endpoints: vec![],
                tag: None,
            })),
        ),
        // Relay::TriangleTest2
        (
            "Relay::TriangleTest2",
            Message::Relay(RelayMessage::TriangleTest2(RelayTriangleTest2 {
                app: None,
                checking_id: "checker1".to_string(),
                checking_endpoint: sample_inet_addr(),
            })),
        ),
        // Relay::TriangleTest3
        (
            "Relay::TriangleTest3",
            Message::Relay(RelayMessage::TriangleTest3(RelayTriangleTest3 {
                app: None,
            })),
        ),
        // Relay::TriangleTest1Response
        (
            "Relay::TriangleTest1Response",
            Message::Relay(RelayMessage::TriangleTest1Response(
                RelayTriangleTest1Response {
                    app: None,
                    no_corner_node: false,
                    response_tag: None,
                },
            )),
        ),
        // Relay::Listen
        (
            "Relay::Listen",
            Message::Relay(RelayMessage::Listen(RelayListen {
                app: None,
                tag: None,
            })),
        ),
        // Relay::Check
        (
            "Relay::Check",
            Message::Relay(RelayMessage::Check(RelayCheck {
                app: None,
                tag: None,
            })),
        ),
        // Relay::ListenResponse
        (
            "Relay::ListenResponse",
            Message::Relay(RelayMessage::ListenResponse(RelayListenResponse {
                app: None,
                response_tag: None,
            })),
        ),
        // Relay::CheckResponse
        (
            "Relay::CheckResponse",
            Message::Relay(RelayMessage::CheckResponse(RelayCheckResponse {
                app: None,
                relay_state: "available".to_string(),
                response_tag: None,
            })),
        ),
        // Relay::KeepAlive
        (
            "Relay::KeepAlive",
            Message::Relay(RelayMessage::KeepAlive(RelayKeepAlive { app: None })),
        ),
        // Relay::RelayCalled
        (
            "Relay::RelayCalled",
            Message::Relay(RelayMessage::RelayCalled(RelayCalled {
                app: None,
                channel: 3,
            })),
        ),
        // Ddb::UpsertResolve
        (
            "Ddb::UpsertResolve",
            Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve {
                app: "ddb".to_string(),
                start_id: "start1".to_string(),
                epoch: 1,
                record: sample_advert_record(),
                original_signature: "origsig".to_string(),
                rippled: false,
                tag: None,
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::DeleteResolve
        (
            "Ddb::DeleteResolve",
            Message::Ddb(DdbMessage::DeleteResolve(DdbDeleteResolve {
                app: "ddb".to_string(),
                start_id: "start1".to_string(),
                epoch: 1,
                original_signature: "origsig".to_string(),
                rippled: false,
                tag: None,
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::Signoff
        (
            "Ddb::Signoff",
            Message::Ddb(DdbMessage::Signoff(DdbSignoff {
                app: "ddb".to_string(),
                start_id: "start1".to_string(),
                rippled: false,
                tag: None,
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::QueryResolve
        (
            "Ddb::QueryResolve",
            Message::Ddb(DdbMessage::QueryResolve(DdbQueryResolve {
                app: "ddb".to_string(),
                id: "node1".to_string(),
                tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::QueryResponse
        (
            "Ddb::QueryResponse",
            Message::Ddb(DdbMessage::QueryResponse(DdbQueryResponse {
                app: "ddb".to_string(),
                found: false,
                advert: None,
                tag: None,
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::UpdateResponse
        (
            "Ddb::UpdateResponse",
            Message::Ddb(DdbMessage::UpdateResponse(DdbUpdateResponse {
                app: "ddb".to_string(),
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::Signon
        (
            "Ddb::Signon",
            Message::Ddb(DdbMessage::Signon(DdbSignon {
                app: "ddb".to_string(),
                start_id: "start1".to_string(),
                original_signature: None,
                rippled: None,
                tag: None,
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::SignonResponse
        (
            "Ddb::SignonResponse",
            Message::Ddb(DdbMessage::SignonResponse(DdbSignonResponse {
                app: "ddb".to_string(),
                tag: None,
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::GetRelaysStatus
        (
            "Ddb::GetRelaysStatus",
            Message::Ddb(DdbMessage::GetRelaysStatus(DdbGetRelaysStatus {
                app: "ddb".to_string(),
                epoch_id: 42,
                tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::RelaysStatusResponse
        (
            "Ddb::RelaysStatusResponse",
            Message::Ddb(DdbMessage::RelaysStatusResponse(DdbRelaysStatusResponse {
                app: "ddb".to_string(),
                responder_state: RelayState::Available,
                epoch_id: 42,
                tree_order: 0,
                relay_ids: vec![],
                relay_endpoints: None,
                relay_states: vec![],
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::InitResolve
        (
            "Ddb::InitResolve",
            Message::Ddb(DdbMessage::InitResolve(DdbInitResolve {
                app: "ddb".to_string(),
                tag: None,
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::InitResponse
        (
            "Ddb::InitResponse",
            Message::Ddb(DdbMessage::InitResponse(DdbInitResponse {
                app: "ddb".to_string(),
                db_count: 0,
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ddb::DumpResolve
        (
            "Ddb::DumpResolve",
            Message::Ddb(DdbMessage::DumpResolve(DdbDumpResolve {
                app: "ddb".to_string(),
                record: sample_advert_record(),
                tag: None,
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ping::Ping
        (
            "Ping::Ping",
            Message::Ping(PingMessage::Ping(PingPing {
                app: "ping".to_string(),
                tag: None,
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Ping::Response
        (
            "Ping::Response",
            Message::Ping(PingMessage::Response(PingResponse {
                app: "ping".to_string(),
                verified_id: "verified1".to_string(),
                response_tag: None,
                text: None,
                data: None,
            })),
        ),
        // Mutex::Request
        (
            "Mutex::Request",
            Message::Mutex(MutexMessage::Request(MutexRequest {
                app: "mutex".to_string(),
                lamport_timestamp: 1,
                tag: None,
                known_ids: None,
            })),
        ),
        // Mutex::Response
        (
            "Mutex::Response",
            Message::Mutex(MutexMessage::Response(MutexResponse {
                app: "mutex".to_string(),
                response_tag: None,
                known_ids: None,
            })),
        ),
        // Mutex::Release
        (
            "Mutex::Release",
            Message::Mutex(MutexMessage::Release(MutexRelease {
                app: "mutex".to_string(),
                tag: None,
                known_ids: None,
            })),
        ),
        // ReportFail::RelayReportFailed
        (
            "ReportFail::RelayReportFailed",
            Message::ReportFail(ReportFailMessage::RelayReportFailed(RelayReportFailed {
                app: "reportFail".to_string(),
                tag: None,
                response_tag: None,
                failed_relay_id: "relay1".to_string(),
                fail_type: "timeout".to_string(),
                timestamp: "2024-01-01T00:00:00Z".to_string(),
            })),
        ),
        // ReportFail::ReportFailedRipple
        (
            "ReportFail::ReportFailedRipple",
            Message::ReportFail(ReportFailMessage::ReportFailedRipple(ReportFailedRipple {
                app: "reportFail".to_string(),
                tag: None,
                response_tag: None,
                failed_relay_id: "relay1".to_string(),
                fail_type: "timeout".to_string(),
                timestamp: "2024-01-01T00:00:00Z".to_string(),
                confirmations: vec![sample_fail_vote()],
                disputes: vec![],
            })),
        ),
        // ReportFail::ReportFailedRippleResponse
        (
            "ReportFail::ReportFailedRippleResponse",
            Message::ReportFail(ReportFailMessage::ReportFailedRippleResponse(
                ReportFailedRippleResponse {
                    app: "reportFail".to_string(),
                    tag: None,
                    response_tag: None,
                    failed_relay_id: "relay1".to_string(),
                    fail_type: "timeout".to_string(),
                    timestamp: "2024-01-01T00:00:00Z".to_string(),
                    confirmations: vec![sample_fail_vote()],
                    disputes: vec![],
                },
            )),
        ),
        // ReportFail::ReportFailedComplete
        (
            "ReportFail::ReportFailedComplete",
            Message::ReportFail(ReportFailMessage::ReportFailedComplete(
                ReportFailedComplete {
                    app: "reportFail".to_string(),
                    tag: None,
                    response_tag: None,
                    failed_relay_id: "relay1".to_string(),
                    fail_type: "timeout".to_string(),
                    timestamp: "2024-01-01T00:00:00Z".to_string(),
                    confirmations: vec![sample_fail_vote()],
                    disputes: vec![],
                },
            )),
        ),
        // Unknown (fallback)
        (
            "Unknown",
            Message::Unknown(serde_json::json!({ "someUnknownField": 99 })),
        ),
    ]
}
