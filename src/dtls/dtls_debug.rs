use serde::{Deserialize, Serialize};
use base64::{engine::general_purpose, Engine as _};

/// JSON representation of a single DTLS record inside a UDP datagram.
/// This captures the DTLS record header and a base64-encoded payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DtlsRecordJson {
    /// ContentType (e.g., 22=Handshake, 23=ApplicationData, etc.)
    pub content_type: u8,
    /// Human-readable ContentType name (e.g., "handshake", "alert"). Optional for backward compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type_name: Option<String>,
    /// ProtocolVersion bytes [major, minor] (e.g., [254, 253] for DTLS 1.2)
    pub version: [u8; 2],
    /// Epoch from the DTLS record header (big-endian)
    pub epoch: u16,
    /// 48-bit sequence number promoted to u64 for JSON; upper 16 bits must be zero
    pub sequence_number: u64,
    /// Length field from the record header (bytes in payload)
    pub length: u16,
    /// Base64 of the payload bytes
    pub payload_b64: String,
    /// Optional parsed DTLS handshake header/body summary (present when content_type==22 and parsing succeeds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handshake: Option<HandshakeJson>,
    /// Optional parsed DTLS alert summary (present when content_type==21 and payload has at least 2 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert: Option<AlertJson>,
}

/// JSON representation of a UDP datagram containing one or more DTLS records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DtlsUdpPacketJson {
    pub records: Vec<DtlsRecordJson>,
}

/// Summary of a DTLS handshake message, including header fields and a minimal decode of extensions
/// for ClientHello and ServerHello.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandshakeJson {
    pub handshake_type: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handshake_type_name: Option<String>,
    /// 24-bit body length promoted to u32
    pub length: u32,
    pub message_seq: u16,
    /// 24-bit promoted to u32
    pub fragment_offset: u32,
    /// 24-bit promoted to u32
    pub fragment_length: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_hello: Option<ClientHelloSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_hello: Option<ServerHelloSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientHelloSummary {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ExtensionJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerHelloSummary {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ExtensionJson>,
}

/// Summary of a DTLS Alert record. Only carries the raw level and description bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AlertJson {
    /// Alert level (1 = warning, 2 = fatal)
    pub level: u8,
    /// Alert description (per TLS/DTLS AlertDescription enum)
    pub description: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionJson {
    pub id: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Map DTLS ContentType byte to a human-readable name.
fn content_type_name(ct: u8) -> &'static str {
    match ct {
        20 => "change_cipher_spec",
        21 => "alert",
        22 => "handshake",
        23 => "application_data",
        24 => "heartbeat",
        _ => "unknown",
    }
}

/// Map DTLS HandshakeType to a human-readable name.
fn handshake_type_name(ht: u8) -> &'static str {
    match ht {
        0 => "hello_request",
        1 => "client_hello",
        2 => "server_hello",
        3 => "hello_verify_request",
        11 => "certificate",
        12 => "server_key_exchange",
        13 => "certificate_request",
        14 => "server_hello_done",
        15 => "certificate_verify",
        16 => "client_key_exchange",
        20 => "finished",
        _ => "unknown",
    }
}

/// Common TLS/DTLS extension type IDs → names (subset).
fn extension_type_name(id: u16) -> &'static str {
    match id {
        0x0000 => "server_name",
        0x0005 => "status_request",
        0x000a => "supported_groups",
        0x000b => "ec_point_formats",
        0x000d => "signature_algorithms",
        0x000f => "heartbeat",
        0x0010 => "application_layer_protocol_negotiation",
        0x0012 => "signed_certificate_timestamp",
        0x0015 => "padding",
        0x0017 => "extended_master_secret",
        0x0023 => "session_ticket",
        0x002b => "supported_versions",
        0x002d => "psk_key_exchange_modes",
        0x0031 => "early_data",
        0x0033 => "key_share",
        0xFF01 => "renegotiation_info",
        _ => "unknown",
    }
}

#[inline]
fn read_u24(be3: &[u8]) -> u32 { ((be3[0] as u32) << 16) | ((be3[1] as u32) << 8) | (be3[2] as u32) }

/// Convert a raw UDP datagram containing DTLS records into a pretty-printed JSON string.
/// Returns Err(String) if the datagram is malformed.
pub fn dtls_udp_to_json(datagram: &[u8]) -> Result<String, String> {
    // Use current global log level
    let level = if log::log_enabled!(log::Level::Trace) {
        log::Level::Trace
    } else if log::log_enabled!(log::Level::Debug) {
        log::Level::Debug
    } else {
        log::Level::Info
    };
    dtls_udp_to_json_with_level(datagram, level)
}

