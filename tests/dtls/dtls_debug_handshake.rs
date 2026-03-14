

use serde_json::Value;
use std::sync::Once;

static INIT_LOG: Once = Once::new();

#[cfg_attr(not(target_os = "ios"), test)]
pub fn dtls_debug_parses_handshake_type_and_extensions() {
    // Ensure logging is initialized at TRACE so dtls_udp_to_json performs full decode
    INIT_LOG.call_once(|| {
        let _ = simple_logger::SimpleLogger::new()
            .with_level(log::LevelFilter::Trace)
            .init();
    });
    use rust_comms::dtls::dtls_debug::{dtls_udp_to_json, json_to_dtls_udp};

    // Build a minimal DTLS Handshake (ClientHello) record with two extensions: server_name (0x0000) and key_share (0x0033).
    // Handshake header (DTLS): type(1)=1, length(3), message_seq(2)=0, fragment_offset(3)=0, fragment_length(3)=length
    // ClientHello body (DTLS 1.2): version(2), random(32), session_id_len(1)=0, cookie_len(1)=0,
    // cipher_suites_len(2)=2, cipher_suites(2)=[0x00,0x00], compression_methods_len(1)=1, method=0,
    // extensions_len(2)= (ext1 + ext2)
    let mut body: Vec<u8> = Vec::new();
    // version FE FD
    body.extend_from_slice(&[0xFE, 0xFD]);
    // random 32 bytes
    body.extend_from_slice(&[0u8; 32]);
    // session_id_len=0
    body.push(0);
    // cookie_len=0
    body.push(0);
    // cipher_suites_len=2 (one suite)
    body.extend_from_slice(&2u16.to_be_bytes());
    // one dummy cipher suite 0x0000
    body.extend_from_slice(&[0x00, 0x00]);
    // compression_methods_len=1, method=0
    body.push(1u8);
    body.push(0u8);
    // Build extensions vector: two extensions with zero-length data
    let mut exts: Vec<u8> = Vec::new();
    // ext 0x0000 server_name, len=0
    exts.extend_from_slice(&0u16.to_be_bytes());
    exts.extend_from_slice(&0u16.to_be_bytes());
    // ext 0x0033 key_share, len=0
    exts.extend_from_slice(&0x0033u16.to_be_bytes());
    exts.extend_from_slice(&0u16.to_be_bytes());
    // extensions_len
    body.extend_from_slice(&(exts.len() as u16).to_be_bytes());
    body.extend_from_slice(&exts);

    // Handshake header
    let hlen = body.len() as u32; // 24-bit length
    let mut hs: Vec<u8> = Vec::new();
    hs.push(1u8); // ClientHello
    hs.extend_from_slice(&[((hlen >> 16) & 0xFF) as u8, ((hlen >> 8) & 0xFF) as u8, (hlen & 0xFF) as u8]);
    hs.extend_from_slice(&0u16.to_be_bytes()); // message_seq
    hs.extend_from_slice(&[0, 0, 0]); // fragment_offset
    hs.extend_from_slice(&[((hlen >> 16) & 0xFF) as u8, ((hlen >> 8) & 0xFF) as u8, (hlen & 0xFF) as u8]); // fragment_length
    hs.extend_from_slice(&body);

    // Wrap in DTLS record header
    let mut datagram: Vec<u8> = Vec::new();
    datagram.push(22u8); // content_type handshake
    datagram.push(0xFE); // DTLS 1.2
    datagram.push(0xFD);
    datagram.extend_from_slice(&0u16.to_be_bytes()); // epoch
    datagram.extend_from_slice(&[0, 0, 0, 0, 0, 1]); // sequence_number
    datagram.extend_from_slice(&(hs.len() as u16).to_be_bytes());
    datagram.extend_from_slice(&hs);

    let json = dtls_udp_to_json(&datagram).expect("dtls_udp_to_json ok");

    // Validate JSON contains handshake type name and extension names
    let v: Value = serde_json::from_str(&json).expect("json parse");
    let records = v.get("records").and_then(|r| r.as_array()).expect("records array");
    assert_eq!(records.len(), 1);
    let rec0 = &records[0];
    assert_eq!(rec0.get("content_type").and_then(|x| x.as_u64()), Some(22));
    let hs = rec0.get("handshake").expect("handshake present");
    assert_eq!(hs.get("handshake_type").and_then(|x| x.as_u64()), Some(1));
    let ht_name = hs.get("handshake_type_name").and_then(|x| x.as_str()).unwrap_or("");
    assert!(ht_name.eq_ignore_ascii_case("client_hello"), "unexpected handshake_type_name: {}", ht_name);

    // Check extensions list includes our two IDs and names
    let ch = hs.get("client_hello").expect("client_hello present");
    let exts_json = ch.get("extensions").and_then(|x| x.as_array()).expect("extensions array");
    let mut have_sn = false; let mut have_ks = false;
    for e in exts_json {
        let id = e.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
        let name = e.get("name").and_then(|x| x.as_str()).unwrap_or("");
        if id == 0 { have_sn = true; assert_eq!(name, "server_name"); }
        if id == 0x33 { have_ks = true; assert_eq!(name, "key_share"); }
    }
    assert!(have_sn && have_ks, "expected server_name and key_share extensions");

    // And test round-trip back to bytes matches the original datagram
    let back = json_to_dtls_udp(&json).expect("roundtrip bytes");
    assert_eq!(back, datagram);
}
