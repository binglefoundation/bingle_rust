// Tests for scripts/mailer.sh (issue #67), which sends the emails `logclient --maildir` writes.
// They run the script with stub `curl` and `aws` commands first on PATH, so no network is used.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant, SystemTime};

use tempfile::TempDir;

const TO: &str = "helpdesk@example.com";
const RELAY: &str = "relay@example.com";
const PASSWORD: &str = "s3cret-smtp-pass";

/// A stand-in for curl. It records its arguments and the file it was asked to upload, prints a
/// verbose-style trace to stderr (including the login, as real curl does), and succeeds or fails
/// according to a marker in the uploaded message.
const STUB_CURL: &str = r#"#!/usr/bin/env bash
n=$(( $(ls "$STUB_LOG" | grep -c '^args') + 1 ))
printf '%s\n' "$@" >"$STUB_LOG/args.$n"
upload=""
user=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --upload-file) upload="$2"; shift ;;
    --user) user="$2"; shift ;;
  esac
  shift
done
cp "$upload" "$STUB_LOG/upload.$n"
echo "< 220 email-smtp.amazonaws.com ESMTP" >&2
echo "> AUTH PLAIN $user" >&2
if grep -q BAD_LOGIN "$upload"; then echo "< 535 Authentication Credentials Invalid" >&2; echo "curl: (67) Login denied" >&2; exit 67; fi
echo "< 235 Authentication successful." >&2
if grep -q NO_NETWORK "$upload"; then echo "curl: (7) Failed to connect" >&2; exit 7; fi
echo "> DATA" >&2
echo "< 354 End data with <CR><LF>.<CR><LF>" >&2
if grep -q NOT_VERIFIED "$upload"; then echo "< 554 Message rejected: Email address is not verified." >&2; exit 55; fi
if grep -q REJECT_ME "$upload"; then echo "< 554 Transaction failed: message content rejected" >&2; exit 55; fi
echo "< 250 Ok 0100018b" >&2
"#;

/// A stand-in for the aws command: records its arguments and the --cli-input-json file, then acts on
/// STUB_AWS_MODE (ok, rejected, notverified or throttled).
const STUB_AWS: &str = r#"#!/usr/bin/env bash
n=$(( $(ls "$STUB_LOG" | grep -c '^args') + 1 ))
printf '%s\n' "$@" >"$STUB_LOG/args.$n"
while [[ $# -gt 0 ]]; do
  if [[ "$1" == --cli-input-json ]]; then cp "${2#file://}" "$STUB_LOG/input.$n"; fi
  shift
done
case "${STUB_AWS_MODE:-ok}" in
  ok) echo '{"MessageId":"0100018b"}' ;;
  rejected) echo "An error occurred (MessageRejected) when calling the SendEmail operation: content rejected" >&2; exit 254 ;;
  notverified) echo "An error occurred (MessageRejected) when calling the SendEmail operation: Email address is not verified." >&2; exit 254 ;;
  throttled) echo "An error occurred (TooManyRequestsException) when calling the SendEmail operation: Rate exceeded" >&2; exit 254 ;;
esac
"#;

struct Harness {
    dir: TempDir,
}

impl Harness {
    fn new() -> Harness {
        let dir = tempfile::tempdir().expect("tempdir");
        for sub in ["bin", "stub_log", "maildir/new"] {
            std::fs::create_dir_all(dir.path().join(sub)).expect("mkdir");
        }
        for (name, body) in [("curl", STUB_CURL), ("aws", STUB_AWS)] {
            let path = dir.path().join("bin").join(name);
            std::fs::write(&path, body).expect("write stub");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("chmod stub");
        }
        Harness { dir }
    }

    fn maildir(&self) -> PathBuf {
        self.dir.path().join("maildir")
    }

    fn stub_log(&self) -> PathBuf {
        self.dir.path().join("stub_log")
    }

    /// Put an email file into new/ with modification time `age_secs` seconds in the past.
    fn drop_email(&self, name: &str, body: &str, age_secs: u64) {
        let path = self.maildir().join("new").join(name);
        std::fs::write(&path, body).expect("write email");
        let file = std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("open email");
        file.set_modified(SystemTime::now() - Duration::from_secs(age_secs))
            .expect("set mtime");
    }

