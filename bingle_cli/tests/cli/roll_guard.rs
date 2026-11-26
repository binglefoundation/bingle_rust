// Tests for the commit guard in scripts/roll_relays.sh (issue #546): a TestNet roll from a checkout
// behind origin/staging, or onto the commit TestNet already runs, stops before any stack is touched.
//
// Each test builds a throwaway repository with an `origin` holding `staging` and `deployed`, copies
// the script into it, and runs a dry run with stub `aws` and `gh` programs. The stub `aws` knows
// the account but no stacks, so a run that gets past the guard fails at the stack check instead:
// "stack ... is not in ...".
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const ACCOUNT: &str = "123456789012";
/// The message of the first check after the guard: the guard let the roll through.
const PAST_GUARD: &str = "is not in us-east-1";

/// Knows the AWS account; every other call fails, as for a stack that doesn't exist.
const STUB_AWS: &str = r#"#!/usr/bin/env bash
if [[ "$1 $2" == "sts get-caller-identity" ]]; then echo "$STUB_ACCOUNT"; exit 0; fi
exit 1
"#;

/// Answers the two queries the script makes: the newest TestNet deployment, and its latest status.
const STUB_GH: &str = r#"#!/usr/bin/env bash
case "$2" in
  */statuses*) echo success ;;
  *deployments\?environment=*) [[ -n "${STUB_DEPLOYED_SHA:-}" ]] && echo "1 $STUB_DEPLOYED_SHA $STUB_DEPLOYED_VERSION" ;;
esac
exit 0
"#;