/// Convert a raw UDP datagram containing DTLS records into a pretty-printed JSON string.
/// Behavior depends on the provided log level for testability.
/// Returns Err(String) if the datagram is malformed.
pub fn dtls_udp_to_json_with_level(datagram: &[u8], level: log::Level) -> Result<String, String> {
    // Behavior depends on provided log level:
    // - Trace: full JSON (pretty) with handshake introspection (existing behavior)
    // - Debug: single-line summary for quick inspection
    // - Below Debug: return an empty string
    if level >= log::Level::Trace {
        // Full decode path (existing behavior)
        let mut i: usize = 0;
        let mut records: Vec<DtlsRecordJson> = Vec::new();

        // DTLS record header is 13 bytes: 1 + 2 + 2 + 6 + 2
        while i < datagram.len() {
            if datagram.len() - i < 13 {
                return Err("truncated DTLS record header".to_string());
            }
            let content_type = datagram[i];
            let version = [datagram[i + 1], datagram[i + 2]];
            let epoch = u16::from_be_bytes([datagram[i + 3], datagram[i + 4]]);
            let seq = ((datagram[i + 5] as u64) << 40)
                | ((datagram[i + 6] as u64) << 32)
                | ((datagram[i + 7] as u64) << 24)
                | ((datagram[i + 8] as u64) << 16)
                | ((datagram[i + 9] as u64) << 8)
                | (datagram[i + 10] as u64);
            let length = u16::from_be_bytes([datagram[i + 11], datagram[i + 12]]);
            let needed = 13 + length as usize;
            if datagram.len() - i < needed {
                return Err("truncated DTLS record payload".to_string());
            }
            let payload = &datagram[i + 13..i + needed];
            let payload_b64 = general_purpose::STANDARD.encode(payload);

            // Attempt to parse DTLS Handshake layer if content_type == 22
            let handshake = if content_type == 22 && payload.len() >= 12 {
                // DTLS Handshake header: type(1), length(3), message_seq(2), fragment_offset(3), fragment_length(3)
                let htype = payload[0];
                let hlen = read_u24(&payload[1..4]);
                let hseq = u16::from_be_bytes([payload[4], payload[5]]);
                let hoff = read_u24(&payload[6..9]);
                let hfrag_len = read_u24(&payload[9..12]);
                let mut hs = HandshakeJson {
                    handshake_type: htype,
                    handshake_type_name: Some(handshake_type_name(htype).to_string()),
                    length: hlen,
                    message_seq: hseq,
                    fragment_offset: hoff,
                    fragment_length: hfrag_len,
                    client_hello: None,
                    server_hello: None,
                };

                // Only attempt body parsing if we have the full first fragment
                if hoff == 0 && (12usize) < payload.len() {
                    let body = &payload[12..payload.len().min(12 + hlen as usize)];
                    match htype {
                        1 => { // ClientHello
                            if let Some(exts) = parse_client_hello_extensions(body) {
                                hs.client_hello = Some(ClientHelloSummary { extensions: exts });
                            }
                        }
                        2 => { // ServerHello
                            if let Some(exts) = parse_server_hello_extensions(body) {
                                hs.server_hello = Some(ServerHelloSummary { extensions: exts });
                            }
                        }
                        _ => {}
                    }
                }
                Some(hs)
            } else { None };

            // Attempt to parse DTLS Alert (content_type == 21)
            let alert = if content_type == 21 && payload.len() >= 2 {
                Some(AlertJson { level: payload[0], description: payload[1] })
            } else { None };

            records.push(DtlsRecordJson {
                content_type,
                content_type_name: Some(content_type_name(content_type).to_string()),
                version,
                epoch,
                sequence_number: seq,
                length,
                payload_b64,
                handshake,
                alert,
            });
            i += needed;
        }

        let packet = DtlsUdpPacketJson { records };
        return serde_json::to_string_pretty(&packet).map_err(|e| e.to_string());
    } else if log::log_enabled!(log::Level::Debug) {
        // Produce a terse single-line summary without heavy parsing
        let mut i: usize = 0;
        let mut parts: Vec<String> = Vec::new();
        while i < datagram.len() {
            if datagram.len() - i < 13 {
                return Err("truncated DTLS record header".to_string());
            }
            let ct = datagram[i];
            let ct_name = content_type_name(ct);
            let epoch = u16::from_be_bytes([datagram[i + 3], datagram[i + 4]]);
            let seq = ((datagram[i + 5] as u64) << 40)
                | ((datagram[i + 6] as u64) << 32)
                | ((datagram[i + 7] as u64) << 24)
                | ((datagram[i + 8] as u64) << 16)
                | ((datagram[i + 9] as u64) << 8)
                | (datagram[i + 10] as u64);
            let len = u16::from_be_bytes([datagram[i + 11], datagram[i + 12]]) as usize;
            // Default: no handshake type
            let mut hs_name: Option<&'static str> = None;
            if ct == 22 {
                // Handshake content: best-effort read first byte as type if present
                if datagram.len() - i >= 14 { // ensure at least one payload byte
                    let htype = datagram[i + 13];
                    hs_name = Some(handshake_type_name(htype));
                }
            }
            if let Some(hn) = hs_name {
                parts.push(format!("len={} ct={} epoch={} seq={} hs={}", len, ct_name, epoch, seq, hn));
            } else {
                parts.push(format!("len={} ct={} epoch={} seq=\"{}\"", len, ct_name, epoch, seq));
            }
            let needed = 13 + len;
            if datagram.len() - i < needed { return Err("truncated DTLS record payload".to_string()); }
            // Alert summary: include level/description bytes when available
            if ct == 21 && len >= 2 {
                let level = datagram[i + 13];
                let desc = datagram[i + 14];
                let last = parts.pop().unwrap_or_else(|| format!("len={} ct={}", len, ct_name));
                parts.push(format!("{} alert=L{}/D{}", last, level, desc));
            }
            i += needed;
        }
        let summary = format!("DTLS {} bytes [{}]", datagram.len(), parts.join("; "));
        Ok(summary)
    } else {
        // Below Debug: suppress output
        Ok(String::new())
    }
}

