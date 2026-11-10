// The peer certificate handler must bind the CA's Ed25519 key to the id in its
// OrganizationName (O): only the holder of the id's key can issue certificates for it.

use crate::util::pki::make_ee;
use crate::util::test_util::{ADDRESS_RECEIVE, ADDRESS_SPEND, PASSPHRASE_SPEND, init_test_logging};
use algo_ops::AlgoOps;
use bingle_core::protocol::cert_verify::peer_certificate_handler;
use bingle_core::protocol::{ISSUER_SUFFIX, VIRTUAL_CA};
use openssl::asn1::Asn1Time;
use openssl::bn::BigNum;
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::{Id, PKey, Private};
use openssl::x509::extension::{BasicConstraints, KeyUsage, SubjectKeyIdentifier};
use openssl::x509::{X509, X509NameBuilder};

/// The Ed25519 key behind `ADDRESS_SPEND`, as the app derives it.
fn spend_key() -> PKey<Private> {
    let ops = AlgoOps::new_for_algorand(
        Some(PASSPHRASE_SPEND.to_string()),
        Some(ADDRESS_SPEND.to_string()),
        None,
    );
    let sk = ops.private_key_bytes().expect("private key bytes");
    PKey::private_key_from_raw_bytes(&sk, Id::ED25519).expect("ed25519 key")
}

/// A self-signed virtual CA with the given key and optional O field.
fn make_ca_with(key: &PKey<Private>, org: Option<&str>) -> X509 {
    let mut name = X509NameBuilder::new().expect("name builder");
    name.append_entry_by_nid(Nid::COMMONNAME, VIRTUAL_CA)
        .expect("cn");
    if let Some(org) = org {
        name.append_entry_by_nid(Nid::ORGANIZATIONNAME, org)
            .expect("o");
    }
    let name = name.build();

    let mut serial = BigNum::new().expect("bignum");
    serial
        .rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false)
        .expect("rand");

    let mut builder = X509::builder().expect("x509 builder");
    builder.set_version(2).expect("version");
    builder
        .set_serial_number(&serial.to_asn1_integer().expect("asn1 serial"))
        .expect("serial");
    builder.set_subject_name(&name).expect("subject");
    builder.set_issuer_name(&name).expect("issuer");
    builder.set_pubkey(key).expect("pubkey");
    builder
        .set_not_before(&Asn1Time::days_from_now(0).expect("nb"))
        .expect("set nb");
    builder
        .set_not_after(&Asn1Time::days_from_now(2).expect("na"))
        .expect("set na");
    let bc = BasicConstraints::new().critical().ca().build().expect("bc");
    builder.append_extension(bc).expect("append bc");
    let ku = KeyUsage::new()
        .critical()
        .key_cert_sign()
        .crl_sign()
        .build()
        .expect("ku");
    builder.append_extension(ku).expect("append ku");
    let skid = SubjectKeyIdentifier::new()
        .build(&builder.x509v3_context(None, None))
        .expect("skid");
    builder.append_extension(skid).expect("append skid");
    builder.sign(key, MessageDigest::null()).expect("sign");
    builder.build()
}

/// Run the handler on a leaf for `id` issued by a CA with `ca_key` and `ca_org`.
fn verify(ca_key: &PKey<Private>, ca_org: Option<&str>, id: &str) -> Result<String, String> {
    let ca = make_ca_with(ca_key, ca_org);
    let (leaf, _) = make_ee(&ca, ca_key, &format!("{}{}", id, ISSUER_SUFFIX));
    let handler = peer_certificate_handler();
    handler(
        &leaf.to_pem().expect("leaf pem"),
        &ca.to_pem().expect("ca pem"),
    )
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn peer_certificate_handler_accepts_ca_keyed_by_its_id() {
    init_test_logging();
    let res = verify(&spend_key(), Some(ADDRESS_SPEND), ADDRESS_SPEND);
    assert_eq!(res, Ok(format!("{}{}", ADDRESS_SPEND, ISSUER_SUFFIX)));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn peer_certificate_handler_rejects_ca_key_not_matching_its_id() {
    init_test_logging();
    // A fresh key that claims ADDRESS_SPEND but doesn't hold its secret.
    let other = PKey::generate_ed25519().expect("ed25519 gen");
    let res = verify(&other, Some(ADDRESS_SPEND), ADDRESS_SPEND);
    assert!(
        res.is_err(),
        "accepted a CA whose key isn't the id's key: {:?}",
        res
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn peer_certificate_handler_rejects_genuine_key_claiming_another_id() {
    init_test_logging();
    // The real ADDRESS_SPEND key, issuing for ADDRESS_RECEIVE.
    let res = verify(&spend_key(), Some(ADDRESS_RECEIVE), ADDRESS_RECEIVE);
    assert!(
        res.is_err(),
        "accepted a CA keyed by a different id: {:?}",
        res
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn peer_certificate_handler_rejects_ca_without_organization() {
    init_test_logging();
    let res = verify(&spend_key(), None, ADDRESS_SPEND);
    assert!(res.is_err(), "accepted a CA with no O field: {:?}", res);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn peer_certificate_handler_rejects_organization_that_isnt_an_address() {
    init_test_logging();
    let res = verify(&spend_key(), Some("not-an-address"), "not-an-address");
    assert!(
        res.is_err(),
        "accepted a CA whose O isn't an address: {:?}",
        res
    );
}
