// Tests for scripts/docker_gateway_start.sh (issue #473), the gateway image's entrypoint. They run
// it with stub programs in place of bingle_cli and mailer.sh, which record their arguments and the
// environment the script hands them.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const FROM: &str = "relay@example.com";
const PHRASE: &str = "abandon ability able about above absent absorb abstract";

/// A stand-in program: writes each argument on its own line to $STUB_OUT/args, and the mailer
/// variables to $STUB_OUT/env.
const STUB: &str = r#"#!/usr/bin/env bash
printf '%s\n' "$@" >"$STUB_OUT/args"
printf 'MAILER_SEND=%s\nSES_RELAY_FROM=%s\nSES_REGION=%s\n' "${MAILER_SEND:-}" "${SES_RELAY_FROM:-}" "${SES_REGION:-}" >"$STUB_OUT/env"
"#;

struct Harness {
    dir: TempDir,
}

impl Harness {
    fn new() -> Harness {
        let dir = tempfile::tempdir().expect("tempdir");
        for sub in ["data", "out", "bin"] {
            fs::create_dir(dir.path().join(sub)).expect("mkdir");
        }
        let stub = dir.path().join("bin/stub");
        fs::write(&stub, STUB).expect("write stub");
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("chmod");
        Harness { dir }
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.dir.path().join(sub)
    }

    /// Run the entrypoint for `role` with only `env` set (plus the stubs and the data volume).
    fn run(&self, role: &str, env: &[(&str, &str)]) -> Output {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/docker_gateway_start.sh");
        let stub = self.path("bin/stub");
        let mut cmd = Command::new("bash");
        cmd.arg(script)
            .arg(role)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("GATEWAY_DATA", self.path("data"))
            .env("BINGLE_CLI", &stub)
            .env("MAILER", &stub)
            .env("STUB_OUT", self.path("out"));
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.output().expect("run entrypoint")
    }

    fn args(&self) -> Vec<String> {
        fs::read_to_string(self.path("out/args"))
            .expect("stub ran")
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn stub_ran(&self) -> bool {
        self.path("out/args").exists()
    }

    fn data(&self, name: &str) -> String {
        self.path("data").join(name).to_string_lossy().into_owned()
    }
}

/// The value following `flag` in `args`.
fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(String::as_str)
}

#[test]
pub fn logclient_runs_on_the_shared_volume_with_the_spam_limit() {
    let h = Harness::new();
    fs::write(h.path("data/state.json"), "{}").expect("state");

    let out = h.run("logclient", &[("FROM_ADDRESS", FROM), ("SPAM_LIMIT", "20")]);

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let args = h.args();
    assert_eq!(args[0], "logclient");
    assert_eq!(
        value_of(&args, "--state_file"),
        Some(h.data("state.json").as_str())
    );
    assert_eq!(
        value_of(&args, "--maildir"),
        Some(h.data("maildir").as_str())
    );
    assert_eq!(value_of(&args, "--from"), Some(FROM));
    assert_eq!(value_of(&args, "--spam-limit"), Some("20"));
    assert_eq!(value_of(&args, "--poll-interval"), Some("0"));
    assert_eq!(
        value_of(&args, "--node-file"),
        Some("/app/nodely_deployed_mainnet_node.json")
    );
    assert!(!args.iter().any(|a| a == "--passphrase"));
}

#[test]
pub fn logclient_without_a_spam_limit_passes_none() {
    let h = Harness::new();
    fs::write(h.path("data/state.json"), "{}").expect("state");

    let out = h.run("logclient", &[("FROM_ADDRESS", FROM), ("SPAM_LIMIT", "")]);

    assert!(out.status.success());
    assert!(!h.args().iter().any(|a| a == "--spam-limit"));
}

#[test]
pub fn the_first_start_imports_the_account_without_logging_the_phrase() {
    let h = Harness::new();

    let out = h.run(
        "logclient",
        &[
            ("FROM_ADDRESS", FROM),
            ("PASSPHRASE", PHRASE),
            ("HANDLE", "@@support_desk"),
        ],
    );

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let args = h.args();
    assert_eq!(value_of(&args, "--passphrase"), Some(PHRASE));
    assert_eq!(value_of(&args, "--handle"), Some("@@support_desk"));
    let logged = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!logged.contains("abandon"), "{logged}");
}

#[test]
pub fn logclient_refuses_to_start_with_no_state_file_and_no_phrase() {
    let h = Harness::new();

    let out = h.run("logclient", &[("FROM_ADDRESS", FROM)]);

    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("PASSPHRASE and HANDLE"));
    assert!(!h.stub_ran());
}

#[test]
pub fn a_phrase_without_a_handle_is_refused() {
    let h = Harness::new();

    let out = h.run(
        "logclient",
        &[("FROM_ADDRESS", FROM), ("PASSPHRASE", PHRASE)],
    );

    assert_eq!(out.status.code(), Some(2));
    assert!(!h.stub_ran());
}

#[test]
pub fn mailer_sends_the_maildir_through_the_ses_api_as_the_from_address() {
    let h = Harness::new();

    let out = h.run(
        "mailer",
        &[
            ("FROM_ADDRESS", FROM),
            ("TO_EMAIL", "helpdesk@example.com"),
            ("AWS_REGION", "ca-central-1"),
        ],
    );

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        h.args(),
        vec!["helpdesk@example.com".to_string(), h.data("maildir")]
    );
    let env = fs::read_to_string(h.path("out/env")).expect("env");
    assert!(env.contains("MAILER_SEND=ses-api\n"), "{env}");
    assert!(env.contains(&format!("SES_RELAY_FROM={FROM}\n")), "{env}");
    assert!(env.contains("SES_REGION=ca-central-1\n"), "{env}");
}

#[test]
pub fn missing_settings_and_unknown_roles_are_refused() {
    let h = Harness::new();
    for (role, env) in [
        ("mailer", vec![("FROM_ADDRESS", FROM)]),
        ("logclient", vec![]),
        ("relay", vec![("FROM_ADDRESS", FROM)]),
    ] {
        let out = h.run(role, &env);
        assert_eq!(out.status.code(), Some(2), "{role} {env:?}");
    }
    assert!(!h.stub_ran());
}
