use bingle_core::messages::marshal::{from_json_str, to_json_string, to_json_value};
use bingle_core::messages::types::{
    FailVote, Message, RelayReportFailed, ReportFailMessage, ReportFailedComplete,
    ReportFailedRipple, ReportFailedRippleResponse,
};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_report_failed_roundtrip() {
    let msg = Message::ReportFail(ReportFailMessage::RelayReportFailed(RelayReportFailed {
        app: "reportFail".into(),
        tag: Some("t1".into()),
        response_tag: None,
        failed_relay_id: "RELAY123".into(),
        fail_type: "send_rejected".into(),
        timestamp: "2026-01-01T00:00:00Z".into(),
    }));
    let json = to_json_string(&msg);
    let back = from_json_str(&json).expect("parse back relay_report_failed");
    assert_eq!(msg, back);

    let v = to_json_value(&msg);
    assert_eq!(v.get("app").and_then(|x| x.as_str()), Some("reportFail"));
    assert_eq!(
        v.get("type").and_then(|x| x.as_str()),
        Some("relayReportFailed")
    );
    assert_eq!(
        v.get("failed_relay_id").and_then(|x| x.as_str()),
        Some("RELAY123")
    );
    assert_eq!(
        v.get("fail_type").and_then(|x| x.as_str()),
        Some("send_rejected")
    );
    assert_eq!(
        v.get("timestamp").and_then(|x| x.as_str()),
        Some("2026-01-01T00:00:00Z")
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn report_failed_ripple_roundtrip() {
    let votes = vec![
        FailVote {
            confirming_id: "NODE_A".into(),
            signature: "SIG_A".into(),
        },
        FailVote {
            confirming_id: "NODE_B".into(),
            signature: "SIG_B".into(),
        },
    ];
    let msg = Message::ReportFail(ReportFailMessage::ReportFailedRipple(ReportFailedRipple {
        app: "reportFail".into(),
        tag: None,
        response_tag: Some("corr1".into()),
        failed_relay_id: "RELAY456".into(),
        fail_type: "send_rejected".into(),
        timestamp: "2026-02-01T00:00:00Z".into(),
        confirmations: votes.clone(),
        disputes: vec![],
    }));
    let json = to_json_string(&msg);
    let back = from_json_str(&json).expect("parse back report_failed_ripple");
    assert_eq!(msg, back);

    let v = to_json_value(&msg);
    assert_eq!(
        v.get("type").and_then(|x| x.as_str()),
        Some("reportFailedRipple")
    );
    let confirmations = v
        .get("confirmations")
        .and_then(|x| x.as_array())
        .expect("confirmations array");
    assert_eq!(confirmations.len(), 2);
    let disputes = v
        .get("disputes")
        .and_then(|x| x.as_array())
        .expect("disputes array");
    assert_eq!(disputes.len(), 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn report_failed_ripple_response_roundtrip() {
    let msg = Message::ReportFail(ReportFailMessage::ReportFailedRippleResponse(
        ReportFailedRippleResponse {
            app: "reportFail".into(),
            tag: None,
            response_tag: None,
            failed_relay_id: "RELAY789".into(),
            fail_type: "timeout".into(),
            timestamp: "2026-03-01T00:00:00Z".into(),
            confirmations: vec![FailVote {
                confirming_id: "NODE_C".into(),
                signature: "SIG_C".into(),
            }],
            disputes: vec![FailVote {
                confirming_id: "NODE_D".into(),
                signature: "SIG_D".into(),
            }],
        },
    ));
    let json = to_json_string(&msg);
    let back = from_json_str(&json).expect("parse back report_failed_ripple_response");
    assert_eq!(msg, back);

    let v = to_json_value(&msg);
    assert_eq!(
        v.get("type").and_then(|x| x.as_str()),
        Some("reportFailedRippleResponse")
    );
    assert_eq!(v.get("app").and_then(|x| x.as_str()), Some("reportFail"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn report_failed_complete_roundtrip() {
    let msg = Message::ReportFail(ReportFailMessage::ReportFailedComplete(
        ReportFailedComplete {
            app: "reportFail".into(),
            tag: None,
            response_tag: None,
            failed_relay_id: "RELAY999".into(),
            fail_type: "send_rejected".into(),
            timestamp: "2026-04-01T00:00:00Z".into(),
            confirmations: vec![],
            disputes: vec![],
        },
    ));
    let json = to_json_string(&msg);
    let back = from_json_str(&json).expect("parse back report_failed_complete");
    assert_eq!(msg, back);

    let v = to_json_value(&msg);
    assert_eq!(
        v.get("type").and_then(|x| x.as_str()),
        Some("reportFailedComplete")
    );
    assert_eq!(v.get("app").and_then(|x| x.as_str()), Some("reportFail"));
    assert_eq!(
        v.get("failed_relay_id").and_then(|x| x.as_str()),
        Some("RELAY999")
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn fail_vote_fields_serialize_correctly() {
    let vote = FailVote {
        confirming_id: "NODE_X".into(),
        signature: "SIG_X".into(),
    };
    let v = serde_json::to_value(&vote).expect("serialize FailVote");
    assert_eq!(
        v.get("confirming_id").and_then(|x| x.as_str()),
        Some("NODE_X")
    );
    assert_eq!(v.get("signature").and_then(|x| x.as_str()), Some("SIG_X"));
}
