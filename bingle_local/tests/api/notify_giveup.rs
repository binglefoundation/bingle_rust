//! Tests for the store-and-retry give-up -> `/alert` nudge (bingle_notify #11).

use std::sync::{Arc, Mutex};

use bingle_core::blockchain::algo_ops::{AlgoChainConfig, AlgoOps};
use bingle_local::api::notify::{
    AlertPoster, AlertRequest, alert_status_accepted, build_alert_request,
};
use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};

// Committed cross-impl parity vector's `alert` case (source of truth:
// bingle_notify/test/fixtures/verify-vector.json). Ed25519 is deterministic, so a correct signer
// reproduces this signature byte-for-byte — exactly what the gateway's algosdk.verifyBytes accepts.
const TEST_MNEMONIC: &str = "square flat curtain negative three april hobby culture unit fit drip bronze cactus stage vault pluck captain nation pond pizza grief domain coin abstract path";
const ALERT_ISS: &str = "alice";
const ALERT_AUDIENCE: &str = "bob";
const ALERT_NONCE: &str = "AAAAAAAAAAAAAAAAAAAAAA";
const ALERT_EXP: i64 = 1893456000;
const ALERT_SIG: &str =
    "9MMEaEHeZavEh53x0OcWbHzS7dasYW3hh8VqqNOVQZDQF8Bq3BYLX4wilD9XGczHznjn+Kf5WHxllp9JS2MwAw==";

/// Test poster: records every `/alert` it is handed instead of hitting a live gateway.
#[derive(Default)]
struct RecordingPoster {
    calls: Mutex<Vec<(String, AlertRequest)>>,
}

impl RecordingPoster {
    fn calls(&self) -> Vec<(String, AlertRequest)> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl AlertPoster for RecordingPoster {
    fn post_alert(&self, gateway_url: &str, body: AlertRequest) {
        self.calls
            .lock()
            .expect("calls lock")
            .push((gateway_url.to_string(), body));
    }
}

fn test_ops() -> AlgoOps {
    AlgoOps::new(
        Some(TEST_MNEMONIC.to_string()),
        None,
        Some(AlgoChainConfig::default()),
    )
}

/// Build a give-up-ready API: the test keypair imported, the local handle seeded (no network), a
/// recording poster injected, and the nudge gated on with a gateway URL.
fn giveup_api(
    gateway_url: Option<&str>,
    notify_on_giveup: bool,
) -> (BingleApiLocalImpl, Arc<RecordingPoster>) {
    let config = LocalApiConfig {
        notify_on_giveup,
        notify_gateway_url: gateway_url.map(|s| s.to_string()),
        ..LocalApiConfig::default()
    };
    let mut api = BingleApiLocalImpl::new(config);
    let poster = Arc::new(RecordingPoster::default());
    api.set_alert_poster(poster.clone());
    api.import_keypair(TEST_MNEMONIC.to_string())
        .expect("import test keypair");
    // Seed the local handle the envelope is issued as (import clears it).
    api.seed_own_handle_for_tests(ALERT_ISS.to_string());
    (api, poster)
}

/// `LocalApiConfig::default()` keeps the feature on but sends nothing until a URL is set.
#[test]
fn default_config_gates_on_but_has_no_url() {
    let config = LocalApiConfig::default();
    assert!(config.notify_on_giveup, "feature defaults on (#11)");
    assert!(
        config.notify_gateway_url.is_none(),
        "gateway URL defaults to None so a defaulted config makes no call"
    );
}

/// `with_notify` is the shared mapping used by the JSI and webserver callers (#17): the gateway URL
/// passes through and `notify_on_giveup` defaults to `true` when the caller leaves it unset.
#[test]
fn with_notify_maps_gateway_url_and_defaults_flag_on() {
    let default_algo = bingle_core::blockchain::algo_ops::AlgoChainConfig::default();

    // Caller supplies a URL and leaves the flag unset ⇒ URL reaches the config, flag defaults on.
    let cfg = LocalApiConfig::with_notify(
        default_algo.clone(),
        111,
        222,
        None,
        Some("https://gw.example".to_string()),
    );
    assert_eq!(cfg.app_id, 111);
    assert_eq!(cfg.asset_id, 222);
    assert_eq!(
        cfg.notify_gateway_url.as_deref(),
        Some("https://gw.example")
    );
    assert!(cfg.notify_on_giveup, "unset flag defaults on");

    // Caller can disable the nudge even with a URL configured.
    let disabled = LocalApiConfig::with_notify(
        default_algo.clone(),
        0,
        0,
        Some(false),
        Some("https://gw.example".to_string()),
    );
    assert!(
        !disabled.notify_on_giveup,
        "explicit false disables the nudge"
    );

    // No URL ⇒ dormant regardless of the flag.
    let dormant = LocalApiConfig::with_notify(default_algo, 0, 0, Some(true), None);
    assert!(dormant.notify_gateway_url.is_none());
}

/// The signed content-free alert envelope is byte-for-byte identical to the committed cross-impl
/// vector's alert case (`bodyHash = sha256("")`).
#[test]
#[cfg(not(target_os = "ios"))]
fn alert_envelope_matches_cross_impl_vector() {
    let req = build_alert_request(
        &test_ops(),
        ALERT_ISS,
        ALERT_AUDIENCE,
        ALERT_NONCE,
        ALERT_EXP,
    )
    .expect("alert envelope should sign");
    assert_eq!(req.sig, ALERT_SIG, "alert signature must match the vector");
    // Content-free: the request carries only the envelope, no sender/preview/ciphertext.
    assert_eq!(req.iss, ALERT_ISS);
    assert_eq!(req.audience, ALERT_AUDIENCE);
    assert_eq!(req.nonce, ALERT_NONCE);
    assert_eq!(req.exp, ALERT_EXP);
}

/// A serialized alert body carries exactly the five envelope fields — no content.
#[test]
#[cfg(not(target_os = "ios"))]
fn alert_request_serializes_content_free() {
    let req = build_alert_request(
        &test_ops(),
        ALERT_ISS,
        ALERT_AUDIENCE,
        ALERT_NONCE,
        ALERT_EXP,
    )
    .expect("sign");
    let value: serde_json::Value = serde_json::to_value(&req).expect("serialize");
    let obj = value.as_object().expect("object");
    let mut keys: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["audience", "exp", "iss", "nonce", "sig"]);
}

