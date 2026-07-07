use bingle_core::protocol::{ISSUER_SUFFIX, VIRTUAL_CA};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn peer_certificate_handler_generates_dump_and_verifies() {
    use openssl::asn1::Asn1Time;
    use openssl::bn::BigNum;
    use openssl::hash::MessageDigest;
    use openssl::nid::Nid;
    use openssl::pkey::{PKey, Private};
    use openssl::x509::extension::{
        AuthorityKeyIdentifier, BasicConstraints, KeyUsage, SubjectKeyIdentifier,
    };
    use openssl::x509::{X509, X509NameBuilder};

    // 1) Create Ed25519 CA key and self-signed certificate with CN = VIRTUAL_CA
    let ca_pkey: PKey<Private> = PKey::generate_ed25519().expect("generate ed25519");

    let mut ca_name_b = X509NameBuilder::new().expect("name builder");
    ca_name_b
        .append_entry_by_nid(Nid::COMMONNAME, VIRTUAL_CA)
        .expect("set CN");
    let ca_name = ca_name_b.build();

    let mut ca_builder = X509::builder().expect("x509 builder");
    let mut serial = BigNum::new().expect("serial");
    serial
        .rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false)
        .expect("serial gen");
    let serial = serial.to_asn1_integer().expect("serial asn1");

    ca_builder.set_version(2).expect("set version");
    ca_builder.set_serial_number(&serial).expect("set serial");
    ca_builder.set_subject_name(&ca_name).expect("set subject");
    ca_builder.set_issuer_name(&ca_name).expect("set issuer");
    ca_builder.set_pubkey(&ca_pkey).expect("set pubkey");
    let nb = Asn1Time::days_from_now(0).expect("nb");
    let na = Asn1Time::days_from_now(365).expect("na");
    ca_builder.set_not_before(&nb).expect("set nb");
    ca_builder.set_not_after(&na).expect("set na");
    let bc = BasicConstraints::new().critical().ca().build().expect("bc");
    ca_builder.append_extension(bc).expect("append bc");
    let skid = SubjectKeyIdentifier::new()
        .build(&ca_builder.x509v3_context(None, None))
        .expect("skid");
    ca_builder.append_extension(skid).expect("append skid");
    ca_builder
        .sign(&ca_pkey, MessageDigest::null())
        .expect("sign ca");
    let ca_cert = ca_builder.build();
    let ca_pem = ca_cert.to_pem().expect("ca pem");

    // 2) Create an end-entity RSA certificate with subject CN that ends with ISSUER_SUFFIX
    // This satisfies the handler's requirement without exposing identity in the CA.
    let mut ee_name_b = X509NameBuilder::new().expect("ee name builder");
    let ee_cn = format!("user{}", ISSUER_SUFFIX);
    ee_name_b
        .append_entry_by_nid(Nid::COMMONNAME, &ee_cn)
        .expect("ee cn");
    let ee_name = ee_name_b.build();

    let rsa = openssl::rsa::Rsa::generate(2048).expect("rsa gen");
    let ee_pkey = PKey::from_rsa(rsa).expect("pkey from rsa");

    let mut ee_builder = X509::builder().expect("ee x509 builder");
    let mut s = BigNum::new().expect("serial");
    s.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false)
        .expect("serial gen");
    let s = s.to_asn1_integer().expect("serial asn1");

    ee_builder.set_version(2).expect("set version");
    ee_builder.set_serial_number(&s).expect("set serial");
    ee_builder.set_subject_name(&ee_name).expect("set subj");
    ee_builder
        .set_issuer_name(ca_cert.subject_name())
        .expect("set issuer");
    ee_builder.set_pubkey(&ee_pkey).expect("set pubkey");
    let nb2 = Asn1Time::days_from_now(0).expect("nb2");
    let na2 = Asn1Time::days_from_now(365).expect("na2");
    ee_builder.set_not_before(&nb2).expect("set nb2");
    ee_builder.set_not_after(&na2).expect("set na2");
    let ee_bc = BasicConstraints::new().critical().build().expect("ee bc");
    ee_builder.append_extension(ee_bc).expect("append ee bc");
    let ku = KeyUsage::new().digital_signature().build().expect("ku");
    ee_builder.append_extension(ku).expect("append ku");
    let skid2 = SubjectKeyIdentifier::new()
        .build(&ee_builder.x509v3_context(Some(&ca_cert), None))
        .expect("skid2");
    ee_builder.append_extension(skid2).expect("append skid2");
    let akid = AuthorityKeyIdentifier::new()
        .keyid(true)
        .issuer(true)
        .build(&ee_builder.x509v3_context(Some(&ca_cert), None))
        .expect("akid");
    ee_builder.append_extension(akid).expect("append akid");

    // Sign the EE cert with the Ed25519 CA key; use MessageDigest::null()
    ee_builder
        .sign(&ca_pkey, MessageDigest::null())
        .expect("sign ee");
    let ee_cert = ee_builder.build();
    let ee_pem = ee_cert.to_pem().expect("ee pem");

    // 3) Call the handler and ensure it returns the EE subject CN (ends with ISSUER_SUFFIX)
    let handler = bingle_core::protocol::cert_verify::peer_certificate_handler();
    let res = handler(&ee_pem, &ca_pem);
    assert!(res.is_ok(), "handler returned error: {:?}", res.err());
    assert_eq!(res.unwrap(), ee_cn.to_string());
}
