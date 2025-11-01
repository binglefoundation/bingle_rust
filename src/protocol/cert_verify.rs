use crate::dtls::dtls_trait::{HandlePeerCertificate, Result};
use crate::protocol::{ISSUER_SUFFIX, VIRTUAL_CA};

#[cfg(not(target_os = "ios"))]
pub fn dump_ca_public_key_debug(ca_pub: &openssl::pkey::PKey<openssl::pkey::Public>) {
    use openssl::hash::MessageDigest;
    // Dump PEM of the public key and a SHA-256 fingerprint over its DER (SPKI)
    let ca_pub_pem: String = ca_pub
        .public_key_to_pem()
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .unwrap_or_else(|_| "<public_key_to_pem failed>".to_string());
    let ca_pub_der: Vec<u8> = ca_pub.public_key_to_der().unwrap_or_else(|_| Vec::new());
    let ca_pub_der_fp = if ca_pub_der.is_empty() {
        "<der failed>".to_string()
    } else {
        // Compute sha256 over DER (SPKI)
        let mut hasher = openssl::hash::Hasher::new(MessageDigest::sha256()).ok();
        if let Some(ref mut h) = hasher {
            let _ = h.update(&ca_pub_der);
            match h.finish() {
                Ok(d) => data_encoding::HEXLOWER.encode(&d),
                Err(_) => "<digest failed>".to_string(),
            }
        } else {
            "<digest failed>".to_string()
        }
    };

    // Attempt to extract the raw public key bytes (e.g., Ed25519 32-byte key) and render a hex dump
    let mut raw_hex_lines: Vec<String> = Vec::new();
    let mut raw_len: usize = 0;
    match ca_pub.raw_public_key() {
        Ok(raw) => {
            raw_len = raw.len();
            // Format like certificate dumps: colon-separated hex, 16 bytes per line
            for (i, chunk) in raw.chunks(16).enumerate() {
                let mut line = String::new();
                // Optional offset label similar to OpenSSL style
                let _ = i; // silence unused if format changes
                for (j, b) in chunk.iter().enumerate() {
                    if j > 0 { line.push(':'); }
                    line.push_str(&format!("{:02x}", b));
                }
                raw_hex_lines.push(line);
            }
        }
        Err(_) => {
            raw_hex_lines.push("<raw public key not available>".to_string());
        }
    }

    println!(
        "[cert_verify][dump][ca_pub] sha256(spki)={} len_der={} bytes",
        ca_pub_der_fp,
        ca_pub_der.len()
    );

    // Print raw public key hex dump in a certificate-like style
    println!(
        "[cert_verify][dump][ca_pub] RAW PUBLIC KEY (len={}):",
        raw_len
    );
    println!("[cert_verify][dump][ca_pub] pub:");
    for l in &raw_hex_lines {
        println!("[cert_verify][dump][ca_pub]     {}", l);
    }

    println!(
        "[cert_verify][dump][ca_pub] BEGIN PUBLIC KEY\n{}[cert_verify][dump][ca_pub] END PUBLIC KEY",
        ca_pub_pem
    );
    #[allow(unused)] {
        crate::util::logging::log_line(&format!(
            "[cert_verify][dump][ca_pub] sha256(spki)={} len_der={} bytes",
            ca_pub_der_fp,
            ca_pub_der.len()
        ));
        crate::util::logging::log_line(&format!(
            "[cert_verify][dump][ca_pub] RAW PUBLIC KEY (len={}):",
            raw_len
        ));
        crate::util::logging::log_line("[cert_verify][dump][ca_pub] pub:");
        for l in &raw_hex_lines {
            crate::util::logging::log_line(&format!("[cert_verify][dump][ca_pub]     {}", l));
        }
        crate::util::logging::log_line("[cert_verify][dump][ca_pub] BEGIN PUBLIC KEY");
        crate::util::logging::log_line(&ca_pub_pem);
        crate::util::logging::log_line("[cert_verify][dump][ca_pub] END PUBLIC KEY");
    }
}

