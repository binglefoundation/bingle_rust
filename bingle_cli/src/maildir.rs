//! Email output for `logclient` (epic #63, issue #65): format a received message as a minimal RFC
//! 5322 email and write it into a Maildir, for a mailer to send and delete.
//!
//! The email has no `To:` header: the recipient belongs to the mailer. Every header value that came
//! from a sender is stripped of control characters, so a crafted handle cannot add a header. Lines
//! end in CRLF throughout, as SMTP requires, so the file can be sent as-is.
//!
//! A Maildir is three folders: a message is written into `tmp/`, flushed to disk and renamed into
//! `new/`, so a reader of `new/` never sees a partial file.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bingle_local::api::bingle_local_api::Message;
use chrono::DateTime;

/// RFC 5322's limit on a line, excluding the CRLF. A body with a longer line is sent
/// quoted-printable rather than 8bit.
const MAX_LINE_OCTETS: usize = 998;

/// Quoted-printable's limit on an encoded line, excluding the CRLF.
const QP_LINE_CHARS: usize = 76;

/// Format `message` as an email from `from_address`, naming the sender by handle in the display name
/// and by `sender_id` (or the handle, when the id is unknown) in `X-Bingle-Sender`.
///
/// `from_address` must already be valid (see [`validate_from_address`]).
pub fn format_email(message: &Message, sender_id: Option<&str>, from_address: &str) -> String {
    let handle = header_text(&message.sender_handle);
    let sender = header_text(sender_id.unwrap_or(&message.sender_handle));
    let date = DateTime::from_timestamp_millis(message.order_time())
        .unwrap_or(DateTime::UNIX_EPOCH)
        .to_rfc2822();
    let message_id: String = message
        .id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
        .collect();
    let (encoding, body) = encode_body(&message.text);

    let headers = [
        format!("Date: {date}"),
        format!("From: \"{} (Bingle)\" <{from_address}>", quoted(&handle)),
        format!("Subject: Bingle message from {handle}"),
        format!("Message-ID: <{message_id}@bingle>"),
        format!("X-Bingle-Sender: {sender}"),
        "MIME-Version: 1.0".to_string(),
        "Content-Type: text/plain; charset=utf-8".to_string(),
        format!("Content-Transfer-Encoding: {encoding}"),
    ];
    let mut email = headers.join("\r\n");
    email.push_str("\r\n\r\n");
    email.push_str(&body);
    email
}

/// Check a `--from` address: one `local@domain` with no spaces, control characters or angle brackets,
/// since it is written into the `From:` header unescaped.
pub fn validate_from_address(address: &str) -> Result<(), String> {
    let bad_char = address
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '<' | '>' | '"'));
    let well_formed = match address.split_once('@') {
        Some((local, domain)) => !local.is_empty() && !domain.is_empty() && !domain.contains('@'),
        None => false,
    };
    if bad_char || !well_formed {
        return Err(format!(
            "--from expects an email address like support@example.com, got '{address}'"
        ));
    }
    Ok(())
}

/// A header value from untrusted text: control characters (CR and LF among them) removed, so it
/// cannot end the header or start another.
fn header_text(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// `text` escaped for use inside a quoted string.
fn quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The body with CRLF line endings, and the transfer encoding it needs: `8bit` unless a line is
/// longer than RFC 5322 allows, in which case `quoted-printable`.
fn encode_body(text: &str) -> (&'static str, String) {
    let normalised = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalised.split('\n').collect();
    let (encoding, encoded): (&'static str, Vec<String>) =
        if lines.iter().all(|l| l.len() <= MAX_LINE_OCTETS) {
            ("8bit", lines.iter().map(|l| l.to_string()).collect())
        } else {
            (
                "quoted-printable",
                lines.iter().map(|l| quoted_printable_line(l)).collect(),
            )
        };
    (encoding, encoded.join("\r\n") + "\r\n")
}

/// One line of text encoded quoted-printable (RFC 2045 §6.7), with soft line breaks keeping each
/// encoded line within [`QP_LINE_CHARS`].
fn quoted_printable_line(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::new();
    let mut width = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let last = i + 1 == bytes.len();
        // Spaces and tabs are literal except at the end of a line, where they would be lost.
        let literal =
            ((b'!'..=b'~').contains(&b) && b != b'=') || (matches!(b, b' ' | b'\t') && !last);
        let token = if literal {
            (b as char).to_string()
        } else {
            format!("={b:02X}")
        };
        // Leave room for the `=` of a soft break unless this token ends the line.
        let limit = if last {
            QP_LINE_CHARS
        } else {
            QP_LINE_CHARS - 1
        };
        if width + token.len() > limit {
            out.push_str("=\r\n");
            width = 0;
        }
        out.push_str(&token);
        width += token.len();
    }
    out
}

/// A Maildir: `tmp/`, `new/` and `cur/` under one directory.
pub struct Maildir {
    root: PathBuf,
    host: String,
    counter: u64,
}

impl Maildir {
    /// Open the Maildir at `root`, creating it and its three folders if missing.
    pub fn open(root: &Path) -> Result<Maildir, String> {
        for sub in ["tmp", "new", "cur"] {
            let dir = root.join(sub);
            fs::create_dir_all(&dir)
                .map_err(|e| format!("cannot create Maildir folder {}: {e}", dir.display()))?;
        }
        // Maildir names use `/` and `:` as separators, so neither may appear in the host part.
        let host = gethostname::gethostname()
            .to_string_lossy()
            .replace('/', "\\057")
            .replace(':', "\\072");
        Ok(Maildir {
            root: root.to_path_buf(),
            host,
            counter: 0,
        })
    }

    /// Write `content` as a new message: into `tmp/`, flushed to disk, then renamed into `new/`.
    /// Returns the path in `new/`. On failure nothing is left in `new/`.
    pub fn deliver(&mut self, content: &[u8]) -> Result<PathBuf, String> {
        self.counter += 1;
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let name = format!(
            "{secs}.{}_{}.{}",
            std::process::id(),
            self.counter,
            self.host
        );
        let tmp = self.root.join("tmp").join(&name);
        let new = self.root.join("new").join(&name);

        let written = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .and_then(|mut f| {
                f.write_all(content)?;
                f.sync_all()
            });
        if let Err(e) = written {
            let _ = fs::remove_file(&tmp);
            return Err(format!("cannot write {}: {e}", tmp.display()));
        }
        if let Err(e) = fs::rename(&tmp, &new) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("cannot move {} into new/: {e}", tmp.display()));
        }
        // Flush the rename itself, so the file is still in new/ after a power loss.
        if let Ok(dir) = File::open(self.root.join("new")) {
            let _ = dir.sync_all();
        }
        Ok(new)
    }
}
