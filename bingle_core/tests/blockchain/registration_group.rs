//! Tests for checking and co-signing the registration service's groups (issue #344).
//!
//! `fixtures/registration_groups.json` holds groups built by the service's own group builder, with
//! fixed test keys, and the signed client transactions algosdk produced for them (see
//! `fixtures/generate_registration_groups.mjs`). Ed25519 signatures are deterministic, so the
//! client's output must match algosdk's byte for byte.

use algo_ops::AlgoOps;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use bingle_core::blockchain::registration_group::{
    RegistrationGroupError, RegistrationGroupIds, app_address, sign_recovery_group,
    sign_registration_group,
};
use ed25519_dalek::SigningKey;
use rmpv::Value;
use serde_json::Value as Json;

const FIXTURE: &str = include_str!("fixtures/registration_groups.json");

struct Fixture {
    json: Json,
}

impl Fixture {
    fn load() -> Self {
        Self {
            json: serde_json::from_str(FIXTURE).expect("fixture JSON"),
        }
    }

    fn ids(&self) -> RegistrationGroupIds {
        RegistrationGroupIds {
            bingle_app_id: self.json["bingle_app_id"].as_u64().expect("app id"),
            bingle_asset_id: self.json["bingle_asset_id"].as_u64().expect("asset id"),
        }
    }

    fn handle(&self) -> String {
        self.json["handle"].as_str().expect("handle").to_string()
    }

    fn key(&self, who: &str) -> SigningKey {
        let mnemonic = self.json[who]["mnemonic"].as_str().expect("mnemonic");
        SigningKey::from_bytes(&AlgoOps::seed_from_passphrase(mnemonic).expect("seed"))
    }

    fn address(&self, who: &str) -> Vec<u8> {
        let address = self.json[who]["address"].as_str().expect("address");
        let parsed: algonaut::core::Address = address.parse().expect("address parses");
        parsed.0.to_vec()
    }

    fn strings(&self, group: &str, field: &str) -> Vec<String> {
        self.json[group][field]
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v.as_str().expect("string").to_string())
            .collect()
    }

    fn transactions(&self, group: &str) -> Vec<String> {
        self.strings(group, "transactions")
    }

    fn signed(&self, group: &str) -> Vec<String> {
        self.strings(group, "signed_transactions")
    }

    fn sign_indexes(&self, group: &str) -> Vec<u32> {
        self.json[group]["sign_indexes"]
            .as_array()
            .expect("array")
            .iter()
            .map(|v| u32::try_from(v.as_u64().expect("index")).expect("small index"))
            .collect()
    }

    fn register(&self, transactions: &[String]) -> Result<Vec<String>, RegistrationGroupError> {
        sign_registration_group(
            transactions,
            &self.sign_indexes("registration"),
            self.ids(),
            &self.handle(),
            &self.key("client"),
        )
    }

    fn recover(&self, transactions: &[String]) -> Result<Vec<String>, RegistrationGroupError> {
        sign_recovery_group(
            transactions,
            &self.sign_indexes("recovery"),
            self.ids(),
            &self.handle(),
            &self.key("client"),
            &self.key("recovered"),
        )
    }
}

/// Decodes one transaction, applies `change` to its msgpack map, and re-encodes it.
fn tamper(
    transactions: &[String],
    index: usize,
    change: impl FnOnce(&mut Vec<(Value, Value)>),
) -> Vec<String> {
    let mut out = transactions.to_vec();
    let bytes = BASE64.decode(&out[index]).expect("base64");
    let mut value = rmpv::decode::read_value(&mut bytes.as_slice()).expect("msgpack");
    let Value::Map(map) = &mut value else {
        panic!("transaction {index} is not a map");
    };
    change(map);
    let mut encoded = Vec::new();
    rmpv::encode::write_value(&mut encoded, &value).expect("encode");
    out[index] = BASE64.encode(encoded);
    out
}

/// Sets (or adds) a field in a transaction map.
fn set(map: &mut Vec<(Value, Value)>, key: &str, value: Value) {
    match map.iter_mut().find(|(k, _)| k.as_str() == Some(key)) {
        Some(entry) => entry.1 = value,
        None => map.push((Value::from(key), value)),
    }
}

fn bytes(value: &[u8]) -> Value {
    Value::Binary(value.to_vec())
}