/// With the flag off, give-up sends no `/alert` even when a URL is configured.
#[test]
#[cfg(not(target_os = "ios"))]
fn giveup_with_flag_off_sends_nothing() {
    let (mut api, poster) = giveup_api(Some("https://gw.example"), false);
    api.add_message(
        ALERT_ISS.into(),
        vec![ALERT_AUDIENCE.into()],
        42,
        "hi".into(),
        None,
    )
    .expect("add message");
    api.update_message_status(42, 1.0, Some("permanent".into()))
        .expect("update");
    assert!(poster.calls().is_empty(), "flag off must send no alert");
}

/// With no gateway URL, give-up sends no `/alert` even though the flag defaults on.
#[test]
#[cfg(not(target_os = "ios"))]
fn giveup_with_no_url_sends_nothing() {
    let (mut api, poster) = giveup_api(None, true);
    api.add_message(
        ALERT_ISS.into(),
        vec![ALERT_AUDIENCE.into()],
        42,
        "hi".into(),
        None,
    )
    .expect("add message");
    api.update_message_status(42, 1.0, Some("permanent".into()))
        .expect("update");
    assert!(poster.calls().is_empty(), "no URL must send no alert");
}

/// The nudge fires on the first unreachable/failed report — waking the offline recipient while the
/// message is still retrying — and then never again for that message: not on later retries, not on a
/// subsequent give-up. It posts a valid signed envelope to `{url}/alert` for the recipient.
#[test]
#[cfg(not(target_os = "ios"))]
fn nudge_fires_once_on_first_unreachable_not_per_retry() {
    let (mut api, poster) = giveup_api(Some("https://gw.example/"), true);
    api.add_message(
        ALERT_ISS.into(),
        vec![ALERT_AUDIENCE.into()],
        7,
        "hi".into(),
        None,
    )
    .expect("add message");

    // First unreachable report (transient, still retrying, progress < 1.0) fires exactly one nudge —
    // this is the case give-up-only nudging never reached, since an unreachable message retries
    // forever and never gives up.
    api.update_message_status(
        7,
        0.0,
        Some("Recipient unreachable — will keep retrying".into()),
    )
    .expect("first transient update");
    let calls = poster.calls();
    assert_eq!(
        calls.len(),
        1,
        "the first unreachable report fires one alert"
    );
    let (url, req) = &calls[0];
    assert_eq!(url, "https://gw.example/");
    assert_eq!(req.iss, ALERT_ISS);
    assert_eq!(req.audience, ALERT_AUDIENCE);

    // A later retry of the same message must not re-nudge.
    api.update_message_status(
        7,
        0.0,
        Some("Recipient unreachable — will keep retrying".into()),
    )
    .expect("second transient update");
    assert_eq!(
        poster.calls().len(),
        1,
        "a later retry of the same message must not re-nudge"
    );

    // A subsequent give-up must not re-nudge either — the message was already nudged once.
    api.update_message_status(7, 1.0, Some("permanent".into()))
        .expect("terminal update");
    assert_eq!(
        poster.calls().len(),
        1,
        "give-up after an unreachable nudge must not re-nudge"
    );

    // The posted signature is a valid alert envelope for the recorded nonce/exp.
    let expected = api
        .get_algo_ops()
        .expect("ops")
        .sign_notify_envelope(
            "alert",
            ALERT_ISS,
            ALERT_AUDIENCE,
            "",
            "",
            &req.nonce,
            req.exp,
        )
        .expect("re-sign");
    assert_eq!(
        req.sig, expected,
        "posted signature must be a valid alert envelope"
    );
}

