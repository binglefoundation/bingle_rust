use crate::dtls::dtls_trait::{HandlePeerCertificate, Result};
use crate::protocol::{ISSUER_SUFFIX, VIRTUAL_CA};

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
        if let Some(ee_subj_cn) = cert
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
        } else {
            println!("[cert_verify][fail] end-entity certificate missing subject CN");
            #[allow(unused)] { crate::util::logging::log_line("[cert_verify][fail] end-entity certificate missing subject CN"); }
            return Err("end-entity certificate missing subject CN".to_string());
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
