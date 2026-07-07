use base64::{Engine as _, engine::general_purpose};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt;
use std::net::SocketAddr;
use std::str::FromStr;

/// InetSocketAddress as defined in BINGLE_SPEC.md
/// Hostname/IP and UDP port number.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InetSocketAddress {
    pub host: String,
    pub port: u16,
}

impl From<SocketAddr> for InetSocketAddress {
    fn from(addr: SocketAddr) -> Self {
        Self {
            host: addr.ip().to_string(),
            port: addr.port(),
        }
    }
}

impl TryFrom<InetSocketAddress> for SocketAddr {
    type Error = String;
    fn try_from(val: InetSocketAddress) -> Result<Self, Self::Error> {
        let addr: SocketAddr = format!("{}:{}", val.host, val.port)
            .parse()
            .map_err(|e| format!("{}", e))?;
        if addr.is_ipv6() {
            return Err("IPv6 addresses are not supported".to_string());
        }
        Ok(addr)
    }
}

impl fmt::Display for InetSocketAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

impl FromStr for InetSocketAddress {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let addr: SocketAddr = s.parse().map_err(|e| format!("{}", e))?;
        if addr.is_ipv6() {
            return Err("IPv6 addresses are not supported".to_string());
        }
        Ok(Self::from(addr))
    }
}

/// Advertisement record for a node (DDB AdvertRecord)
/// See generated/BINGLE_SPEC.md
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdvertRecord {
    /// Identifier of the node (Algorand address)
    pub id: String,
    /// Optional network endpoint (IP/hostname and port) for direct DTLS
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<InetSocketAddress>,
    /// True if this node provides relay services
    #[serde(rename = "amRelay")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub am_relay: Option<bool>,
    /// Optional identifier of the relay this node uses
    #[serde(rename = "relayId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_id: Option<String>,
    /// Optional signature from the relay verifying this record
    #[serde(rename = "relaySig")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_sig: Option<String>,
    /// Record creation or update timestamp (RFC 3339 date-time)
    pub date: String,
    /// Optional signature of this record by the node
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
}

impl AdvertRecord {
    /// Create a new signed AdvertRecord.
    /// Signs over all fields except sig using ed25519/sha-512.
    pub fn new(
        id: String,
        endpoint: Option<InetSocketAddress>,
        am_relay: Option<bool>,
        relay_id: Option<String>,
        relay_sig: Option<String>,
        date: String,
        signing_key: &SigningKey,
    ) -> Self {
        let mut rec = Self {
            id,
            endpoint,
            am_relay,
            relay_id,
            relay_sig,
            date,
            sig: None,
        };
        rec.sig = Some(rec.calculate_signature(signing_key));
        rec
    }

    /// Create a new unsigned AdvertRecord.
    pub fn new_unsigned(
        id: String,
        endpoint: Option<InetSocketAddress>,
        am_relay: Option<bool>,
        relay_id: Option<String>,
        relay_sig: Option<String>,
        date: String,
    ) -> Self {
        Self {
            id,
            endpoint,
            am_relay,
            relay_id,
            relay_sig,
            date,
            sig: None,
        }
    }

    /// Calculate the signature over all fields except sig.
    pub fn calculate_signature(&self, signing_key: &SigningKey) -> String {
        let mut copy = self.clone();
        copy.sig = None;
        // Use serde_json::to_vec for deterministic serialization of the struct
        let data = serde_json::to_vec(&copy).expect("Failed to serialize AdvertRecord for signing");
        let signature = signing_key.sign(&data);
        general_purpose::STANDARD.encode(signature.to_bytes())
    }

    /// Verify the signature of this record.
    /// The public key is derived from the id (Algorand address).
    pub fn verify(&self) -> bool {
        let sig_str = match &self.sig {
            Some(s) => s,
            None => return false,
        };

        let pk_bytes = match crate::blockchain::algo_ops::address_to_byte_key(&self.id) {
            Ok(pk) => pk,
            Err(_) => return false,
        };

        let vk = match VerifyingKey::from_bytes(&pk_bytes) {
            Ok(vk) => vk,
            Err(_) => return false,
        };

        let sig_bytes = match general_purpose::STANDARD.decode(sig_str) {
            Ok(b) => b,
            Err(_) => return false,
        };

        let sig_arr: [u8; 64] = match sig_bytes.try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let sig = Signature::from_bytes(&sig_arr);

        let mut copy = self.clone();
        copy.sig = None;
        let data = match serde_json::to_vec(&copy) {
            Ok(d) => d,
            Err(_) => return false,
        };

        vk.verify(&data, &sig).is_ok()
    }

    /// Serialize to a compact CSV format for blockchain storage.
    /// Order: endpoint, am_relay, relay_id, relay_sig, date, sig
    /// id is omitted as it is implied by the storage context.
    pub fn serialize_csv(&self) -> String {
        let endpoint_str = self
            .endpoint
            .as_ref()
            .map(|e| e.to_string())
            .unwrap_or_default();
        let am_relay_str = match self.am_relay {
            Some(true) => "T",
            _ => "F",
        };
        let relay_id_str = self.relay_id.as_deref().unwrap_or_default();
        let relay_sig_str = self.relay_sig.as_deref().unwrap_or_default();
        let date_str = &self.date;
        let sig_str = self.sig.as_deref().unwrap_or_default();

        format!(
            "{},{},{},{},{},{}",
            endpoint_str, am_relay_str, relay_id_str, relay_sig_str, date_str, sig_str
        )
    }

    /// Deserialize from the compact CSV format.
    /// Requires the id to be provided externally.
    pub fn deserialize_csv(id: String, csv: &str) -> Option<Self> {
        let parts: Vec<&str> = csv.split(',').collect();
        if parts.len() != 6 {
            return None;
        }

        let endpoint = if parts[0].is_empty() {
            None
        } else {
            InetSocketAddress::from_str(parts[0]).ok()
        };

        let am_relay = match parts[1] {
            "T" => Some(true),
            "F" => Some(false),
            _ => return None,
        };

        let relay_id = if parts[2].is_empty() {
            None
        } else {
            Some(parts[2].to_string())
        };
        let relay_sig = if parts[3].is_empty() {
            None
        } else {
            Some(parts[3].to_string())
        };
        let date = parts[4].to_string();
        let sig = if parts[5].is_empty() {
            None
        } else {
            Some(parts[5].to_string())
        };

        Some(Self {
            id,
            endpoint,
            am_relay,
            relay_id,
            relay_sig,
            date,
            sig,
        })
    }
}
