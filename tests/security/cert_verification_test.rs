use openssl::asn1::Asn1Time;
use openssl::bn::BigNum;
use openssl::ec::{EcGroup, EcKey};
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::{PKey, Private};
use openssl::x509::extension::{AuthorityKeyIdentifier, BasicConstraints, ExtendedKeyUsage, KeyUsage, SubjectKeyIdentifier};
use openssl::x509::{X509NameBuilder, X509};

use rust_comms::protocol::cert_verify::peer_certificate_handler;
use rust_comms::protocol::{VIRTUAL_CA, ISSUER_SUFFIX};

fn make_serial() -> BigNum {
    let mut serial = BigNum::new().expect("bignum");
    serial.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false).expect("rand");
    serial
}

fn make_ca_custom(cn: &str, key: &str, nb_days: i32, na_days: i32, is_ca: bool) -> (X509, PKey<Private>) {
    let pkey = PKey::generate_ed25519().expect("ed25519 gen");
    let mut name = X509NameBuilder::new().expect("name builder");
    name.append_entry_by_nid(Nid::COMMONNAME, cn).expect("cn");
    name.append_entry_by_nid(Nid::ORGANIZATIONNAME, key).expect("o");
    let name = name.build();

    let mut builder = X509::builder().expect("x509 builder");
    builder.set_version(2).expect("version");
    builder.set_serial_number(&make_serial().to_asn1_integer().expect("asn1 serial")).expect("serial");
    builder.set_subject_name(&name).expect("subject");
    builder.set_issuer_name(&name).expect("issuer");
    builder.set_pubkey(&pkey).expect("pubkey");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let nb_unix = now + (nb_days as i64 * 24 * 3600);
    let na_unix = now + (na_days as i64 * 24 * 3600);

    let not_before = Asn1Time::from_unix(nb_unix).expect("not before");
    builder.set_not_before(&not_before).expect("set not before");
    let not_after = Asn1Time::from_unix(na_unix).expect("not after");
    builder.set_not_after(&not_after).expect("set not after");

    if is_ca {
        let bc = BasicConstraints::new().critical().ca().build().expect("bc");
        builder.append_extension(bc).expect("append bc");
    } else {
        let bc = BasicConstraints::new().critical().build().expect("bc");
        builder.append_extension(bc).expect("append bc");
    }
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

    builder.sign(&pkey, MessageDigest::null()).expect("sign");
    (builder.build(), pkey)
}

fn make_ee_custom(ca_cert: &X509, ca_key: &PKey<Private>, cn: &str, nb_days: i32, na_days: i32, is_ca: bool) -> (X509, PKey<Private>) {
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).expect("ecgroup");
    let ec_key = EcKey::generate(&group).expect("eckey gen");
    let pkey = PKey::from_ec_key(ec_key).expect("pkey from ec");

    let mut name = X509NameBuilder::new().expect("name builder");
    name.append_entry_by_nid(Nid::COMMONNAME, cn).expect("cn");
    let name = name.build();

    let mut builder = X509::builder().expect("x509 builder");
    builder.set_version(2).expect("version");
    builder.set_serial_number(&make_serial().to_asn1_integer().expect("asn1 serial")).expect("serial");
    builder.set_subject_name(&name).expect("subject");
    builder.set_issuer_name(ca_cert.subject_name()).expect("issuer");
    builder.set_pubkey(&pkey).expect("pubkey");

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let nb_unix = now + (nb_days as i64 * 24 * 3600);
    let na_unix = now + (na_days as i64 * 24 * 3600);

    let not_before = Asn1Time::from_unix(nb_unix).expect("not before");
    builder.set_not_before(&not_before).expect("set not before");
    let not_after = Asn1Time::from_unix(na_unix).expect("not after");
    builder.set_not_after(&not_after).expect("set not after");

    if is_ca {
        let bc = BasicConstraints::new().critical().ca().build().expect("bc");
        builder.append_extension(bc).expect("append bc");
    } else {
        let bc = BasicConstraints::new().critical().build().expect("bc");
        builder.append_extension(bc).expect("append bc");
    }
    let ku = KeyUsage::new()
        .critical()
        .digital_signature()
        .key_encipherment()
        .build()
        .expect("ku");
    builder.append_extension(ku).expect("append ku");
    let eku = ExtendedKeyUsage::new()
        .server_auth()
        .client_auth()
        .build()
        .expect("eku");
    builder.append_extension(eku).expect("append eku");
    let skid = SubjectKeyIdentifier::new()
        .build(&builder.x509v3_context(Some(ca_cert), None))
        .expect("skid");
    builder.append_extension(skid).expect("append skid");
    let akid = AuthorityKeyIdentifier::new()
        .keyid(true)
        .build(&builder.x509v3_context(Some(ca_cert), None))
        .expect("akid");
    builder.append_extension(akid).expect("append akid");

    builder.sign(ca_key, MessageDigest::null()).expect("sign");
    (builder.build(), pkey)
}