struct Repo {
    dir: TempDir,
    /// The first staging commit (version 0.1.0) and the head of staging (0.1.1).
    older: String,
    head: String,
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write_version(work: &Path, version: &str) {
    fs::write(
        work.join("bingle_core/Cargo.toml"),
        format!("[package]\nname = \"bingle_core\"\nversion = \"{version}\"\n"),
    )
    .expect("write Cargo.toml");
}

impl Repo {
    fn new() -> Repo {
        let dir = tempfile::tempdir().expect("tempdir");
        let origin = dir.path().join("origin.git");
        let work = dir.path().join("work");
        git(
            dir.path(),
            &["init", "--bare", "-q", origin.to_str().unwrap()],
        );
        git(
            dir.path(),
            &["init", "-q", "-b", "staging", work.to_str().unwrap()],
        );
        git(
            &work,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );

        fs::create_dir_all(work.join("scripts")).expect("mkdir scripts");
        fs::create_dir_all(work.join("bingle_core")).expect("mkdir bingle_core");
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/roll_relays.sh");
        fs::copy(&script, work.join("scripts/roll_relays.sh")).expect("copy script");

        write_version(&work, "0.1.0");
        git(&work, &["add", "-A"]);
        git(&work, &["commit", "-q", "-m", "0.1.0"]);
        let older = git(&work, &["rev-parse", "HEAD"]);
        git(
            &work,
            &["push", "-q", "origin", "staging", "staging:deployed"],
        );

        write_version(&work, "0.1.1");
        git(&work, &["commit", "-q", "-am", "0.1.1"]);
        let head = git(&work, &["rev-parse", "HEAD"]);
        git(&work, &["push", "-q", "origin", "staging"]);

        let stubs = dir.path().join("stubs");
        fs::create_dir_all(&stubs).expect("mkdir stubs");
        for (name, body) in [("aws", STUB_AWS), ("gh", STUB_GH)] {
            let path = stubs.join(name);
            fs::write(&path, body).expect("write stub");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        fs::write(
            dir.path().join("relays.env"),
            format!(
                "NETWORK=testnet\nACCOUNT_ID={ACCOUNT}\nRELAY=echo-1 relay-stack-1 us-east-1 ec2 /app/node.json\n"
            ),
        )
        .expect("write inventory");
        Repo { dir, older, head }
    }

    fn work(&self) -> PathBuf {
        self.dir.path().join("work")
    }

    fn checkout(&self, sha: &str) {
        git(&self.work(), &["checkout", "-q", sha]);
    }

    /// A dry run, with TestNet's latest successful deployment at `deployed` (sha, version).
    fn dry_run(&self, deployed: Option<(&str, &str)>, extra: &[&str]) -> Output {
        let path = format!(
            "{}:{}",
            self.dir.path().join("stubs").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = Command::new("bash");
        cmd.arg(self.work().join("scripts/roll_relays.sh"))
            .arg("--dry-run")
            .args(extra)
            .current_dir(self.dir.path())
            .env("PATH", path)
            .env("BINGLE_NETWORK", "testnet")
            .env("BINGLE_RELAYS_FILE", self.dir.path().join("relays.env"))
            .env("STUB_ACCOUNT", ACCOUNT);
        if let Some((sha, version)) = deployed {
            cmd.env("STUB_DEPLOYED_SHA", sha)
                .env("STUB_DEPLOYED_VERSION", version);
        }
        cmd.output().expect("run roll_relays.sh")
    }
}

fn printed(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
pub fn a_checkout_behind_staging_is_refused_before_any_stack() {
    let repo = Repo::new();
    repo.checkout(&repo.older);
    let out = repo.dry_run(Some((&repo.older, "0.1.0")), &[]);
    let text = printed(&out);
    assert!(!out.status.success(), "the roll went ahead: {text}");
    assert!(text.contains("earlier staging commit"), "{text}");
    assert!(
        text.contains(&format!("{} (0.1.0)", &repo.older[..8]))
            && text.contains(&format!("{} (0.1.1)", &repo.head[..8])),
        "both commits and versions are named: {text}"
    );
    assert!(
        text.contains("git pull --ff-only"),
        "the fix is named: {text}"
    );
    assert!(
        !text.contains(PAST_GUARD),
        "stopped before the stacks: {text}"
    );
}

#[test]
pub fn allow_earlier_rolls_an_earlier_staging_commit_with_a_warning() {
    let repo = Repo::new();
    repo.checkout(&repo.older);
    let out = repo.dry_run(Some((&repo.head, "0.1.1")), &["--allow-earlier"]);
    let text = printed(&out);
    assert!(text.contains("because of --allow-earlier"), "{text}");
    assert!(
        text.contains(PAST_GUARD),
        "the guard let it through: {text}"
    );
}

#[test]
pub fn the_commit_testnet_already_runs_is_refused() {
    let repo = Repo::new();
    repo.checkout(&repo.head);
    let out = repo.dry_run(Some((&repo.head, "0.1.1")), &[]);
    let text = printed(&out);
    assert!(!out.status.success(), "the roll went ahead: {text}");
    assert!(
        text.contains(&format!("testnet already runs {} (0.1.1)", &repo.head[..8])),
        "{text}"
    );
    assert!(text.contains("--force"), "the override is named: {text}");
    assert!(
        !text.contains(PAST_GUARD),
        "stopped before the stacks: {text}"
    );
}

#[test]
pub fn force_redeploys_the_commit_testnet_already_runs() {
    let repo = Repo::new();
    repo.checkout(&repo.head);
    let out = repo.dry_run(Some((&repo.head, "0.1.1")), &["--force"]);
    let text = printed(&out);
    assert!(text.contains("because of --force"), "{text}");
    assert!(
        text.contains(PAST_GUARD),
        "the guard let it through: {text}"
    );
}

#[test]
pub fn the_roll_says_which_commit_it_moves_testnet_from_and_to() {
    let repo = Repo::new();
    repo.checkout(&repo.head);
    let out = repo.dry_run(Some((&repo.older, "0.1.0")), &[]);
    let text = printed(&out);
    assert!(
        text.contains(&format!(
            "rolling testnet from {} (0.1.0) to {} (0.1.1)",
            &repo.older[..8],
            &repo.head[..8]
        )),
        "{text}"
    );
    assert!(
        text.contains(PAST_GUARD),
        "the guard let it through: {text}"
    );
}
