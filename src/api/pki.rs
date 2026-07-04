use crate::blockchain::algo_ops::AlgoOps;
use crate::protocol::ISSUER_SUFFIX;

/// Generate a PKI set from AlgoOps secret:
/// - CA certificate (PEM) signed by Ed25519 key derived from AlgoOps private key
/// - Server certificate + private key (PEM), RSA 2048 signed by CA
/// - Client certificate + private key (PEM), RSA 2048 signed by CA
pub fn generate_pki_from_ops(ops: &AlgoOps) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>), String> {
    use openssl::asn1::Asn1Time;
    use openssl::bn::BigNum;
    use openssl::hash::MessageDigest;
    use openssl::nid::Nid;
    use openssl::pkey::{Id, PKey};
    use openssl::x509::extension::{BasicConstraints, KeyUsage, SubjectKeyIdentifier};
    use openssl::x509::{X509NameBuilder, X509};

    tracing::info!("[generate_pki_from_ops] Generating {:?} from algo", ops.address);

    // 1) Build CA PKey from Algorand private key (ed25519 32 bytes)
    let sk = ops
        .private_key_bytes()
        .map_err(|e| format!("failed to get private key: {e}"))?;
    if sk.len() != 32 {
        return Err("Algorand secret must be 32 bytes".to_string());
    }
    let ca_pkey = PKey::private_key_from_raw_bytes(&sk, Id::ED25519)
        .map_err(|e| format!("failed to construct Ed25519 CA key: {}", e))?;

    // CA subject/issuer name: fixed virtual CA CN and include ORGANIZATIONNAME with Algorand address
    let mut name_builder = X509NameBuilder::new().map_err(|e| format!("name builder: {}", e))?;
    name_builder
        .append_entry_by_nid(Nid::COMMONNAME, crate::protocol::VIRTUAL_CA)
        .map_err(|e| format!("set CN: {}", e))?;
    // Add OrganizationName (O) as the base32 Algorand address from AlgoOps, if available
    let issuer_address = ops
        .address
        .as_ref()
        .ok_or_else(|| "AlgoOps has no address; cannot set CA ORGANIZATIONNAME".to_string())?;
    name_builder
        .append_entry_by_nid(Nid::ORGANIZATIONNAME, issuer_address)
        .map_err(|e| format!("set O: {}", e))?;
    let ca_name = name_builder.build();

    // CA cert builder
    let mut ca_builder = X509::builder().map_err(|e| format!("x509 builder: {}", e))?;
    let mut serial = BigNum::new().map_err(|e| format!("serial: {}", e))?;
    serial
        .rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false)
        .map_err(|e| format!("serial gen: {}", e))?;
    let serial = serial
        .to_asn1_integer()
        .map_err(|e| format!("serial asn1: {}", e))?;
    ca_builder.set_version(2).map_err(|e| format!("set version: {}", e))?;
    ca_builder
        .set_serial_number(&serial)
        .map_err(|e| format!("set serial: {}", e))?;
    ca_builder
        .set_subject_name(&ca_name)
        .map_err(|e| format!("set subject: {}", e))?;
    ca_builder
        .set_issuer_name(&ca_name)
        .map_err(|e| format!("set issuer: {}", e))?;
    ca_builder
        .set_pubkey(&ca_pkey)
        .map_err(|e| format!("set pubkey: {}", e))?;
    let nb = Asn1Time::days_from_now(0).map_err(|e| format!("nb: {}", e))?;
    ca_builder
        .set_not_before(&nb)
        .map_err(|e| format!("nb set: {}", e))?;
    let na = Asn1Time::days_from_now(3650).map_err(|e| format!("na: {}", e))?;
    ca_builder
        .set_not_after(&na)
        .map_err(|e| format!("na set: {}", e))?;
    let bc = BasicConstraints::new().critical().ca().build().map_err(|e| format!("bc: {}", e))?;
    ca_builder
        .append_extension(bc)
        .map_err(|e| format!("append bc: {}", e))?;
    let ku = KeyUsage::new()
        .critical()
        .key_cert_sign()
        .crl_sign()
        .build()
        .map_err(|e| format!("ku: {}", e))?;
    ca_builder
        .append_extension(ku)
        .map_err(|e| format!("append ku: {}", e))?;
    let skid = SubjectKeyIdentifier::new()
        .build(&ca_builder.x509v3_context(None, None))
        .map_err(|e| format!("skid: {}", e))?;
    ca_builder
        .append_extension(skid)
        .map_err(|e| format!("append skid: {}", e))?;
    // Self-signed Ed25519 (md ignored)
    ca_builder
        .sign(&ca_pkey, MessageDigest::null())
        .map_err(|e| format!("sign ca: {}", e))?;
    let ca_cert = ca_builder.build();
    let ca_pem = ca_cert.to_pem().map_err(|e| format!("ca pem: {}", e))?;

    // Helper to create an end-entity RSA certificate signed by CA
    fn make_end_entity(
        issuer_name: &openssl::x509::X509NameRef,
        ca_pkey: &PKey<openssl::pkey::Private>,
        issuer_cert: &X509,
        cn: &str,
    ) -> Result<(X509, PKey<openssl::pkey::Private>), String> {
        use openssl::asn1::Asn1Time;
        use openssl::bn::BigNum;
        use openssl::ec::{EcGroup, EcKey};
        use openssl::hash::MessageDigest;
        use openssl::nid::Nid;
        use openssl::x509::extension::{AuthorityKeyIdentifier, BasicConstraints, ExtendedKeyUsage, KeyUsage, SubjectKeyIdentifier};
        use openssl::x509::{X509NameBuilder, X509};
        // Generate EC (P-256) private key
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).map_err(|e| format!("ec group: {}", e))?;
        let ec_key = EcKey::generate(&group).map_err(|e| format!("ec key gen: {}", e))?;
        let pkey = PKey::from_ec_key(ec_key).map_err(|e| format!("pkey from ec: {}", e))?;
        // Subject name
        let mut nb = X509NameBuilder::new().map_err(|e| format!("name builder: {}", e))?;
        nb.append_entry_by_nid(Nid::COMMONNAME, cn)
            .map_err(|e| format!("set CN: {}", e))?;
        let subj = nb.build();
        // Build cert
        let mut b = X509::builder().map_err(|e| format!("x509 builder: {}", e))?;
        let mut s = BigNum::new().map_err(|e| format!("serial: {}", e))?;
        s.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false)
            .map_err(|e| format!("serial gen: {}", e))?;
        let s = s
            .to_asn1_integer()
            .map_err(|e| format!("serial asn1: {}", e))?;
        b.set_version(2).map_err(|e| format!("set ver: {}", e))?;
        b.set_serial_number(&s).map_err(|e| format!("set serial: {}", e))?;
        b.set_subject_name(&subj).map_err(|e| format!("set subj: {}", e))?;
        b.set_issuer_name(issuer_name)
            .map_err(|e| format!("set issuer: {}", e))?;
        b.set_pubkey(&pkey).map_err(|e| format!("set pubkey: {}", e))?;
        let nb2 = Asn1Time::days_from_now(0).map_err(|e| format!("nb: {}", e))?;
        b.set_not_before(&nb2).map_err(|e| format!("nb set: {}", e))?;
        let na2 = Asn1Time::days_from_now(365).map_err(|e| format!("na: {}", e))?;
        b.set_not_after(&na2).map_err(|e| format!("na set: {}", e))?;
        let bc = BasicConstraints::new().critical().build().map_err(|e| format!("bc: {}", e))?;
        b.append_extension(bc).map_err(|e| format!("append bc: {}", e))?;
        let ku = KeyUsage::new().digital_signature().build().map_err(|e| format!("ku: {}", e))?;
        b.append_extension(ku).map_err(|e| format!("append ku: {}", e))?;
        let eku = ExtendedKeyUsage::new()
            .server_auth()
            .client_auth()
            .build()
            .map_err(|e| format!("eku: {}", e))?;
        b.append_extension(eku).map_err(|e| format!("append eku: {}", e))?;
        let skid = SubjectKeyIdentifier::new()
            .build(&b.x509v3_context(Some(issuer_cert), None))
            .map_err(|e| format!("skid: {}", e))?;
        b.append_extension(skid).map_err(|e| format!("append skid: {}", e))?;
        let akid = AuthorityKeyIdentifier::new()
            .keyid(true)
            .issuer(true)
            .build(&b.x509v3_context(Some(issuer_cert), None))
            .map_err(|e| format!("akid: {}", e))?;
        b.append_extension(akid).map_err(|e| format!("append akid: {}", e))?;
        // Sign with CA key. If CA is Ed25519 (as in our tests), OpenSSL requires MessageDigest::null().
        b.sign(ca_pkey, MessageDigest::null())
            .map_err(|e| format!("sign child: {}", e))?;
        Ok((b.build(), pkey))
    }

    let issuer_name = ca_cert.subject_name();
    let ee_cn = format!("{}{}", if issuer_address.len() > 64 { &issuer_address[..64] } else { issuer_address }, ISSUER_SUFFIX);
    let (server_cert, server_pkey) = make_end_entity(issuer_name, &ca_pkey, &ca_cert, ee_cn.as_str())?;
    let (client_cert, client_pkey) = make_end_entity(issuer_name, &ca_pkey, &ca_cert, ee_cn.as_str())?;

    // PEM outputs
    let server_cert_pem = server_cert
        .to_pem()
        .map_err(|e| format!("server cert pem: {}", e))?;
    let client_cert_pem = client_cert
        .to_pem()
        .map_err(|e| format!("client cert pem: {}", e))?;
    let server_key_pem = server_pkey
        .private_key_to_pem_pkcs8()
        .map_err(|e| format!("server key pem: {}", e))?;
    let client_key_pem = client_pkey
        .private_key_to_pem_pkcs8()
        .map_err(|e| format!("client key pem: {}", e))?;

    Ok((
        ca_pem,
        server_cert_pem,
        server_key_pem,
        client_cert_pem,
        client_key_pem,
    ))
}

