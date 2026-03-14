

use rust_comms::dtls::dtls_debug::dtls_udp_to_json;

// Helper to build a single DTLS record datagram with given content type and payload
fn build_dtls_record(ct: u8, epoch: u16, seq: u64, version: [u8; 2], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(13 + payload.len());
    out.push(ct);
    out.extend_from_slice(&version);
    out.extend_from_slice(&epoch.to_be_bytes());
    // 48-bit sequence number; we only support low 48 bits
    let seq48 = seq & 0x0000_FFFF_FFFF_FFFFu64;
    out.push(((seq48 >> 40) & 0xFF) as u8);
    out.push(((seq48 >> 32) & 0xFF) as u8);
    out.push(((seq48 >> 24) & 0xFF) as u8);
    out.push(((seq48 >> 16) & 0xFF) as u8);
    out.push(((seq48 >> 8) & 0xFF) as u8);
    out.push((seq48 & 0xFF) as u8);
    out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn dtls_debug_includes_alert_level_and_description() {
    // DTLS alert: level=2 (fatal), description=40 (handshake_failure)
    let alert_payload = [2u8, 40u8];
    let datagram = build_dtls_record(21, 0, 1, [254, 253], &alert_payload);

    // We don't control the global logger level here; dtls_udp_to_json adapts to level.
    // We validate that either trace JSON contains an `alert` object, or the debug summary
    // contains the alert L/D snippet.
    let txt = dtls_udp_to_json(&datagram).expect("parser should succeed");
    assert!(!txt.is_empty(), "expected some output at or above debug level");

    // Try JSON path first
    if txt.contains("\"records\"") {
        // Expect alert fields present with correct values
        assert!(txt.contains("\"alert\""), "trace JSON should contain alert object");
        assert!(txt.contains("\"level\": 2"), "alert.level should be 2");
        assert!(txt.contains("\"description\": 40"), "alert.description should be 40");
    } else {
        // Debug summary path: contains "alert=L2/D40"
        assert!(txt.contains("alert=L2/D40"), "debug summary should include alert snippet");
    }
}
