//! Tests for the APNs `/register` path: raw-token → hex → signed envelope → poster (bingle_notify #i).

use std::sync::{Arc, Mutex};

use bingle_local::api::notify::register::{APNS_TOKEN_BYTES, RegisterRequest, encode_apns_token};
use bingle_local::api::notify::RegisterPoster;
use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};
use bingle_core::api::bingle_api::BingleError;

const TEST_MNEMONIC: &str = "square flat curtain negative three april hobby culture unit fit drip bronze cactus stage vault pluck captain nation pond pizza grief domain coin abstract path";
const ISS: &str = "alice";
const GATEWAY: &str = "https://gw.example";

/// Test poster: records every `/register` it is handed and reports acceptance, instead of hitting a
/// live gateway.
#[derive(Default)]
struct RecordingRegisterPoster {
    calls: Mutex<Vec<(String, RegisterRequest)>>,
}

impl RecordingRegisterPoster {
    fn calls(&self) -> Vec<(String, RegisterRequest)> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl RegisterPoster for RecordingRegisterPoster {
    fn post_register(
        &self,
        gateway_url: &str,
        body: RegisterRequest,
    ) -> Result<bool, BingleError> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((gateway_url.to_string(), body));
        Ok(true)
    }
}

/// A register-ready API: the test keypair imported, the local handle seeded, a recording poster
/// injected, and the given gateway/env configured.
fn register_api(
    gateway_url: Option<&str>,
    notify_env: &str,
) -> (BingleApiLocalImpl, Arc<RecordingRegisterPoster>) {
    let config = LocalApiConfig {
        notify_gateway_url: gateway_url.map(|s| s.to_string()),
        notify_env: notify_env.to_string(),
        ..LocalApiConfig::default()
    };
    let mut api = BingleApiLocalImpl::new(config);
    let poster = Arc::new(RecordingRegisterPoster::default());
    api.set_register_poster(poster.clone());
    api.import_keypair(TEST_MNEMONIC.to_string())
        .expect("import test keypair");
    api.seed_own_handle_for_tests(ISS.to_string());
    (api, poster)
}

/// A raw 32-byte token whose ascending bytes make the expected hex obvious.
fn ascending_token() -> Vec<u8> {
    (0u8..APNS_TOKEN_BYTES as u8).collect()
}

#[test]
fn encode_apns_token_is_lowercase_hex() {
    let hex = encode_apns_token(&ascending_token()).expect("32 bytes encodes");
    assert_eq!(
        hex,
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
    );
    assert_eq!(hex.len(), 64, "32 bytes -> 64 hex chars");
}

#[test]
fn encode_apns_token_rejects_wrong_length() {
    assert!(encode_apns_token(&[0u8; 31]).is_err(), "31 bytes must fail");
    assert!(encode_apns_token(&[0u8; 33]).is_err(), "33 bytes must fail");
    // The exact real-world failure: an 80-byte token must be rejected, not stored.
    assert!(encode_apns_token(&[0u8; 80]).is_err(), "80 bytes must fail");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn register_posts_signed_envelope_with_hex_token() {
    let (api, poster) = register_api(Some(GATEWAY), "sandbox");
    let accepted = api
        .register_apns_token(ascending_token())
        .expect("register should succeed");
    assert!(accepted, "recording poster reports acceptance");

    let calls = poster.calls();
    assert_eq!(calls.len(), 1, "exactly one /register posted");
    let (url, req) = &calls[0];
    assert_eq!(url, GATEWAY);
    assert_eq!(req.iss, ISS);
    assert_eq!(req.env, "sandbox");
    assert_eq!(
        req.token,
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        "token must be the lowercase-hex of the raw bytes"
    );

    // The posted signature is a valid register envelope for the recorded token/env/nonce/exp.
    let expected = api
        .get_algo_ops()
        .expect("ops")
        .sign_notify_envelope("register", ISS, "", &req.token, &req.env, &req.nonce, req.exp)
        .expect("re-sign");
    assert_eq!(
        req.sig, expected,
        "posted signature must be a valid register envelope"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn register_uses_configured_env() {
    let (api, poster) = register_api(Some(GATEWAY), "production");
    api.register_apns_token(ascending_token())
        .expect("register should succeed");
    assert_eq!(poster.calls()[0].1.env, "production");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn register_rejects_wrong_length_token_without_posting() {
    let (api, poster) = register_api(Some(GATEWAY), "sandbox");
    let err = api.register_apns_token(vec![0u8; 80]);
    assert!(err.is_err(), "an 80-byte token must be rejected");
    assert!(poster.calls().is_empty(), "nothing is posted for a bad token");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn register_without_gateway_url_errors() {
    let (api, poster) = register_api(None, "sandbox");
    let err = api.register_apns_token(ascending_token());
    assert!(err.is_err(), "no gateway URL must error");
    assert!(poster.calls().is_empty(), "nothing posted without a gateway");
}