/// The notify gate applies to the unreachable path too: with the flag off, an unreachable retry
/// sends no `/alert` even though a URL is configured.
#[test]
#[cfg(not(target_os = "ios"))]
fn unreachable_with_flag_off_sends_nothing() {
    let (mut api, poster) = giveup_api(Some("https://gw.example"), false);
    api.add_message(
        ALERT_ISS.into(),
        vec![ALERT_AUDIENCE.into()],
        11,
        "hi".into(),
        None,
    )
    .expect("add message");
    api.update_message_status(
        11,
        0.0,
        Some("Recipient unreachable — will keep retrying".into()),
    )
    .expect("transient update");
    assert!(
        poster.calls().is_empty(),
        "flag off must send no alert on an unreachable retry"
    );
}

/// Give-up fires one alert per recipient of the message.
#[test]
#[cfg(not(target_os = "ios"))]
fn giveup_fires_once_per_recipient() {
    let (mut api, poster) = giveup_api(Some("https://gw.example"), true);
    api.add_message(
        ALERT_ISS.into(),
        vec!["bob".into(), "carol".into()],
        9,
        "hi".into(),
        None,
    )
    .expect("add message");
    api.update_message_status(9, 1.0, Some("permanent".into()))
        .expect("terminal update");
    let audiences: Vec<String> = poster
        .calls()
        .into_iter()
        .map(|(_, r)| r.audience)
        .collect();
    assert_eq!(audiences.len(), 2);
    assert!(audiences.contains(&"bob".to_string()));
    assert!(audiences.contains(&"carol".to_string()));
}

/// A successful send (progress 1.0, no failure_reason) is not a give-up and fires no nudge.
#[test]
#[cfg(not(target_os = "ios"))]
fn successful_send_does_not_nudge() {
    let (mut api, poster) = giveup_api(Some("https://gw.example"), true);
    api.add_message(
        ALERT_ISS.into(),
        vec![ALERT_AUDIENCE.into()],
        3,
        "hi".into(),
        None,
    )
    .expect("add message");
    api.update_message_status(3, 1.0, None).expect("update");
    assert!(
        poster.calls().is_empty(),
        "a delivered message must not nudge"
    );
}

/// The nudge is best-effort: it never fails, blocks, or retries delivery. Even when the poster is
/// exercised, `update_message_status` returns Ok and the message's terminal-failure state is
/// unchanged by the nudge.
#[test]
#[cfg(not(target_os = "ios"))]
fn giveup_nudge_does_not_affect_delivery_outcome() {
    let (mut api, poster) = giveup_api(Some("https://gw.example"), true);
    api.add_message(
        ALERT_ISS.into(),
        vec![ALERT_AUDIENCE.into()],
        5,
        "hi".into(),
        None,
    )
    .expect("add message");
    let result = api.update_message_status(5, 1.0, Some("permanent".into()));
    assert!(result.is_ok(), "the nudge must never fail delivery");
    assert_eq!(poster.calls().len(), 1, "the nudge was exercised");

    // Delivery outcome is untouched by the nudge: still terminally failed with its reason.
    let msg = api
        .get_messages()
        .expect("messages")
        .into_iter()
        .find(|m| m.timestamp == 5)
        .expect("message present");
    assert_eq!(msg.progress, Some(1.0));
    assert_eq!(msg.failure_reason.as_deref(), Some("permanent"));
}

/// Best-effort status handling: 2xx and the coalescing 429 are accepted; other statuses (and, by
/// the same contract, transport errors) are ignored — logged, never retried.
#[test]
fn alert_status_classification() {
    assert!(alert_status_accepted(200));
    assert!(alert_status_accepted(202));
    assert!(alert_status_accepted(429));
    assert!(!alert_status_accepted(400));
    assert!(!alert_status_accepted(500));
    assert!(!alert_status_accepted(0));
}
