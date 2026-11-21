// Unit tests for the logclient email format and Maildir writer (bingle_cli::maildir, issue #65).
use bingle_cli::chat::parse_chat_args;
use bingle_cli::chat_receive::receive_message;
use bingle_cli::chat_state::ChatState;
use bingle_cli::maildir::{Maildir, format_email, validate_from_address};
use bingle_local::api::bingle_local_api::{BingleLocalApi, Message};
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use serde_json::json;
use tempfile::TempDir;

const FROM: &str = "support@example.com";
/// 2023-11-14T22:13:20Z.
const SENT: i64 = 1_700_000_000_000;

/// A stored message from `handle` with `text`, sent at [`SENT`]. Returns it with the temp dir that
/// holds its state (kept alive).
fn message_from(handle: &str, text: &str) -> (Message, TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut local = BingleApiLocalImpl::new(LocalApiConfig::default());
    local.generate_keypair().expect("keypair");
    local.seed_own_handle_for_tests("support-desk".to_string());
    let path = dir.path().join("state.json").to_string_lossy().into_owned();
    local.save(&path).expect("save");
    let chat_args = parse_chat_args(vec!["--state_file".to_string(), path]).expect("parse");
    let mut state = ChatState::from_chat_args(&chat_args).expect("bridge");
    let message = receive_message(
        &mut state,
        "PEER_ID",
        handle,
        &json!({ "text": text, "sent_time": SENT }),
    )
    .expect("accepted");
    (message, dir)
}

/// The email's header lines and its body.
fn split(email: &str) -> (Vec<&str>, &str) {
    let (head, body) = email
        .split_once("\r\n\r\n")
        .expect("blank line after headers");
    (head.split("\r\n").collect(), body)
}