    fn command(&self, env: &[(&str, &str)], args: &[&str]) -> Command {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/mailer.sh");
        let path = format!(
            "{}:{}",
            self.dir.path().join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = Command::new("bash");
        cmd.arg(script)
            .args(args)
            .env("PATH", path)
            .env("STUB_LOG", self.stub_log())
            .env("SES_REGION", "ca-central-1")
            .env("SES_RELAY_FROM", RELAY)
            .env("SES_SMTP_USER", "AKIAEXAMPLE")
            .env("SES_SMTP_PASS", PASSWORD)
            .env_remove("MAILER_SEND")
            .env_remove("MAILER_INTERVAL")
            .env_remove("STUB_AWS_MODE");
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd
    }

    /// Run one pass (`--once`) over the Maildir.
    fn run_once(&self, env: &[(&str, &str)]) -> Output {
        let maildir = self.maildir().to_string_lossy().into_owned();
        self.command(env, &["--once", TO, &maildir])
            .output()
            .expect("run mailer.sh")
    }

    fn names_in(&self, sub: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.maildir().join(sub))
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    /// The contents of the stub's recorded file `kind.n` (n from 1).
    fn recorded(&self, kind: &str, n: usize) -> String {
        std::fs::read_to_string(self.stub_log().join(format!("{kind}.{n}"))).unwrap_or_default()
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
pub fn each_file_is_sent_with_a_to_header_and_removed() {
    let h = Harness::new();
    h.drop_email("1.1_1.host", "From: a\r\n\r\nfirst\r\n", 20);
    h.drop_email("2.1_2.host", "From: a\r\n\r\nsecond\r\n", 10);

    let out = h.run_once(&[]);

    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(h.names_in("new").is_empty());
    assert!(h.names_in("failed").is_empty());
    // Oldest first, each with the To: header prepended to the file unchanged.
    assert_eq!(
        h.recorded("upload", 1),
        format!("To: {TO}\r\nFrom: a\r\n\r\nfirst\r\n")
    );
    assert_eq!(
        h.recorded("upload", 2),
        format!("To: {TO}\r\nFrom: a\r\n\r\nsecond\r\n")
    );
    let args = h.recorded("args", 1);
    for expected in [
        "smtps://email-smtp.ca-central-1.amazonaws.com:465",
        "--ssl-reqd",
        &format!("--mail-from\n{RELAY}\n"),
        &format!("--mail-rcpt\n{TO}\n"),
    ] {
        assert!(args.contains(expected), "missing {expected:?} in:\n{args}");
    }
}

#[test]
pub fn a_send_that_may_clear_leaves_the_file_for_the_next_pass() {
    let h = Harness::new();
    h.drop_email("1.1_1.host", "NO_NETWORK\r\n", 0);

    let out = h.run_once(&[]);

    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert_eq!(h.names_in("new"), vec!["1.1_1.host"]);
    assert!(h.names_in("failed").is_empty());
    assert!(stderr(&out).contains("will retry"), "{}", stderr(&out));
}

#[test]
pub fn a_content_rejection_moves_the_file_to_failed_and_later_files_still_send() {
    let h = Harness::new();
    h.drop_email("1.1_1.host", "REJECT_ME\r\n", 20);
    h.drop_email("2.1_2.host", "fine\r\n", 10);

    let out = h.run_once(&[]);

    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(h.names_in("new").is_empty());
    assert_eq!(h.names_in("failed"), vec!["1.1_1.host"]);
    assert!(h.recorded("upload", 2).ends_with("fine\r\n"));
    assert!(
        stderr(&out).contains("554 Transaction failed"),
        "{}",
        stderr(&out)
    );
}

#[test]
pub fn login_and_unverified_address_rejections_are_retried_not_failed() {
    // Both are setup problems that would reject every message, so moving files to failed/ would
    // empty the queue for nothing.
    let h = Harness::new();
    h.drop_email("1.1_1.host", "BAD_LOGIN\r\n", 20);
    h.drop_email("2.1_2.host", "NOT_VERIFIED\r\n", 10);

    let out = h.run_once(&[]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(h.names_in("new"), vec!["1.1_1.host", "2.1_2.host"]);
    assert!(h.names_in("failed").is_empty());
}

#[test]
pub fn the_smtp_password_is_never_logged() {
    let h = Harness::new();
    h.drop_email("1.1_1.host", "BAD_LOGIN\r\n", 20);
    h.drop_email("2.1_2.host", "fine\r\n", 10);

    let out = h.run_once(&[]);

    assert!(
        !stderr(&out).contains(PASSWORD),
        "password leaked: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("535 Authentication Credentials Invalid"),
        "{}",
        stderr(&out)
    );
}

/// Environment overrides, arguments, and text the error must contain.
type ConfigCase<'a> = (Vec<(&'a str, &'a str)>, Vec<&'a str>, &'a str);

#[test]
pub fn missing_or_bad_configuration_exits_2_before_sending() {
    let h = Harness::new();
    h.drop_email("1.1_1.host", "fine\r\n", 0);
    let maildir = h.maildir().to_string_lossy().into_owned();

    let cases: Vec<ConfigCase> = vec![
        (
            vec![("SES_SMTP_PASS", "")],
            vec!["--once", TO, &maildir],
            "SES_SMTP_PASS",
        ),
        (
            vec![("SES_REGION", "")],
            vec!["--once", TO, &maildir],
            "SES_REGION",
        ),
        (
            vec![("SES_RELAY_FROM", "relay")],
            vec!["--once", TO, &maildir],
            "SES_RELAY_FROM",
        ),
        (
            vec![("MAILER_SEND", "pigeon")],
            vec!["--once", TO, &maildir],
            "MAILER_SEND",
        ),
        (
            vec![("MAILER_INTERVAL", "0")],
            vec!["--once", TO, &maildir],
            "MAILER_INTERVAL",
        ),
        (
            vec![],
            vec!["--once", "not an address", &maildir],
            "<to_email>",
        ),
        (vec![], vec!["--once", TO], "Usage"),
    ];
    for (env, args, expected) in cases {
        let out = h.command(&env, &args).output().expect("run");
        assert_eq!(
            out.status.code(),
            Some(2),
            "{env:?} {args:?}: {}",
            stderr(&out)
        );
        assert!(stderr(&out).contains(expected), "{env:?}: {}", stderr(&out));
    }
    assert_eq!(
        h.names_in("new"),
        vec!["1.1_1.host"],
        "nothing should be sent"
    );
}

#[test]
pub fn ses_api_mode_sends_the_raw_email_without_smtp_credentials() {
    let h = Harness::new();
    h.drop_email("1.1_1.host", "From: a\r\n\r\nhello\r\n", 0);

    let out = h.run_once(&[
        ("MAILER_SEND", "ses-api"),
        ("SES_SMTP_USER", ""),
        ("SES_SMTP_PASS", ""),
    ]);

    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(h.names_in("new").is_empty());
    let args = h.recorded("args", 1);
    assert!(args.starts_with("sesv2\nsend-email\n"), "{args}");
    assert!(args.contains("--region\nca-central-1\n"), "{args}");
    let input: serde_json::Value =
        serde_json::from_str(&h.recorded("input", 1)).expect("input is JSON");
    assert_eq!(input["FromEmailAddress"], serde_json::json!(RELAY));
    assert_eq!(input["Destination"]["ToAddresses"], serde_json::json!([TO]));
    let data = input["Content"]["Raw"]["Data"]
        .as_str()
        .expect("raw data")
        .to_string();
    let decoded = Command::new("base64")
        .arg("--decode")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .expect("stdin")
                .write_all(data.as_bytes())?;
            child.wait_with_output()
        })
        .expect("base64 --decode");
    assert_eq!(
        String::from_utf8_lossy(&decoded.stdout),
        format!("To: {TO}\r\nFrom: a\r\n\r\nhello\r\n")
    );
}

#[test]
pub fn ses_api_rejections_fail_and_throttling_and_unverified_addresses_retry() {
    for (mode, failed) in [
        ("rejected", true),
        ("notverified", false),
        ("throttled", false),
    ] {
        let h = Harness::new();
        h.drop_email("1.1_1.host", "hello\r\n", 0);

        let out = h.run_once(&[("MAILER_SEND", "ses-api"), ("STUB_AWS_MODE", mode)]);

        let in_failed = h.names_in("failed") == vec!["1.1_1.host"];
        assert_eq!(in_failed, failed, "{mode}: {}", stderr(&out));
        assert_eq!(h.names_in("new").is_empty(), failed, "{mode}");
    }
}

#[test]
pub fn sigterm_while_waiting_stops_promptly() {
    let h = Harness::new();
    let maildir = h.maildir().to_string_lossy().into_owned();
    let mut child = h
        .command(&[("MAILER_INTERVAL", "60")], &[TO, &maildir])
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn mailer.sh");
    // Let it finish its first (empty) pass and start waiting.
    std::thread::sleep(Duration::from_millis(500));

    let sent = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill");
    assert!(sent.success());
    let start = Instant::now();
    let mut exited = false;
    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(Some(_)) = child.try_wait() {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if !exited {
        let _ = child.kill();
    }
    let status = child.wait().expect("wait");
    assert!(exited, "mailer.sh should exit within 5s of SIGTERM");
    assert!(status.success(), "{status:?}");
}
