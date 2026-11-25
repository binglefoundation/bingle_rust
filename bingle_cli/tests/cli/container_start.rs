// Tests for scripts/docker_start.sh (relay image) and scripts/docker_webserver_start.sh (webserver
// image). They run each entrypoint with a stub program in place of the binary, which records the
// arguments it was handed, and check what the script prints.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const PHRASE: &str = "abandon ability able about above absent absorb abstract";

/// A stand-in program: writes each argument on its own line to $STUB_OUT/args.
const STUB: &str = r#"#!/usr/bin/env bash
printf '%s\n' "$@" >"$STUB_OUT/args"
"#;

struct Harness {
    dir: TempDir,
}

impl Harness {
    fn new() -> Harness {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("out")).expect("mkdir");
        let stub = dir.path().join("stub");
        fs::write(&stub, STUB).expect("write stub");
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("chmod");
        // the relay script refuses to start without non-empty STUN and node files
        fs::write(dir.path().join("stun.txt"), "stun.example.com:3478\n").expect("write stun");
        fs::write(dir.path().join("node.json"), "{}\n").expect("write node");
        Harness { dir }
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.dir.path().join(sub)
    }

    /// Run `script` with only `env` set, plus the stub in place of the binary named by `bin_var`.
    fn run(&self, script: &str, bin_var: &str, env: &[(&str, &str)]) -> Output {
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../scripts")
            .join(script);
        let mut cmd = Command::new("bash");
        cmd.arg(script)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env(bin_var, self.path("stub"))
            .env("STUB_OUT", self.path("out"))
            .env("STUN_FILE", self.path("stun.txt"))
            .env("NODE_FILE", self.path("node.json"))
            .env("HANDLE", "relay-1")
            .env("PASSPHRASE", PHRASE)
            .env("PORT", "12121");
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.output().expect("run entrypoint")
    }

    fn args(&self) -> Vec<String> {
        fs::read_to_string(self.path("out/args"))
            .expect("stub args")
            .lines()
            .map(str::to_string)
            .collect()
    }
}

fn printed(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(String::as_str)
}

#[test]
pub fn relay_start_logs_the_command_without_the_passphrase() {
    let h = Harness::new();
    let out = h.run(
        "docker_start.sh",
        "BINGLE_CLI",
        &[("RELAY", "1"), ("EXTERNAL_IP", "192.0.2.1")],
    );
    assert!(out.status.success(), "script failed: {}", printed(&out));
    let text = printed(&out);
    assert!(!text.contains(PHRASE), "passphrase printed: {text}");
    assert!(
        !text.contains("abandon"),
        "part of the passphrase printed: {text}"
    );
    assert!(
        text.contains("--passphrase <set>"),
        "command line not logged: {text}"
    );
    assert!(
        text.contains("--handle relay-1"),
        "handle not logged: {text}"
    );
}

#[test]
pub fn relay_start_still_hands_the_passphrase_to_the_binary() {
    let h = Harness::new();
    let out = h.run("docker_start.sh", "BINGLE_CLI", &[]);
    assert!(out.status.success(), "script failed: {}", printed(&out));
    let args = h.args();
    assert_eq!(value_of(&args, "--passphrase"), Some(PHRASE));
    assert_eq!(value_of(&args, "--handle"), Some("relay-1"));
}

#[test]
pub fn webserver_start_logs_the_command_without_the_passphrase() {
    let h = Harness::new();
    let out = h.run("docker_webserver_start.sh", "BINGLE_WEBSERVER", &[]);
    assert!(out.status.success(), "script failed: {}", printed(&out));
    let text = printed(&out);
    assert!(
        !text.contains("abandon"),
        "part of the passphrase printed: {text}"
    );
    assert!(
        text.contains("--passphrase <set>"),
        "command line not logged: {text}"
    );
    assert_eq!(value_of(&h.args(), "--passphrase"), Some(PHRASE));
}
