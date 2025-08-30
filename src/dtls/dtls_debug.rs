use serde::{Deserialize, Serialize};
use base64::{engine::general_purpose, Engine as _};

/// JSON representation of a single DTLS record inside a UDP datagram.
/// This captures the DTLS record header and a base64-encoded payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DtlsRecordJson {
    /// ContentType (e.g., 22=Handshake, 23=ApplicationData, etc.)
    pub content_type: u8,
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
}

/// JSON representation of a UDP datagram containing one or more DTLS records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DtlsUdpPacketJson {
    pub records: Vec<DtlsRecordJson>,
}

/// Convert a raw UDP datagram containing DTLS records into a pretty-printed JSON string.
/// Returns Err(String) if the datagram is malformed.
pub fn dtls_udp_to_json(datagram: &[u8]) -> Result<String, String> {
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
        records.push(DtlsRecordJson {
            content_type,
            version,
            epoch,
            sequence_number: seq,
            length,
            payload_b64,
        });
        i += needed;
    }

    let packet = DtlsUdpPacketJson { records };
    serde_json::to_string_pretty(&packet).map_err(|e| e.to_string())
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

    #[test]
    fn roundtrip_single_record() {
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