#[cfg(not(target_os = "ios"))]
pub fn peer_certificate_handler() -> HandlePeerCertificate {
    fn handler(cert_pem: &[u8], ca_pem: &[u8]) -> Result<String> {
        use openssl::nid::Nid;
        use openssl::pkey::Id;
        use openssl::x509::X509;

        // Entry log
        println!("[cert_verify] peer_certificate_handler called: cert_len={}, ca_len={}", cert_pem.len(), ca_pem.len());
        #[allow(unused)] { crate::util::logging::log_line(&format!("[cert_verify] peer_certificate_handler called: cert_len={}, ca_len={}", cert_pem.len(), ca_pem.len())); }

        // Parse certificates with explicit error logging
        let cert = match X509::from_pem(cert_pem) {
            Ok(c) => c,
            Err(e) => {
                println!("[cert_verify][fail] invalid peer certificate PEM: {}", e);
                #[allow(unused)] { crate::util::logging::log_line(&format!("[cert_verify][fail] invalid peer certificate PEM: {}", e)); }
                return Err(format!("invalid peer certificate PEM: {}", e));
            }
        };
        let ca = match X509::from_pem(ca_pem) {
            Ok(c) => c,
            Err(e) => {
                println!("[cert_verify][fail] invalid CA certificate PEM: {}", e);
                #[allow(unused)] { crate::util::logging::log_line(&format!("[cert_verify][fail] invalid CA certificate PEM: {}", e)); }
                return Err(format!("invalid CA certificate PEM: {}", e));
            }
        };

        // Human-readable dumps of certificates for diagnostics
        dump_cert_debug("peer", &cert);
        dump_cert_debug("ca", &ca);

        // Extract CA issuer/subject CN (self-signed)
        let ca_cn = match ca
            .subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
            .and_then(|e| e.data().as_utf8().ok())
            .map(|s| s.to_string())
        {
            Some(s) => s,
            None => {
                println!("[cert_verify][fail] CA certificate missing CN");
                #[allow(unused)] { crate::util::logging::log_line("[cert_verify][fail] CA certificate missing CN"); }
                return Err("CA certificate missing CN".to_string());
            }
        };
        // CA CN must be our virtual CA value
        if ca_cn != VIRTUAL_CA {
            println!("[cert_verify][fail] unexpected CA CN: '{}' (expected '{}')", ca_cn, VIRTUAL_CA);
            #[allow(unused)] { crate::util::logging::log_line(&format!("[cert_verify][fail] unexpected CA CN: '{}' (expected '{}')", ca_cn, VIRTUAL_CA)); }
            return Err("unexpected CA CN".to_string());
        }

        // Validate the CA public key exists and is Ed25519, and CA is self-signed
        let ca_pub = match ca.public_key() {
            Ok(p) => p,
            Err(e) => {
                println!("[cert_verify][fail] extract CA public key failed: {}", e);
                #[allow(unused)] { crate::util::logging::log_line(&format!("[cert_verify][fail] extract CA public key failed: {}", e)); }
                return Err(format!("extract CA public key failed: {}", e));
            }
        };
        if ca_pub.id() != Id::ED25519 {
            println!("[cert_verify][fail] CA public key is not Ed25519 (found {:?})", ca_pub.id());
            #[allow(unused)] { crate::util::logging::log_line("[cert_verify][fail] CA public key is not Ed25519"); }
            return Err("CA public key is not Ed25519".to_string());
        }

        // Human-readable dump of the CA public key for diagnostics
        dump_ca_public_key_debug(&ca_pub);

        if !ca.verify(&ca_pub).unwrap_or(false) {
            println!("[cert_verify][fail] CA self-signature verification failed");
            #[allow(unused)] { crate::util::logging::log_line("[cert_verify][fail] CA self-signature verification failed"); }
            return Err("CA self-signature verification failed".to_string());
        }

        // Validate that end-entity certificate is issued by CA and signature verifies
        let ee_issuer_cn = match cert
            .issuer_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
            .and_then(|e| e.data().as_utf8().ok())
            .map(|s| s.to_string())
        {
            Some(s) => s,
            None => {
                println!("[cert_verify][fail] end-entity certificate missing issuer CN");
                #[allow(unused)] { crate::util::logging::log_line("[cert_verify][fail] end-entity certificate missing issuer CN"); }
                return Err("end-entity certificate missing issuer CN".to_string());
            }
        };
        if ee_issuer_cn != VIRTUAL_CA {
            println!("[cert_verify][fail] end-entity issuer CN does not equal virtual CA: ee='{}', expected='{}'", ee_issuer_cn, VIRTUAL_CA);
            #[allow(unused)] { crate::util::logging::log_line(&format!("[cert_verify][fail] end-entity issuer CN does not equal virtual CA: ee='{}', expected='{}'", ee_issuer_cn, VIRTUAL_CA)); }
            return Err("end-entity issuer CN mismatch".to_string());
        }
        if !cert.verify(&ca_pub).unwrap_or(false) {
            println!("[cert_verify][fail] end-entity certificate signature verification failed");
            #[allow(unused)] { crate::util::logging::log_line("[cert_verify][fail] end-entity certificate signature verification failed"); }
            return Err("end-entity certificate signature verification failed".to_string());
        }

        // Ensure the end-entity's subject CN is consistent (ends with issuer suffix)
        let ee_subj_cn = if let Some(ee_subj_cn) = cert
            .subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
            .and_then(|e| e.data().as_utf8().ok())
            .map(|s| s.to_string())
        {
            if !ee_subj_cn.ends_with(ISSUER_SUFFIX) {
                println!("[cert_verify][fail] end-entity CN missing issuer suffix: {}", ee_subj_cn);
                #[allow(unused)] { crate::util::logging::log_line(&format!("[cert_verify][fail] end-entity CN missing issuer suffix: {}", ee_subj_cn)); }
                return Err("end-entity CN missing issuer suffix".to_string());
            }
            ee_subj_cn
        } else {
            println!("[cert_verify][fail] end-entity certificate missing subject CN");
            #[allow(unused)] { crate::util::logging::log_line("[cert_verify][fail] end-entity certificate missing subject CN"); }
            return Err("end-entity certificate missing subject CN".to_string());
        };

        // If the CA includes an OrganizationName (O), validate it matches the end-entity subject CN with the issuer suffix removed.
        let expected_addr = ee_subj_cn.trim_end_matches(ISSUER_SUFFIX).to_string();
        let ca_org_opt: Option<String> = ca
            .subject_name()
            .entries_by_nid(Nid::ORGANIZATIONNAME)
            .next()
            .and_then(|e| e.data().as_utf8().ok())
            .map(|s| s.to_string());
        if let Some(ca_org) = ca_org_opt {
            if ca_org != expected_addr {
                println!("[cert_verify][fail] CA O (org) does not match EE subject CN without suffix: ca.O='{}' expected='{}'", ca_org, expected_addr);
                #[allow(unused)] { crate::util::logging::log_line(&format!("[cert_verify][fail] CA O (org) does not match EE subject CN without suffix: ca.O='{}' expected='{}'", ca_org, expected_addr)); }
                return Err("CA organization does not match end-entity subject".to_string());
            }
        } else {
            // Tolerate missing O for backward compatibility; log for diagnostics
            println!("[cert_verify][warn] CA certificate has no OrganizationName (O); skipping address match");
            #[allow(unused)] { crate::util::logging::log_line("[cert_verify][warn] CA certificate has no OrganizationName (O); skipping address match"); }
        }

        // All checks passed: return the issuer (full CN)
        println!("[cert_verify][ok] peer_certificate_handler success: issuer={}", ca_cn);
        #[allow(unused)] { crate::util::logging::log_line(&format!("[cert_verify][ok] peer_certificate_handler success: issuer={}", ca_cn)); }
        Ok(ca_cn)
    }
    handler
}