#[test]
fn expired_peer_cert_rejected() {
    let address = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
    let (ca_cert, ca_key) = make_ca_custom(VIRTUAL_CA, address, -10, 10, true);
    let cn = format!("{}{}", address, ISSUER_SUFFIX);
    // Peer cert expired (nb=-5, na=-1)
    let (ee_cert, _) = make_ee_custom(&ca_cert, &ca_key, &cn, -5, -1, false);

    let handler = peer_certificate_handler();
    let res = handler(&ee_cert.to_pem().unwrap(), &ca_cert.to_pem().unwrap());

    assert!(res.is_err());
    assert!(res.unwrap_err().contains("peer certificate expired"));
}

#[test]
fn not_yet_valid_peer_cert_rejected() {
    let address = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
    let (ca_cert, ca_key) = make_ca_custom(VIRTUAL_CA, address, -10, 10, true);
    let cn = format!("{}{}", address, ISSUER_SUFFIX);
    // Peer cert not yet valid (nb=1, na=5)
    let (ee_cert, _) = make_ee_custom(&ca_cert, &ca_key, &cn, 1, 5, false);

    let handler = peer_certificate_handler();
    let res = handler(&ee_cert.to_pem().unwrap(), &ca_cert.to_pem().unwrap());

    assert!(res.is_err());
    assert!(res.unwrap_err().contains("peer certificate not yet valid"));
}

#[test]
fn expired_ca_cert_rejected() {
    let address = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
    // CA cert expired (nb=-10, na=-1)
    let (ca_cert, ca_key) = make_ca_custom(VIRTUAL_CA, address, -10, -1, true);
    let cn = format!("{}{}", address, ISSUER_SUFFIX);
    let (ee_cert, _) = make_ee_custom(&ca_cert, &ca_key, &cn, -5, 5, false);

    let handler = peer_certificate_handler();
    let res = handler(&ee_cert.to_pem().unwrap(), &ca_cert.to_pem().unwrap());

    assert!(res.is_err());
    assert!(res.unwrap_err().contains("CA certificate expired"));
}

#[test]
fn untrusted_ca_rejected() {
    let address = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
    // CA with WRONG CN
    let (ca_cert, ca_key) = make_ca_custom("WRONG_CA", address, 0, 1, true);

    let cn = format!("{}{}", address, ISSUER_SUFFIX);
    let (ee_cert, _) = make_ee_custom(&ca_cert, &ca_key, &cn, 0, 1, false);

    let handler = peer_certificate_handler();
    let res = handler(&ee_cert.to_pem().unwrap(), &ca_cert.to_pem().unwrap());

    assert!(res.is_err());
    assert!(res.unwrap_err().contains("unexpected CA CN"));
}

#[test]
fn identity_mismatch_rejected() {
    let address_ca = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
    let address_ee = "OO3BIFZDJPGMNXZ74NOVH5KZ5WBL3KCPLPELAF32P7HDCQGQIBID7PJC7A";
    
    let (ca_cert, ca_key) = make_ca_custom(VIRTUAL_CA, address_ca, -1, 1, true);
    // EE CN uses address_ee, but CA OrganizationName is address_ca
    let cn = format!("{}{}", address_ee, ISSUER_SUFFIX);
    let (ee_cert, _) = make_ee_custom(&ca_cert, &ca_key, &cn, 0, 1, false);

    let handler = peer_certificate_handler();
    let res = handler(&ee_cert.to_pem().unwrap(), &ca_cert.to_pem().unwrap());

    assert!(res.is_err());
    assert!(res.unwrap_err().contains("CA O (org) does not match EE subject CN without suffix"));
}

#[test]
fn valid_certs_accepted() {
    let address = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
    let (ca_cert, ca_key) = make_ca_custom(VIRTUAL_CA, address, -1, 1, true);
    let cn = format!("{}{}", address, ISSUER_SUFFIX);
    let (ee_cert, _) = make_ee_custom(&ca_cert, &ca_key, &cn, 0, 1, false);

    let handler = peer_certificate_handler();
    let res = handler(&ee_cert.to_pem().unwrap(), &ca_cert.to_pem().unwrap());

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), cn);
}

#[test]
fn ca_with_basic_constraints_false_rejected() {
    let address = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
    // CA but with CA:FALSE
    let (ca_cert, ca_key) = make_ca_custom(VIRTUAL_CA, address, -1, 1, false);
    let cn = format!("{}{}", address, ISSUER_SUFFIX);
    let (ee_cert, _) = make_ee_custom(&ca_cert, &ca_key, &cn, 0, 1, false);

    let handler = peer_certificate_handler();
    let res = handler(&ee_cert.to_pem().unwrap(), &ca_cert.to_pem().unwrap());

    assert!(res.is_err());
    assert!(res.unwrap_err().contains("CA certificate basic constraints: CA is false"));
}

#[test]
fn ee_with_basic_constraints_true_rejected() {
    let address = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
    let (ca_cert, ca_key) = make_ca_custom(VIRTUAL_CA, address, -1, 1, true);
    let cn = format!("{}{}", address, ISSUER_SUFFIX);
    // EE but with CA:TRUE
    let (ee_cert, _) = make_ee_custom(&ca_cert, &ca_key, &cn, 0, 1, true);

    let handler = peer_certificate_handler();
    let res = handler(&ee_cert.to_pem().unwrap(), &ca_cert.to_pem().unwrap());

    assert!(res.is_err());
    assert!(res.unwrap_err().contains("end-entity certificate basic constraints: CA is true"));
}
