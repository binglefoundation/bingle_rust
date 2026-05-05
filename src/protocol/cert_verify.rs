use crate::dtls::dtls_trait::{HandlePeerCertificate, Result};
use crate::protocol::{ISSUER_SUFFIX, VIRTUAL_CA};

pub fn dump_ca_public_key_info(ca_pub: &openssl::pkey::PKey<openssl::pkey::Public>) {
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

    tracing::info!(
        "[cert_verify][dump][ca_pub] sha256(spki)={} len_der={} bytes",
        ca_pub_der_fp,
        ca_pub_der.len()
    );

    // Print raw public key hex dump in a certificate-like style
    tracing::info!(
        "[cert_verify][dump][ca_pub] RAW PUBLIC KEY (len={}):",
        raw_len
    );
    tracing::info!("[cert_verify][dump][ca_pub] pub:");
    for l in &raw_hex_lines {
        tracing::info!("[cert_verify][dump][ca_pub]     {}", l);
    }

    tracing::info!(
        "[cert_verify][dump][ca_pub] BEGIN PUBLIC KEY\n{}[cert_verify][dump][ca_pub] END PUBLIC KEY",
        ca_pub_pem
    );
}

pub fn peer_certificate_handler() -> HandlePeerCertificate {
    fn handler(cert_pem: &[u8], ca_pem: &[u8]) -> Result<String> {
        use openssl::nid::Nid;
        use openssl::pkey::Id;
        use openssl::x509::X509;

        // Entry log
        tracing::info!("[cert_verify] peer_certificate_handler called: cert_len={}, ca_len={}", cert_pem.len(), ca_pem.len());
        
        // Small helper to log a failure reason and return Err(msg) consistently.
        #[inline]
        fn log_fail<M: Into<String>>(msg: M) -> Result<String> {
            let s = msg.into();
            tracing::info!("[cert_verify][fail] {}", s);
            Err(s)
        }
        
        // Parse certificates with explicit error logging
        let cert = match X509::from_pem(cert_pem) {
            Ok(c) => c,
            Err(e) => {
                return log_fail(format!("invalid peer certificate PEM: {}", e));
            }
        };
        let ca = match X509::from_pem(ca_pem) {
            Ok(c) => c,
            Err(e) => {
                return log_fail(format!("invalid CA certificate PEM: {}", e));
            }
        };

        // Dumps of certificates for diagnostics (emit PEM)
        dump_cert_info("peer", &cert, true);
        dump_cert_info("ca", &ca, true);

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
                return log_fail("CA certificate missing CN");
            }
        };
        // CA CN must be our virtual CA value
        if ca_cn != VIRTUAL_CA {
            return log_fail(format!("unexpected CA CN: '{}' (expected '{}')", ca_cn, VIRTUAL_CA));
        }

        // Validate the CA public key exists and is Ed25519, and CA is self-signed
        let ca_pub = match ca.public_key() {
            Ok(p) => p,
            Err(e) => {
                return log_fail(format!("extract CA public key failed: {}", e));
            }
        };
        if ca_pub.id() != Id::ED25519 {
            return log_fail(format!("CA public key is not Ed25519 (found {:?})", ca_pub.id()));
        }

        // Human-readable dump of the CA public key for diagnostics
        dump_ca_public_key_info(&ca_pub);

        if !ca.verify(&ca_pub).unwrap_or(false) {
            return log_fail("CA self-signature verification failed".to_string());
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
                return log_fail("end-entity certificate missing issuer CN");
            }
        };
        if ee_issuer_cn != VIRTUAL_CA {
            return log_fail(format!("end-entity issuer CN does not equal virtual CA: ee='{}', expected='{}'", ee_issuer_cn, VIRTUAL_CA));
        }
        if !cert.verify(&ca_pub).unwrap_or(false) {
            tracing::info!("[cert_verify][fail] verification failed with cert/PK/ca cert");

            return log_fail("end-entity certificate signature verification failed".to_string());
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
                return log_fail(format!("end-entity CN missing issuer suffix: {}", ee_subj_cn));
            }
            ee_subj_cn
        } else {
            return log_fail("end-entity certificate missing subject CN".to_string());
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
                return log_fail(format!("CA O (org) does not match EE subject CN without suffix: ca.O='{}' expected='{}'", ca_org, expected_addr));
            }
        } else {
            // Tolerate missing O for backward compatibility; log for diagnostics
            tracing::info!("[cert_verify][warn] CA certificate has no OrganizationName (O); skipping address match");
        }

        // All checks passed: return the end-entity subject CN (ee_subj_cn), not the CA's CN
        tracing::info!("[cert_verify][ok] peer_certificate_handler success: subjectCN={}", ee_subj_cn);
        #[allow(unused)] {  }
        Ok(ee_subj_cn)
    }
    handler
}

pub fn peer_certificate_accept_all_handler() -> HandlePeerCertificate {
    fn handler(cert_pem: &[u8], _ca_pem: &[u8]) -> Result<String> {
        tracing::info!("[cert_verify] accept_all handler called: cert_len={}", cert_pem.len());
        #[allow(unused)] {  }
        Ok("accept-all".to_string())
    }
    handler
}


pub fn dump_cert_info(tag: &str, cert: &openssl::x509::X509, use_pem: bool) {
    use openssl::hash::MessageDigest;

    let cert_fp = cert
        .digest(MessageDigest::sha256())
        .ok()
        .map(|b| data_encoding::HEXLOWER.encode(&b))
        .unwrap_or_else(|| "<digest failed>".to_string());
    let der_len = cert.to_der().map(|v| v.len()).unwrap_or(0);
    tracing::info!(
        "[cert_verify][dump][{}] sha256={} len={} bytes",
        tag, cert_fp, der_len
    );

    if use_pem {
        let pem: String = cert
            .to_pem()
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .unwrap_or_else(|_| "<x509 to_pem failed>".to_string());
        // Already includes proper -----BEGIN/END CERTIFICATE----- markers
        tracing::info!("[cert_verify][dump][{}] PEM\n{}", tag, pem);
    } else {
        let cert_text: String = cert
            .to_text()
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .unwrap_or_else(|_| "<x509 to_text failed>".to_string());
        tracing::info!(
            "[cert_verify][dump][{}] BEGIN CERT\n{}\n[cert_verify][dump][{}] END CERT",
            tag, cert_text, tag
        );
    }
}