/// Decode a quoted-printable body (RFC 2045), for checking the encoder round-trips.
fn decode_quoted_printable(body: &str) -> String {
    let joined = body.replace("=\r\n", "");
    let bytes = joined.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).expect("hex");
            out.push(u8::from_str_radix(hex, 16).expect("hex digit"));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).expect("utf8")
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn headers_are_the_minimal_set_in_order_with_no_to() {
    let (message, _dir) = message_from("bob", "help please");
    let email = format_email(&message, Some("PEER_BOB"), FROM);
    let (headers, body) = split(&email);

    assert_eq!(
        headers,
        vec![
            "Date: Tue, 14 Nov 2023 22:13:20 +0000".to_string(),
            format!("From: \"bob (Bingle)\" <{FROM}>"),
            "Subject: Bingle message from bob".to_string(),
            format!("Message-ID: <{}@bingle>", message.id),
            "X-Bingle-Sender: PEER_BOB".to_string(),
            "MIME-Version: 1.0".to_string(),
            "Content-Type: text/plain; charset=utf-8".to_string(),
            "Content-Transfer-Encoding: 8bit".to_string(),
        ]
    );
    assert!(!headers.iter().any(|h| h.starts_with("To:")));
    assert_eq!(body, "help please\r\n");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn sender_header_falls_back_to_the_handle_when_the_id_is_unknown() {
    let (message, _dir) = message_from("bob", "hi");
    let email = format_email(&message, None, FROM);
    assert!(email.contains("\r\nX-Bingle-Sender: bob\r\n"), "{email}");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_handle_with_line_breaks_cannot_add_a_header() {
    let (message, _dir) = message_from("evil\r\nBcc: victim@example.com", "hi");
    let email = format_email(&message, Some("PEER\nX-Injected: 1"), FROM);
    let (headers, _) = split(&email);

    assert_eq!(headers.len(), 8, "{headers:#?}");
    assert!(
        !headers
            .iter()
            .any(|h| h.starts_with("Bcc:") || h.starts_with("X-Injected:")),
        "{headers:#?}"
    );
    assert!(email.contains("Subject: Bingle message from evilBcc: victim@example.com\r\n"));
    assert!(email.contains("X-Bingle-Sender: PEERX-Injected: 1\r\n"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn quotes_in_a_handle_are_escaped_in_the_display_name() {
    let (message, _dir) = message_from("bo\"b\\", "hi");
    let email = format_email(&message, None, FROM);
    assert!(
        email.contains(&format!("From: \"bo\\\"b\\\\ (Bingle)\" <{FROM}>\r\n")),
        "{email}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn every_line_ends_in_crlf() {
    let (message, _dir) = message_from("bob", "one\ntwo\r\nthree\rfour\n");
    let email = format_email(&message, None, FROM);

    assert!(
        !email.replace("\r\n", "").contains(['\r', '\n']),
        "{email:?}"
    );
    let (_, body) = split(&email);
    assert_eq!(body, "one\r\ntwo\r\nthree\r\nfour\r\n\r\n");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_line_over_998_octets_is_sent_quoted_printable_and_round_trips() {
    let long = format!("{} = café ", "word ".repeat(300));
    let text = format!("{long}\nshort line");
    let (message, _dir) = message_from("bob", &text);
    let email = format_email(&message, None, FROM);
    let (headers, body) = split(&email);

    assert!(headers.contains(&"Content-Transfer-Encoding: quoted-printable"));
    for line in body.split("\r\n") {
        assert!(line.len() <= 76, "encoded line too long: {line}");
        assert!(line.is_ascii(), "{line}");
    }
    assert_eq!(
        decode_quoted_printable(body),
        format!("{}\r\nshort line\r\n", long)
    );
    // The trailing space of the long line is encoded so it survives transport.
    assert!(body.contains("=20\r\n"), "{body}");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn from_address_validation() {
    for good in ["support@example.com", "a.b+c@mail.example.org"] {
        assert!(validate_from_address(good).is_ok(), "{good}");
    }
    for bad in [
        "",
        "support",
        "@example.com",
        "support@",
        "a@b@c",
        "sup port@example.com",
        "support@example.com\r\nBcc: x@y",
        "<support@example.com>",
    ] {
        assert!(validate_from_address(bad).is_err(), "{bad:?}");
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn opening_a_maildir_creates_its_three_folders() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("support");
    Maildir::open(&root).expect("open");
    for sub in ["tmp", "new", "cur"] {
        assert!(root.join(sub).is_dir(), "{sub}/ should exist");
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn deliver_writes_into_new_with_unique_names_and_leaves_tmp_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut maildir = Maildir::open(dir.path()).expect("open");

    let first = maildir.deliver(b"first\r\n").expect("first");
    let second = maildir.deliver(b"second\r\n").expect("second");

    assert_ne!(first, second);
    for path in [&first, &second] {
        assert_eq!(path.parent(), Some(dir.path().join("new").as_path()));
        let name = path.file_name().and_then(|n| n.to_str()).expect("name");
        assert!(!name.contains(':'), "{name}");
    }
    assert_eq!(std::fs::read(&first).expect("read"), b"first\r\n");
    assert_eq!(std::fs::read(&second).expect("read"), b"second\r\n");
    assert_eq!(
        std::fs::read_dir(dir.path().join("tmp"))
            .expect("tmp")
            .count(),
        0
    );
}

#[test]
#[cfg(all(not(target_os = "ios"), unix))]
pub fn a_failed_write_leaves_nothing_in_new() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut maildir = Maildir::open(dir.path()).expect("open");
    let tmp = dir.path().join("tmp");
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o500)).expect("chmod");
    // Root ignores the permission, so there is no failure to test.
    if std::fs::write(tmp.join("probe"), b"").is_ok() {
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o700)).expect("chmod back");
        eprintln!("skipping: running with privileges that ignore directory permissions");
        return;
    }

    let result = maildir.deliver(b"lost\r\n");

    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o700)).expect("chmod back");
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_dir(dir.path().join("new"))
            .expect("new")
            .count(),
        0
    );
}
