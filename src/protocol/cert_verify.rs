use crate::dtls::dtls_trait::{HandlePeerCertificate, Result};
use crate::protocol::ISSUER_SUFFIX;
use crate::blockchain::algo_ops::address_to_byte_key;

#[cfg(not(target_os = "ios"))]
pub fn peer_certificate_handler() -> HandlePeerCertificate {
    fn handler(cert_pem: &[u8], ca_pem: &[u8]) -> Result<String> {
        use openssl::nid::Nid;
        use openssl::pkey::{Id, PKey};
        use openssl::x509::X509;

        // Parse certificates
        let cert = X509::from_pem(cert_pem).map_err(|e| format!("invalid peer certificate PEM: {}", e))?;
        let ca = X509::from_pem(ca_pem).map_err(|e| format!("invalid CA certificate PEM: {}", e))?;

        // Extract CA issuer/subject CN (self-signed)
        let ca_cn = ca
            .subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
            .and_then(|e| e.data().as_utf8().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| "CA certificate missing CN".to_string())?;
        // CN must end with the global suffix
        if !ca_cn.ends_with(ISSUER_SUFFIX) { return Err("CA CN missing issuer suffix".to_string()); }
        let ca_id = ca_cn.trim_end_matches(ISSUER_SUFFIX);

        // Validate CA public key corresponds to Algorand address from CN
        let algo_pk = address_to_byte_key(ca_id).map_err(|e| format!("invalid Algorand address in CA CN: {}", e))?; // 32-byte public key
        let algo_pkey = PKey::public_key_from_raw_bytes(&algo_pk, Id::ED25519).map_err(|e| format!("failed to build Algorand public key: {}", e))?;
        // Compare DER encodings of public keys to ensure same key
        let ca_pub = ca.public_key().map_err(|e| format!("extract CA public key failed: {}", e))?;
        let ca_pub_der = ca_pub.public_key_to_der().map_err(|e| format!("encode CA public key DER failed: {}", e))?;
        let algo_der = algo_pkey.public_key_to_der().map_err(|e| format!("encode Algorand public key DER failed: {}", e))?;
        if ca_pub_der != algo_der { return Err("CA public key does not match Algorand address".to_string()); }
        // Additionally, verify the CA is self-signed correctly
        if !ca.verify(&ca_pub).unwrap_or(false) { return Err("CA self-signature verification failed".to_string()); }

        // Validate that end-entity certificate is issued by CA and signature verifies
        let ee_issuer_cn = cert
            .issuer_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
            .and_then(|e| e.data().as_utf8().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| "end-entity certificate missing issuer CN".to_string())?;
        // The issuer CN must equal the CA CN
        if ee_issuer_cn != ca_cn { return Err("end-entity issuer CN does not match CA CN".to_string()); }
        // Verify signature with CA public key
        if !cert.verify(&ca_pub).unwrap_or(false) { return Err("end-entity certificate signature verification failed".to_string()); }

        // Ensure the end-entity's subject CN is consistent
        if let Some(ee_subj_cn) = cert
            .subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
            .and_then(|e| e.data().as_utf8().ok())
            .map(|s| s.to_string())
        {
            if !ee_subj_cn.ends_with(ISSUER_SUFFIX) { return Err("end-entity CN missing issuer suffix".to_string()); }
            let ee_id = ee_subj_cn.trim_end_matches(ISSUER_SUFFIX);
            if ee_id != ca_id { return Err("end-entity CN id does not match CA id".to_string()); }
        } else {
            return Err("end-entity certificate missing subject CN".to_string());
        }

        // All checks passed: return the issuer (full CN)
        Ok(ca_cn)
    }
    handler
}

#[cfg(target_os = "ios")]
pub fn peer_certificate_handler() -> HandlePeerCertificate {
    fn handler(_cert_pem: &[u8], _ca_pem: &[u8]) -> Result<String> { Err("peer cert handler not available on iOS".to_string()) }
    handler
}