fn parse_client_hello_extensions(body: &[u8]) -> Option<Vec<ExtensionJson>> {
    // Structure (DTLS 1.2):
    // version(2) + random(32) + session_id_len(1) + session_id + cookie_len(1) + cookie +
    // cipher_suites_len(2) + cipher_suites + compression_methods_len(1) + compression_methods +
    // [extensions_len(2) + extensions]
    let mut p = 0usize;
    if body.len() < p + 2 { return None; }
    p += 2; // version
    if body.len() < p + 32 { return None; }
    p += 32; // random
    if body.len() < p + 1 { return None; }
    let sid_len = body[p] as usize; p += 1;
    if body.len() < p + sid_len { return None; }
    p += sid_len; // session_id
    if body.len() < p + 1 { return None; }
    let cookie_len = body[p] as usize; p += 1;
    if body.len() < p + cookie_len { return None; }
    p += cookie_len; // cookie
    if body.len() < p + 2 { return None; }
    let cs_len = u16::from_be_bytes([body[p], body[p+1]]) as usize; p += 2;
    if body.len() < p + cs_len { return None; }
    p += cs_len; // cipher suites
    if body.len() < p + 1 { return None; }
    let cm_len = body[p] as usize; p += 1;
    if body.len() < p + cm_len { return None; }
    p += cm_len; // compression methods
    if body.len() < p + 2 { return Some(Vec::new()); }
    let ext_total = u16::from_be_bytes([body[p], body[p+1]]) as usize; p += 2;
    if body.len() < p + ext_total { return None; }
    parse_extensions(&body[p..p+ext_total])
}

fn parse_server_hello_extensions(body: &[u8]) -> Option<Vec<ExtensionJson>> {
    // Structure (TLS 1.2 style):
    // version(2) + random(32) + session_id_len(1) + session_id + cipher_suite(2) + compression_method(1) +
    // [extensions_len(2) + extensions]
    let mut p = 0usize;
    if body.len() < p + 2 { return None; }
    p += 2; // version
    if body.len() < p + 32 { return None; }
    p += 32; // random
    if body.len() < p + 1 { return None; }
    let sid_len = body[p] as usize; p += 1;
    if body.len() < p + sid_len { return None; }
    p += sid_len; // session_id
    if body.len() < p + 2 { return None; }
    p += 2; // cipher_suite
    if body.len() < p + 1 { return None; }
    p += 1; // compression_method
    if body.len() < p + 2 { return Some(Vec::new()); }
    let ext_total = u16::from_be_bytes([body[p], body[p+1]]) as usize; p += 2;
    if body.len() < p + ext_total { return None; }
    parse_extensions(&body[p..p+ext_total])
}

