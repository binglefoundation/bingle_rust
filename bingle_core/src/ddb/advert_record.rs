use base64::{Engine as _, engine::general_purpose};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::str::FromStr;

/// InetSocketAddress as defined in BINGLE_SPEC.md
/// Hostname/IP and UDP port number.
///
/// `host` holds a bare IP address: IPv4 dotted-quad, or IPv6 without brackets (issue #493).
/// Brackets around an IPv6 host are tolerated on input. The text form (`Display`/`FromStr`) is
/// `a.b.c.d:port` for IPv4 and `[addr]:port` for IPv6.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InetSocketAddress {
    pub host: String,
    pub port: u16,
}

/// Reduce an IPv4-mapped IPv6 address (`::ffff:a.b.c.d`) to the IPv4 address it carries, so a
/// dual-stack socket's view of an IPv4 peer and the peer's own IPv4 view compare equal.
pub fn canonical_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        IpAddr::V4(_) => ip,
    }
}

/// Parse an `InetSocketAddress` host: an IP address, with optional brackets around an IPv6
/// address. Not canonicalised; see [`canonical_ip`].
fn parse_host_ip(host: &str) -> Option<IpAddr> {
    let bare = match host.strip_prefix('[') {
        Some(rest) => rest.strip_suffix(']')?,
        None => host,
    };
    let ip: IpAddr = bare.parse().ok()?;
    // Brackets are only meaningful around an IPv6 address.
    if bare.len() != host.len() && ip.is_ipv4() {
        return None;
    }
    Some(ip)
}

/// True when `ip` is an IPv6 address a node may advertise as `endpoint6` and a peer may use
/// (issue #493): global unicast (`2000::/3`), which excludes link-local (`fe80::/10`), unique
/// local (`fc00::/7`), multicast (`ff00::/8`), IPv4-mapped (`::ffff:0:0/96`) and the unspecified
/// address. Loopback (`::1`) is accepted so the loopback test topologies can exercise the path;
/// emission (bingle_rust #499) refuses it separately.
pub fn is_advertisable_ipv6(ip: &Ipv6Addr) -> bool {
    if ip.is_loopback() {
        return true;
    }
    (ip.segments()[0] & 0xe000) == 0x2000
}

impl InetSocketAddress {
    /// The host as an IP address, IPv4-mapped IPv6 reduced to IPv4, or `None` when the host is
    /// not an IP address.
    pub fn ip(&self) -> Option<IpAddr> {
        parse_host_ip(&self.host).map(canonical_ip)
    }
}

impl From<SocketAddr> for InetSocketAddress {
    fn from(addr: SocketAddr) -> Self {
        Self {
            host: canonical_ip(addr.ip()).to_string(),
            port: addr.port(),
        }
    }
}

impl TryFrom<InetSocketAddress> for SocketAddr {
    type Error = String;
    fn try_from(val: InetSocketAddress) -> Result<Self, Self::Error> {
        match val.ip() {
            Some(ip) => Ok(SocketAddr::new(ip, val.port)),
            None => Err(format!("invalid host '{}': not an IP address", val.host)),
        }
    }
}

impl fmt::Display for InetSocketAddress {
    /// Formats the host verbatim, adding brackets around an IPv6 host so the port can be split
    /// off again. Verbatim (not canonicalised) so the text form round-trips the signed host.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') && !self.host.starts_with('[') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

impl FromStr for InetSocketAddress {
    type Err = String;
    /// Parses `a.b.c.d:port`, `[v6]:port`, and the unbracketed `v6:port` form (the port is the
    /// text after the last colon). IPv4-mapped IPv6 is reduced to IPv4; any other host keeps
    /// its text, minus brackets.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (host, port) = s
            .rsplit_once(':')
            .ok_or_else(|| format!("invalid socket address '{}': no port", s))?;
        let port: u16 = port
            .parse()
            .map_err(|e| format!("invalid port in '{}': {}", s, e))?;
        let ip = parse_host_ip(host)
            .ok_or_else(|| format!("invalid host in '{}': not an IP address", s))?;
        let canonical = canonical_ip(ip);
        let host = if canonical != ip {
            canonical.to_string()
        } else {
            host.trim_start_matches('[')
                .trim_end_matches(']')
                .to_string()
        };
        Ok(Self { host, port })
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
    /// Optional IPv6 endpoint for direct DTLS (issue #493). Kept and re-served verbatim by
    /// relays, and covered by the signature only when present, so a record without it
    /// serialises, signs and verifies exactly as before the field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint6: Option<InetSocketAddress>,
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
            endpoint6: None,
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
            endpoint6: None,
            am_relay,
            relay_id,
            relay_sig,
            date,
            sig: None,
        }
    }

    /// Set the optional IPv6 endpoint (issue #493). Clears any signature, since the signature
    /// covers `endpoint6`; sign again with [`AdvertRecord::signed`].
    pub fn with_endpoint6(mut self, endpoint6: Option<InetSocketAddress>) -> Self {
        self.endpoint6 = endpoint6;
        self.sig = None;
        self
    }

    /// Sign this record (over all fields except sig) and return it.
    pub fn signed(mut self, signing_key: &SigningKey) -> Self {
        self.sig = Some(self.calculate_signature(signing_key));
        self
    }

    /// The IPv6 endpoint as an address a peer may use, or `None` when it is absent or not usable:
    /// not an IPv6 address, or not one [`is_advertisable_ipv6`] accepts. Relays store and re-serve
    /// a record whatever its `endpoint6` holds (the signature covers the field, so it can't be
    /// stripped, and refusing the record would also lose its IPv4 endpoint); every consumer that
    /// would send to `endpoint6` must read it through this method instead.
    pub fn usable_endpoint6(&self) -> Option<SocketAddr> {
        let addr = SocketAddr::try_from(self.endpoint6.clone()?).ok()?;
        match addr.ip() {
            IpAddr::V6(v6) if is_advertisable_ipv6(&v6) => Some(addr),
            _ => None,
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

        let pk_bytes = match algo_ops::address_to_byte_key(&self.id) {
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
    /// Order: endpoint, am_relay, relay_id, relay_sig, date, sig[, endpoint6]
    /// id is omitted as it is implied by the storage context. The seventh field, endpoint6, is
    /// written only when present, so a record without it has the original six-field form.
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

        let base = format!(
            "{},{},{},{},{},{}",
            endpoint_str, am_relay_str, relay_id_str, relay_sig_str, date_str, sig_str
        );
        match &self.endpoint6 {
            Some(endpoint6) => format!("{},{}", base, endpoint6),
            None => base,
        }
    }

    /// Deserialize from the compact CSV format.
    /// Requires the id to be provided externally. Accepts the six-field form, and the
    /// seven-field form with endpoint6 (issue #493); an endpoint6 that doesn't parse is an error.
    pub fn deserialize_csv(id: String, csv: &str) -> Option<Self> {
        let parts: Vec<&str> = csv.split(',').collect();
        if parts.len() != 6 && parts.len() != 7 {
            return None;
        }
        let endpoint6 = match parts.get(6) {
            Some(text) if !text.is_empty() => Some(InetSocketAddress::from_str(text).ok()?),
            _ => None,
        };

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
            endpoint6,
            am_relay,
            relay_id,
            relay_sig,
            date,
            sig,
        })
    }
}