fn assert_template(result: Result<Vec<String>, RegistrationGroupError>, index: usize) {
    match result {
        Err(RegistrationGroupError::Template { index: at, .. }) => assert_eq!(at, index),
        other => panic!("expected a template error at {index}, got {other:?}"),
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_group_signs_as_algosdk_does() {
    let f = Fixture::load();
    let signed = f.register(&f.transactions("registration")).expect("signs");
    assert_eq!(signed, f.signed("registration"));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_group_with_invite_signs_as_algosdk_does() {
    let f = Fixture::load();
    let signed = f
        .register(&f.transactions("registration_invite"))
        .expect("signs");
    assert_eq!(signed, f.signed("registration_invite"));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_group_closing_out_to_the_funder_with_zero_fees_signs_as_algosdk_does() {
    let f = Fixture::load();
    let signed = f.recover(&f.transactions("recovery")).expect("signs");
    assert_eq!(signed, f.signed("recovery"));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_group_with_invite_signs_as_algosdk_does() {
    let f = Fixture::load();
    let signed = f
        .recover(&f.transactions("recovery_invite"))
        .expect("signs");
    assert_eq!(signed, f.signed("recovery_invite"));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_a_payment_to_another_receiver() {
    let f = Fixture::load();
    let funder = f.address("funder");
    let txns = tamper(&f.transactions("registration"), 4, |m| {
        set(m, "arcv", bytes(&funder))
    });
    assert_template(f.register(&txns), 4);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_a_different_amount() {
    let f = Fixture::load();
    let txns = tamper(&f.transactions("registration"), 4, |m| {
        set(m, "aamt", Value::from(2))
    });
    assert_template(f.register(&txns), 4);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_an_amount_on_the_opt_in() {
    let f = Fixture::load();
    let txns = tamper(&f.transactions("registration"), 1, |m| {
        set(m, "aamt", Value::from(1))
    });
    assert_template(f.register(&txns), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_a_rekey() {
    let f = Fixture::load();
    let funder = f.address("funder");
    let txns = tamper(&f.transactions("registration"), 3, |m| {
        set(m, "rekey", bytes(&funder))
    });
    assert_template(f.register(&txns), 3);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_a_close_to() {
    let f = Fixture::load();
    let funder = f.address("funder");
    let txns = tamper(&f.transactions("registration"), 1, |m| {
        set(m, "aclose", bytes(&funder))
    });
    assert_template(f.register(&txns), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_a_fee_above_the_minimum() {
    let f = Fixture::load();
    let txns = tamper(&f.transactions("registration"), 5, |m| {
        set(m, "fee", Value::from(2000))
    });
    assert_template(f.register(&txns), 5);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_another_handle() {
    let f = Fixture::load();
    let result = sign_registration_group(
        &f.transactions("registration"),
        &f.sign_indexes("registration"),
        f.ids(),
        "zoë_test",
        &f.key("client"),
    );
    assert_template(result, 5);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_another_app() {
    let f = Fixture::load();
    let ids = RegistrationGroupIds {
        bingle_app_id: f.ids().bingle_app_id + 1,
        ..f.ids()
    };
    let result = sign_registration_group(
        &f.transactions("registration"),
        &f.sign_indexes("registration"),
        ids,
        &f.handle(),
        &f.key("client"),
    );
    assert_template(result, 3);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_an_unexpected_field() {
    let f = Fixture::load();
    let txns = tamper(&f.transactions("registration"), 5, |m| {
        set(m, "apap", bytes(&[1, 2, 3]))
    });
    assert!(matches!(
        f.register(&txns),
        Err(RegistrationGroupError::Decode { index: 5, .. })
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_another_signer() {
    let f = Fixture::load();
    let result = sign_registration_group(
        &f.transactions("registration"),
        &f.sign_indexes("registration"),
        f.ids(),
        &f.handle(),
        &f.key("recovered"),
    );
    assert_template(result, 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_other_sign_indexes() {
    let f = Fixture::load();
    let result = sign_registration_group(
        &f.transactions("registration"),
        &[1, 3, 4, 6],
        f.ids(),
        &f.handle(),
        &f.key("client"),
    );
    assert!(matches!(
        result,
        Err(RegistrationGroupError::SignIndexes { .. })
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_a_transaction_from_another_group() {
    let f = Fixture::load();
    let txns = tamper(&f.transactions("registration"), 6, |m| {
        set(m, "grp", bytes(&[0; 32]))
    });
    assert!(matches!(
        f.register(&txns),
        Err(RegistrationGroupError::Group { index: 6 })
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_refuses_a_recovery_sized_group() {
    let f = Fixture::load();
    assert!(matches!(
        f.register(&f.transactions("recovery")),
        Err(RegistrationGroupError::GroupSize { actual: 9, .. })
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_refuses_a_close_out_to_another_address() {
    let f = Fixture::load();
    let service = f.address("service");
    let txns = tamper(&f.transactions("recovery"), 1, |m| {
        set(m, "arcv", bytes(&service));
        set(m, "aclose", bytes(&service));
    });
    assert_template(f.recover(&txns), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_refuses_an_algo_close_to_another_address() {
    let f = Fixture::load();
    let funder = f.address("funder");
    let txns = tamper(&f.transactions("recovery"), 2, |m| {
        set(m, "close", bytes(&funder))
    });
    assert_template(f.recover(&txns), 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_refuses_a_funder_transfer_to_another_address() {
    let f = Fixture::load();
    let client = f.address("client");
    let txns = tamper(&f.transactions("recovery"), 4, |m| {
        set(m, "arcv", bytes(&client))
    });
    assert_template(f.recover(&txns), 4);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_refuses_a_rekey_of_the_old_account() {
    let f = Fixture::load();
    let funder = f.address("funder");
    let txns = tamper(&f.transactions("recovery"), 0, |m| {
        set(m, "rekey", bytes(&funder))
    });
    assert_template(f.recover(&txns), 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_refuses_a_payment_to_another_app() {
    let f = Fixture::load();
    let other = app_address(f.ids().bingle_app_id + 1);
    let txns = tamper(&f.transactions("recovery"), 6, |m| {
        set(m, "arcv", bytes(&other))
    });
    assert_template(f.recover(&txns), 6);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_refuses_swapped_keys() {
    let f = Fixture::load();
    let result = sign_recovery_group(
        &f.transactions("recovery"),
        &f.sign_indexes("recovery"),
        f.ids(),
        &f.handle(),
        &f.key("recovered"),
        &f.key("client"),
    );
    // The funder's transfer is checked first, and it doesn't pay the (swapped) new address.
    assert_template(result, 4);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn app_address_matches_the_group() {
    let f = Fixture::load();
    // Transaction 4 of the registration group pays the Bingle DApp's account.
    let bytes = BASE64
        .decode(&f.transactions("registration")[4])
        .expect("base64");
    let value = rmpv::decode::read_value(&mut bytes.as_slice()).expect("msgpack");
    let receiver = value["arcv"].as_slice().expect("arcv").to_vec();
    assert_eq!(receiver, app_address(f.ids().bingle_app_id));
}
