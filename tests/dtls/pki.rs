use openssl::asn1::Asn1Time;
use openssl::bn::BigNum;
use openssl::ec::{EcGroup, EcKey};
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::{PKey, Private};
use openssl::x509::extension::{AuthorityKeyIdentifier, BasicConstraints, ExtendedKeyUsage, KeyUsage, SubjectKeyIdentifier};
use openssl::x509::{X509NameBuilder, X509};
use crate::util::test_util::ADDRESS_RECEIVE;

#[allow(dead_code)]
pub struct TestCerts {
    pub ca_crt: Vec<u8>,
    pub server_crt: Vec<u8>,
    pub server_key: Vec<u8>,
    #[allow(dead_code)]
    pub client_crt: Vec<u8>,
    #[allow(dead_code)]
    pub client_key: Vec<u8>,
}

#[allow(dead_code)]
pub fn
generate_ed25519_test_certs() -> TestCerts {
    generate_ed25519_test_certs_with_key(ADDRESS_RECEIVE.to_string().as_str())
}

#[allow(dead_code)]
fn make_serial() -> BigNum {
    let mut serial = BigNum::new().expect("bignum");
    serial.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false).expect("rand");
    serial
}

#[allow(dead_code)]
fn make_ca(key: &str) -> (X509, PKey<Private>) {
    let pkey = PKey::generate_ed25519().expect("ed25519 gen");
    let mut name = X509NameBuilder::new().expect("name builder");
    name.append_entry_by_nid(Nid::COMMONNAME, "virtual.bingle.home.arpa").expect("cn");
    name.append_entry_by_nid(Nid::ORGANIZATIONNAME, key).expect("o");
    let name = name.build();

    let mut builder = X509::builder().expect("x509 builder");
    builder.set_version(2).expect("version");
    builder.set_serial_number(&make_serial().to_asn1_integer().expect("asn1 serial")).expect("serial");
    builder.set_subject_name(&name).expect("subject");
    builder.set_issuer_name(&name).expect("issuer");
    builder.set_pubkey(&pkey).expect("pubkey");

    let not_before = Asn1Time::days_from_now(0).expect("not before");
    builder.set_not_before(&not_before).expect("set not before");
    let not_after = Asn1Time::days_from_now(2).expect("not after");
    builder.set_not_after(&not_after).expect("set not after");

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

    builder.sign(&pkey, MessageDigest::null()).expect("sign");
    (builder.build(), pkey)
}

#[allow(dead_code)]
fn make_ee(ca_cert: &X509, ca_key: &PKey<Private>, cn: &str) -> (X509, PKey<Private>) {
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

    let not_before = Asn1Time::days_from_now(0).expect("not before");
    builder.set_not_before(&not_before).expect("set not before");
    let not_after = Asn1Time::days_from_now(2).expect("not after");
    builder.set_not_after(&not_after).expect("set not after");

    let bc = BasicConstraints::new().critical().build().expect("bc");
    builder.append_extension(bc).expect("append bc");
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

#[allow(dead_code)]
pub fn generate_ed25519_test_certs_with_key(key: &str) -> TestCerts {
    let (ca_cert, ca_key) = make_ca(key);
    let cn = format!("{}.", key);
    let (server_cert, server_key) = make_ee(&ca_cert, &ca_key, &cn);
    let (client_cert, client_key) = make_ee(&ca_cert, &ca_key, &cn);

    TestCerts {
        ca_crt: ca_cert.to_pem().expect("ca pem"),
        server_crt: server_cert.to_pem().expect("server pem"),
        server_key: server_key.private_key_to_pem_pkcs8().expect("server key pem"),
        client_crt: client_cert.to_pem().expect("client pem"),
        client_key: client_key.private_key_to_pem_pkcs8().expect("client key pem"),
    }
}