#[cfg(not(target_os = "ios"))]
pub fn peer_certificate_accept_all_handler() -> HandlePeerCertificate {
    fn handler(cert_pem: &[u8], _ca_pem: &[u8]) -> Result<String> {
        println!("[cert_verify] accept_all handler called: cert_len={}", cert_pem.len());
        #[allow(unused)] { crate::util::logging::log_line(&format!("[cert_verify] accept_all handler called: cert_len={}", cert_pem.len())); }
        Ok("accept-all".to_string())
    }
    handler
}

#[cfg(target_os = "ios")]
pub fn peer_certificate_handler() -> HandlePeerCertificate {
    fn handler(_cert_pem: &[u8], _ca_pem: &[u8]) -> Result<String> { Err("peer cert handler not available on iOS".to_string()) }
    handler
}

#[cfg(target_os = "ios")]
pub fn peer_certificate_accept_all_handler() -> HandlePeerCertificate {
    fn handler(_cert_pem: &[u8], _ca_pem: &[u8]) -> Result<String> { Ok("accept-all".to_string()) }
    handler
}


#[cfg(not(target_os = "ios"))]
pub fn dump_cert_debug(tag: &str, cert: &openssl::x509::X509) {
    use openssl::hash::MessageDigest;
    let cert_text: String = cert
        .to_text()
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .unwrap_or_else(|_| "<x509 to_text failed>".to_string());
    let cert_fp = cert
        .digest(MessageDigest::sha256())
        .ok()
        .map(|b| data_encoding::HEXLOWER.encode(&b))
        .unwrap_or_else(|| "<digest failed>".to_string());
    let der_len = cert.to_der().map(|v| v.len()).unwrap_or(0);
    println!(
        "[cert_verify][dump][{}] sha256={} len={} bytes",
        tag, cert_fp, der_len
    );
    println!(
        "[cert_verify][dump][{}] BEGIN CERT\n{}\n[cert_verify][dump][{}] END CERT",
        tag, cert_text, tag
    );
    #[allow(unused)] {
        crate::util::logging::log_line(&format!(
            "[cert_verify][dump][{}] sha256={} len={} bytes",
            tag, cert_fp, der_len
        ));
        crate::util::logging::log_line(&format!("[cert_verify][dump][{}] BEGIN CERT", tag));
        crate::util::logging::log_line(&cert_text);
        crate::util::logging::log_line(&format!("[cert_verify][dump][{}] END CERT", tag));
    }
}
