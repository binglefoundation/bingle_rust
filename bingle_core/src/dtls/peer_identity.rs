//! Who a DTLS peer is: verifying the id a server's certificate names, and checking it against the
//! id a sender expects at that endpoint.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::api::bingle_api::{NetworkEndpoint, NetworkEndpointKey};
use crate::dtls::dtls_trait::{HandlePeerCertificate, Result};

/// Run the peer certificate handler on the server's certificate after the client handshake.
/// Returns the verified id (issuer CN without `ISSUER_SUFFIX`), or `None` if the handler
/// returned an empty issuer. Fails if the server presented no certificate or CA, or the
/// handler rejects them.
pub(crate) fn verify_server_peer_certificate(
    ssl: &openssl::ssl::SslRef,
    handler: HandlePeerCertificate,
    to: &NetworkEndpoint,
) -> Result<Option<String>> {
    let cert_pem = ssl
        .peer_certificate()
        .ok_or_else(|| "no peer certificate presented".to_string())?
        .to_pem()
        .map_err(|e| format!("peer certificate to_pem failed: {}", e))?;
    // The CA is the last element of the presented chain.
    let ca_pem = ssl
        .peer_cert_chain()
        .and_then(|chain| chain.iter().last().map(|ca| ca.to_pem()))
        .ok_or_else(|| "no peer CA certificate in chain".to_string())?
        .map_err(|e| format!("peer CA certificate to_pem failed: {}", e))?;
    let issuer = handler(&cert_pem, &ca_pem).map_err(|e| {
        tracing::warn!(
            "[DtlsOpenSsl::][peer_cert_handler][client/post-connect][{}] rejected: {}",
            to,
            e
        );
        format!("peer certificate rejected: {}", e)
    })?;
    if issuer.is_empty() {
        tracing::warn!(
            "[DtlsOpenSsl::][peer_cert_handler][client/post-connect][{}] empty issuer returned",
            to
        );
        return Ok(None);
    }
    let id = issuer
        .trim_end_matches(crate::protocol::ISSUER_SUFFIX)
        .to_string();
    tracing::debug!(
        "[DtlsOpenSsl::][peer_cert_handler][client/post-connect][{}] issuer={} id={}",
        to,
        issuer,
        id
    );
    Ok(Some(id))
}

/// The id each endpoint's peer must verify as, from `Dtls::expect_peer_id`. Kept apart from the
/// per-peer session state because that is replaced on every reconnect, and a reliable transport
/// retry reconnects without coming back through the caller that set it.
#[derive(Default)]
pub(crate) struct ExpectedPeerIds {
    ids: Mutex<HashMap<NetworkEndpointKey, String>>,
}

impl ExpectedPeerIds {
    /// Expect the peer at `endpoint` to verify as `id`.
    pub(crate) fn expect(&self, endpoint: &NetworkEndpoint, id: &str) {
        let Some(key) = endpoint.get_key() else {
            tracing::warn!(
                "[DtlsOpenSsl:::expect_peer_id] endpoint has no key: {}",
                endpoint
            );
            return;
        };
        if let Ok(mut m) = self.ids.lock() {
            m.insert(key, id.to_string());
        }
    }

    /// Forget every expected id.
    pub(crate) fn clear(&self) {
        if let Ok(mut m) = self.ids.lock() {
            m.clear();
        }
    }

    /// Fail when an id is expected at `key` and `verified` (the session's verified id) isn't it.
    pub(crate) fn check(
        &self,
        key: &NetworkEndpointKey,
        to: &NetworkEndpoint,
        verified: &str,
    ) -> Result<()> {
        let expected = match self.ids.lock() {
            Ok(m) => m.get(key).cloned(),
            Err(_) => return Err("expected peer ids lock poisoned".to_string()),
        };
        match expected {
            Some(id) if id != verified => {
                tracing::warn!(
                    "[DtlsOpenSsl:::send] peer at {} verified as '{}', expected '{}'",
                    to,
                    verified,
                    id
                );
                Err(format!(
                    "dtls peer at {} is not the destination id {}",
                    to, id
                ))
            }
            _ => Ok(()),
        }
    }
}