fn parse_extensions(mut data: &[u8]) -> Option<Vec<ExtensionJson>> {
    let mut out = Vec::new();
    while data.len() >= 4 {
        let id = u16::from_be_bytes([data[0], data[1]]);
        let len = u16::from_be_bytes([data[2], data[3]]) as usize;
        let name = extension_type_name(id);
        out.push(ExtensionJson { id, name: Some(name.to_string()) });
        if data.len() < 4 + len { return None; }
        data = &data[4 + len..];
    }
    Some(out)
}

/// Convert a JSON string produced by `dtls_udp_to_json` back into raw UDP datagram bytes.
/// Returns Err(String) if the JSON is invalid or inconsistent (e.g., length mismatch).
pub fn json_to_dtls_udp(json: &str) -> Result<Vec<u8>, String> {
    let packet: DtlsUdpPacketJson = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut out: Vec<u8> = Vec::new();

    for rec in packet.records.iter() {
        // Decode payload
        let payload = general_purpose::STANDARD.decode(&rec.payload_b64).map_err(|e| e.to_string())?;
        if payload.len() != rec.length as usize {
            return Err(format!(
                "length mismatch: header={} payload={} bytes",
                rec.length,
                payload.len()
            ));
        }
        // Validate sequence_number upper bits (should be within 48-bit range)
        if rec.sequence_number >> 48 != 0 {
            return Err("sequence_number exceeds 48 bits".to_string());
        }
        // Header
        out.push(rec.content_type);
        out.push(rec.version[0]);
        out.push(rec.version[1]);
        out.extend_from_slice(&rec.epoch.to_be_bytes());
        // 48-bit big-endian sequence number
        out.push(((rec.sequence_number >> 40) & 0xFF) as u8);
        out.push(((rec.sequence_number >> 32) & 0xFF) as u8);
        out.push(((rec.sequence_number >> 24) & 0xFF) as u8);
        out.push(((rec.sequence_number >> 16) & 0xFF) as u8);
        out.push(((rec.sequence_number >> 8) & 0xFF) as u8);
        out.push((rec.sequence_number & 0xFF) as u8);
        out.extend_from_slice(&(rec.length.to_be_bytes()));
        // Payload
        out.extend_from_slice(&payload);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;
    static INIT_LOG: Once = Once::new();

    #[test]
    fn roundtrip_single_record() {
        INIT_LOG.call_once(|| {
            let _ = simple_logger::SimpleLogger::new()
                .with_level(log::LevelFilter::Trace)
                .init();
        });
        // Construct a simple DTLS record: content_type=23 (AppData), version=DTLS1.2 (0xFEFD)
        let payload = b"hello-dtls";
        let mut datagram: Vec<u8> = Vec::new();
        datagram.push(23);
        datagram.push(0xFE);
        datagram.push(0xFD);
        datagram.extend_from_slice(&0u16.to_be_bytes()); // epoch
        // seq = 0x000001_020304 (48-bit). We'll just use 0x000000_000001 for simplicity
        datagram.extend_from_slice(&[0, 0, 0, 0, 0, 1]);
        datagram.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        datagram.extend_from_slice(payload);

        let json = dtls_udp_to_json(&datagram).expect("json");
        let back = json_to_dtls_udp(&json).expect("back");
        assert_eq!(back, datagram);
    }

    #[test]
    fn parse_multiple_records() {
        // Ensure logging is initialized at TRACE so dtls_udp_to_json performs full decode
        INIT_LOG.call_once(|| {
            let _ = simple_logger::SimpleLogger::new()
                .with_level(log::LevelFilter::Trace)
                .init();
        });
        // Two small records concatenated
        let mut d: Vec<u8> = Vec::new();
        for seq in 1u64..=2 {
            let payload = vec![seq as u8; 3];
            d.push(22); // Handshake
            d.push(0xFE);
            d.push(0xFD);
            d.extend_from_slice(&0u16.to_be_bytes());
            d.extend_from_slice(&[0, 0, 0, 0, 0, seq as u8]);
            d.extend_from_slice(&(payload.len() as u16).to_be_bytes());
            d.extend_from_slice(&payload);
        }
        let json = dtls_udp_to_json(&d).expect("json");
        let packet: DtlsUdpPacketJson = serde_json::from_str(&json).expect("de");
        assert_eq!(packet.records.len(), 2);
        let back = json_to_dtls_udp(&json).expect("back");
        assert_eq!(back, d);
    }
}
