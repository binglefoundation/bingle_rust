use rust_comms::messages::marshal;
use rust_comms::messages::types::*;
use rust_comms::engine::RelayState;

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_upsert_serde_roundtrip() {
    let rec = AdvertRecord { id: "ID".into(), endpoint: None, am_relay: Some(false), relay_id: None, relay_sig: None, date: "2025-01-01T00:00:00Z".into(), sig: None };
    let msg = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve {
        app: "ddb".into(),
        start_id: "START".into(),
        epoch: 1,
        record: rec,
        original_signature: "SIG".into(),
        rippled: true,
        tag: Some("t1".into()),
        text: None,
        data: None,
    }));
    let json = marshal::to_json_string(&msg);
    let back = marshal::from_json_str(&json).unwrap();
    assert_eq!(msg, back);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_query_and_response_roundtrip() {
    let q = Message::Ddb(DdbMessage::QueryResolve(DdbQueryResolve {
        app: "ddb".into(),
        id: "ID123".into(),
        tag: None,
        text: None,
        data: None,
    }));
    let jq = marshal::to_json_string(&q);
    let q2 = marshal::from_json_str(&jq).unwrap();
    assert_eq!(q, q2);

    let resp = Message::Ddb(DdbMessage::QueryResponse(DdbQueryResponse {
        app: "ddb".into(),
        found: true,
        advert: Some(AdvertRecord { id: "ID123".into(), endpoint: None, am_relay: None, relay_id: None, relay_sig: None, date: "2025-01-02T03:04:05Z".into(), sig: None }),
        response_tag: Some("corr".into()),
        text: None,
        data: None,
    }));
    let jr = marshal::to_json_string(&resp);
    let r2 = marshal::from_json_str(&jr).unwrap();
    assert_eq!(resp, r2);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_update_and_delete_roundtrip() {
    let upd = Message::Ddb(DdbMessage::UpdateResponse(DdbUpdateResponse { app: "ddb".into(), response_tag: None, text: None, data: None }));
    let ju = marshal::to_json_string(&upd);
    let u2 = marshal::from_json_str(&ju).unwrap();
    assert_eq!(upd, u2);

    let del = Message::Ddb(DdbMessage::DeleteResolve(DdbDeleteResolve {
        app: "ddb".into(),
        start_id: "START".into(),
        epoch: 42,
        original_signature: "OSIG".into(),
        rippled: false,
        tag: None,
        text: None,
        data: None,
    }));
    let jd = marshal::to_json_string(&del);
    let d2 = marshal::from_json_str(&jd).unwrap();
    assert_eq!(del, d2);
}


#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_signon_and_response_roundtrip() {
    let signon = Message::Ddb(DdbMessage::Signon(DdbSignon {
        app: "ddb".into(),
        start_id: "NEWNODE".into(),
        original_signature: Some("SIG2".into()),
        rippled: Some(true),
        tag: Some("t".into()),
        text: None,
        data: None,
    }));
    let js = marshal::to_json_string(&signon);
    let s2 = marshal::from_json_str(&js).unwrap();
    assert_eq!(signon, s2);

    let signon_resp = Message::Ddb(DdbMessage::SignonResponse(DdbSignonResponse {
        app: "ddb".into(),
        response_tag: None,
        text: None,
        data: None,
    }));
    let jr = marshal::to_json_string(&signon_resp);
    let r2 = marshal::from_json_str(&jr).unwrap();
    assert_eq!(signon_resp, r2);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_get_epoch_and_info_roundtrip() {
    let get = Message::Ddb(DdbMessage::GetRelaysStatus(DdbGetRelaysStatus {
        app: "ddb".into(),
        epoch_id: -1,
        tag: None,
        text: None,
        data: None,
    }));
    let jg = marshal::to_json_string(&get);
    let g2 = marshal::from_json_str(&jg).unwrap();
    assert_eq!(get, g2);

    let info = Message::Ddb(DdbMessage::RelaysStatusResponse(DdbRelaysStatusResponse {
        app: "ddb".into(),
        responder_state: RelayState::Available,
        epoch_id: 7,
        tree_order: 4,
        relay_ids: vec!["RID1".into(), "RID2".into()],
        relay_endpoints: Some(vec![InetSocketAddress { host: "192.168.1.1".into(), port: 3456 }]),
        relay_states: vec![RelayState::Available, RelayState::Starting],
        response_tag: None,
        text: None,
        data: None,
    }));
    let ji = marshal::to_json_string(&info);
    let i2 = marshal::from_json_str(&ji).unwrap();
    assert_eq!(info, i2);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_init_and_dump_roundtrip() {
    let init = Message::Ddb(DdbMessage::InitResolve(DdbInitResolve {
        app: "ddb".into(),
        tag: None,
        text: None,
        data: None,
    }));
    let j1 = marshal::to_json_string(&init);
    let i2 = marshal::from_json_str(&j1).unwrap();
    assert_eq!(init, i2);

    let init_resp = Message::Ddb(DdbMessage::InitResponse(DdbInitResponse {
        app: "ddb".into(),
        db_count: 3,
        response_tag: None,
        text: None,
        data: None,
    }));
    let j2 = marshal::to_json_string(&init_resp);
    let ir2 = marshal::from_json_str(&j2).unwrap();
    assert_eq!(init_resp, ir2);

    let rec = AdvertRecord { id: "ID9".into(), endpoint: Some(InetSocketAddress { host: "host".into(), port: 9999 }), am_relay: Some(true), relay_id: Some("RID".into()), relay_sig: None, date: "2025-01-03T00:00:00Z".into(), sig: Some("RSIG".into()) };
    let dump = Message::Ddb(DdbMessage::DumpResolve(DdbDumpResolve { app: "ddb".into(), record: rec.clone(), tag: None, text: None, data: None }));
    let jd = marshal::to_json_string(&dump);
    let d2 = marshal::from_json_str(&jd).unwrap();
    assert_eq!(dump, d2);

    let dump_resp = Message::Ddb(DdbMessage::DumpResolveResponse(DdbDumpResolveResponse {
        app: "ddb".into(),
        record_index: 1,
        record_id: rec.id.clone(),
        record: rec.clone(),
        response_tag: None,
        text: None,
        data: None,
    }));
    let jr = marshal::to_json_string(&dump_resp);
    let dr2 = marshal::from_json_str(&jr).unwrap();
    assert_eq!(dump_resp, dr2);
}
