use bingle_core::dtls::dtls_openssl::openssl_impl::PeerCmd;

#[test]
fn peer_cmd_stop_displays_as_stop() {
    assert_eq!(format!("{}", PeerCmd::Stop), "Stop");
}

#[test]
fn peer_cmd_send_printable_text_displays_as_text() {
    let payload = b"hello world".to_vec();
    assert_eq!(
        format!("{}", PeerCmd::Send(payload)),
        "Send(\"hello world\")"
    );
}

#[test]
fn peer_cmd_send_text_with_newline_displays_as_text() {
    let payload = b"line1\nline2".to_vec();
    assert_eq!(
        format!("{}", PeerCmd::Send(payload)),
        "Send(\"line1\nline2\")"
    );
}

#[test]
fn peer_cmd_send_binary_displays_length_and_hex_preview() {
    let payload: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09];
    let result = format!("{}", PeerCmd::Send(payload));
    assert_eq!(result, "Send(10 bytes, [00 01 02 03 04 05 06 07 ...])");
}

#[test]
fn peer_cmd_send_binary_short_payload_displays_all_bytes() {
    let payload: Vec<u8> = vec![0xff, 0xfe, 0x00];
    let result = format!("{}", PeerCmd::Send(payload));
    assert_eq!(result, "Send(3 bytes, [ff fe 00 ...])");
}

#[test]
fn peer_cmd_send_invalid_utf8_displays_as_binary() {
    let payload: Vec<u8> = vec![0x80, 0x81, 0x82];
    let result = format!("{}", PeerCmd::Send(payload));
    assert_eq!(result, "Send(3 bytes, [80 81 82 ...])");
}
