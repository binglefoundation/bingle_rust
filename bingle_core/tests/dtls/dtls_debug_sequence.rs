use bingle_core::dtls::dtls_debug::dtls_udp_to_json_with_level;
use serde_json::Value;

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

#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_trace_json_includes_sequence_and_epoch() {
    let epoch = 3u16;
    let seq: u64 = 0x0000_0000_00AB_CDu64; // fits in 48 bits
    let datagram = build_dtls_record(22, epoch, seq, [0xFE, 0xFD], &[0u8; 12]); // minimal handshake header length

    let json_txt = dtls_udp_to_json_with_level(&datagram, tracing::Level::TRACE).expect("ok");
    assert!(
        json_txt.contains("\"records\""),
        "expected JSON output at TRACE"
    );

    let v: Value = serde_json::from_str(&json_txt).expect("valid json");
    let rec = v
        .get("records")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .expect("record present");
    let se = rec
        .get("sequence_number")
        .and_then(|x| x.as_u64())
        .expect("sequence present");
    let ep = rec
        .get("epoch")
        .and_then(|x| x.as_u64())
        .expect("epoch present");
    assert_eq!(se, seq);
    assert_eq!(ep as u16, epoch);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_debug_compact_includes_sequence_and_epoch() {
    let epoch = 7u16;
    let seq: u64 = 0x11_2233_4455u64; // within 48 bits
    let datagram = build_dtls_record(23, epoch, seq, [0xFE, 0xFD], &[1u8, 2u8, 3u8]);

    let out = dtls_udp_to_json_with_level(&datagram, tracing::Level::DEBUG).expect("ok");
    assert!(
        !out.is_empty(),
        "expected some output at or above debug level"
    );

    // In debug compact mode we expect epoch and seq presented. If we're at TRACE, we still
    // expect the JSON string to contain the numeric sequence and epoch values.
    if out.contains("\"records\"") {
        assert!(
            out.contains(&format!("\"sequence_number\": {}", seq)),
            "trace JSON should include sequence_number"
        );
        assert!(
            out.contains(&format!("\"epoch\": {}", epoch)),
            "trace JSON should include epoch"
        );
    } else {
        // Compact summary path
        assert!(
            out.contains(&format!("epoch={}", epoch)),
            "compact summary should include epoch"
        );
        // Accept both seq=123 and seq="123" to be tolerant of formatting
        let s1 = format!("seq={}", seq);
        let s2 = format!("seq=\"{}\"", seq);
        assert!(
            out.contains(&s1) || out.contains(&s2),
            "compact summary should include sequence number"
        );
    }
}
