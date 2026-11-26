// Tests for the pre-flight in scripts/roll_relays.sh (issue #548): every stack's change set is
// previewed before any stack changes, and one question covers all the stacks that need review.
//
// Each test builds a throwaway TestNet repository (an `origin` holding `staging` and `deployed`)
// with the real roll script and aws/lib/changeset.sh, and a stand-in aws/deploy_relay.sh that runs
// the real change-set flow (changeset_apply) against the fixtures in scripts/tests/fixtures/changeset
// instead of AWS. A stub `aws` on PATH answers the roll's own stack and log queries. Everything the
// roll does to a stack lands in a log: "preview <stack> <result>", "roll <stack> <result>",
// "execute <stack>", and "build" for the image build.
//
// The questions need a terminal, so a run that answers one goes through `script`, which gives the
// roll a pseudo-terminal and types the answers.
#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tempfile::TempDir;

const ACCOUNT: &str = "123456789012";
const RELAYS: [&str; 2] = ["relay-1", "relay-2"];

/// Answers the roll's own AWS calls: the account, the stacks (handle "h-<stack>", node file
/// /app/node.json, express) and the start log of this commit.
const STUB_AWS: &str = r#"#!/usr/bin/env bash
stack=""
args=("$@")
for ((i = 0; i < ${#args[@]}; i++)); do
  [[ "${args[i]}" == --stack-name ]] && stack=${args[i+1]}
done
case "$1 $2" in
  "sts get-caller-identity") echo "$STUB_ACCOUNT" ;;
  "cloudformation describe-stacks")
    case "$*" in
      *Handle*) echo "h-$stack" ;;
      *NodeFile*) echo /app/node.json ;;
      *InstanceType*) echo None ;;
    esac ;;
  "logs filter-log-events") echo "Bingle CLI starting. Version: 0.1.1 (git $STUB_SHA, built now)" ;;
esac
exit 0
"#;

/// A stand-in for aws/deploy_relay.sh: the real changeset_apply, with aws served from fixtures.
/// STUB_<stack>_preview and STUB_<stack>_roll (dashes as underscores) name each phase's change set;
/// the roll falls back to the preview's, and both default to image_only.json.
const STUB_DEPLOY: &str = r#"#!/usr/bin/env bash
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$here/lib/changeset.sh"
stack="" yes=0 dry=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --stack-name) stack=$2; shift 2 ;;
    --handle|--region|--tag|--relay-stack) shift 2 ;;
    --yes) yes=1; shift ;;
    --dry-run) dry=1; shift ;;
    *) shift ;;
  esac
done
key=${stack//-/_}
phase=roll
[[ $dry -eq 1 ]] && phase=preview
var="STUB_${key}_${phase}"
fixture=${!var:-}
if [[ -z "$fixture" ]]; then var="STUB_${key}_preview"; fixture=${!var:-image_only.json}; fi
aws() {
  case "$1 $2" in
    "cloudformation get-template-summary") cat "$STUB_FIXTURES/template_summary.json" ;;
    "cloudformation describe-stacks") cat "$STUB_FIXTURES/stack.json" ;;
    "cloudformation describe-change-set") cat "$STUB_FIXTURES/$fixture" ;;
    "cloudformation execute-change-set") echo "execute $stack" >>"$STUB_LOG" ;;
    "cloudformation wait") [[ "$3" != change-set-create-complete ]] || [[ "$fixture" != empty.json ]] ;;
  esac
}
CS_PREFIX="[deploy]"
changeset_apply "$stack" us-east-1 template.yaml ImageUri image "$yes" "$dry"
rc=$?
echo "$phase $stack $CS_RESULT" >>"$STUB_LOG"
exit $rc
"#;

const STUB_BUILD: &str = "#!/usr/bin/env bash\necho build >>\"$STUB_LOG\"\n";

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

fn write_exe(path: &Path, body: &str) {
    fs::write(path, body).expect("write script");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
}

struct Roll {
    dir: TempDir,
    sha: String,
    /// Fixture names per stack and phase, as (env var, fixture).
    fixtures: Vec<(String, String)>,
}

impl Roll {
    fn new() -> Roll {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
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

        for sub in ["scripts", "aws/lib", "bingle_core"] {
            fs::create_dir_all(work.join(sub)).expect("mkdir");
        }
        fs::copy(
            src.join("scripts/roll_relays.sh"),
            work.join("scripts/roll_relays.sh"),
        )
        .expect("copy roll script");
        fs::copy(
            src.join("aws/lib/changeset.sh"),
            work.join("aws/lib/changeset.sh"),
        )
        .expect("copy changeset.sh");
        write_exe(&work.join("aws/deploy_relay.sh"), STUB_DEPLOY);
        write_exe(&work.join("scripts/build_cli_image.sh"), STUB_BUILD);
        fs::write(
            work.join("bingle_core/Cargo.toml"),
            "[package]\nname = \"bingle_core\"\nversion = \"0.1.1\"\n",
        )
        .expect("write Cargo.toml");
        git(&work, &["add", "-A"]);
        git(&work, &["commit", "-q", "-m", "0.1.1"]);
        git(
            &work,
            &["push", "-q", "origin", "staging", "staging:deployed"],
        );
        let sha = git(&work, &["rev-parse", "HEAD"]);

        let stubs = dir.path().join("stubs");
        fs::create_dir_all(&stubs).expect("mkdir stubs");
        write_exe(&stubs.join("aws"), STUB_AWS);
        write_exe(&stubs.join("gh"), "#!/usr/bin/env bash\nexit 0\n");
        write_exe(&stubs.join("docker"), "#!/usr/bin/env bash\nexit 0\n");
        let mut inventory = format!("NETWORK=testnet\nACCOUNT_ID={ACCOUNT}\n");
        for stack in RELAYS {
            inventory.push_str(&format!(
                "RELAY=h-{stack} {stack} us-east-1 express /app/node.json\n"
            ));
        }
        fs::write(dir.path().join("relays.env"), inventory).expect("write inventory");
        fs::write(dir.path().join("log"), "").expect("write log");
        Roll {
            dir,
            sha,
            fixtures: Vec::new(),
        }
    }

    /// The change set `stack` shows in the pre-flight (and in the roll, unless `roll` overrides it).
    fn preview(mut self, stack: &str, fixture: &str) -> Roll {
        self.fixtures.push((
            format!("STUB_{}_preview", stack.replace('-', "_")),
            fixture.to_string(),
        ));
        self
    }

    /// The change set `stack` shows when it is really rolled.
    fn roll(mut self, stack: &str, fixture: &str) -> Roll {
        self.fixtures.push((
            format!("STUB_{}_roll", stack.replace('-', "_")),
            fixture.to_string(),
        ));
        self
    }

    /// Run the roll. With `answers`, it runs on a pseudo-terminal and they are typed in.
    fn run(&self, extra: &[&str], answers: Option<&str>) -> Output {
        let script = self.dir.path().join("work/scripts/roll_relays.sh");
        let mut roll_args = vec!["bash".to_string(), script.display().to_string()];
        roll_args.extend(extra.iter().map(|s| s.to_string()));
        let mut cmd = match answers {
            None => {
                let mut c = Command::new(&roll_args[0]);
                c.args(&roll_args[1..]);
                c
            }
            Some(_) if cfg!(target_os = "macos") => {
                let mut c = Command::new("script");
                c.args(["-q", "/dev/null"]).args(&roll_args);
                c
            }
            Some(_) => {
                let mut c = Command::new("script");
                c.args(["-qec", &roll_args.join(" "), "/dev/null"]);
                c
            }
        };
        let path = format!(
            "{}:{}",
            self.dir.path().join("stubs").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        cmd.current_dir(self.dir.path())
            .env("PATH", path)
            .env("BINGLE_NETWORK", "testnet")
            .env("BINGLE_RELAYS_FILE", self.dir.path().join("relays.env"))
            .env("BINGLE_NO_RECORD", "1")
            .env("START_TIMEOUT", "5")
            .env("STUB_ACCOUNT", ACCOUNT)
            .env("STUB_SHA", &self.sha)
            .env("STUB_LOG", self.dir.path().join("log"))
            .env(
                "STUB_FIXTURES",
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/tests/fixtures/changeset"),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &self.fixtures {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("run roll_relays.sh");
        let mut stdin = child.stdin.take().expect("stdin");
        let Some(answers) = answers else {
            drop(stdin);
            return child.wait_with_output().expect("wait for roll_relays.sh");
        };
        // Type each answer only once its prompt is on screen: typed ahead, it can be read (or
        // flushed with an end of file) before the roll asks.
        let mut stdout = child.stdout.take().expect("stdout");
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let reader = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = stdout.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        let mut seen = Vec::new();
        let mut asked = 0;
        for answer in answers.lines() {
            asked += 1;
            let deadline = Instant::now() + Duration::from_secs(60);
            while count(&String::from_utf8_lossy(&seen), "[y/N]") < asked {
                let left = deadline.saturating_duration_since(Instant::now());
                match rx.recv_timeout(left) {
                    Ok(chunk) => seen.extend(chunk),
                    // the roll finished (or stalled) without asking again
                    Err(_) => break,
                }
            }
            if stdin.write_all(format!("{answer}\n").as_bytes()).is_err() {
                break;
            }
        }
        // Keep the terminal open until the roll exits: closing it sends an end of file, which on
        // macOS can overtake the last answer. An unexpected extra question therefore hangs until
        // the test runner's timeout, and fails the test.
        let status = child.wait().expect("wait for roll_relays.sh");
        drop(stdin);
        reader.join().expect("join the output reader");
        seen.extend(rx.try_iter().flatten());
        let mut stderr = Vec::new();
        if let Some(mut e) = child.stderr.take() {
            e.read_to_end(&mut stderr).expect("read stderr");
        }
        Output {
            status,
            stdout: seen,
            stderr,
        }
    }

    fn log(&self) -> Vec<String> {
        fs::read_to_string(self.dir.path().join("log"))
            .expect("read log")
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn log_path(&self) -> PathBuf {
        self.dir.path().join("log")
    }
}

fn printed(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

const ONE_QUESTION: &str = "Roll all 2 stack(s), including these changes?";
const STACK_QUESTION: &str = "Execute this change set on";

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

#[test]
pub fn all_expected_changes_roll_without_a_question() {
    let roll = Roll::new();
    let out = roll.run(&[], None);
    let text = printed(&out);
    assert!(out.status.success(), "the roll failed: {text}");
    assert!(!text.contains(ONE_QUESTION), "{text}");
    assert_eq!(
        roll.log(),
        vec![
            "preview relay-1 dry-run",
            "preview relay-2 dry-run",
            "build",
            "execute relay-1",
            "roll relay-1 executed",
            "execute relay-2",
            "roll relay-2 executed",
        ],
        "every stack is previewed before the build and the roll"
    );
}

#[test]
pub fn one_question_covers_the_stacks_that_need_review() {
    let roll = Roll::new().preview("relay-2", "security_group_change.json");
    let out = roll.run(&[], Some("y\n"));
    let text = printed(&out);
    assert!(out.status.success(), "the roll failed: {text}");
    assert_eq!(count(&text, ONE_QUESTION), 1, "{text}");
    assert!(
        text.contains("1 of 2 stack(s) change more than the task definition and service"),
        "{text}"
    );
    assert_eq!(
        count(&text, STACK_QUESTION),
        0,
        "approved at the start, so not asked again: {text}"
    );
    assert!(
        text.contains("approved at the start of the roll; executing without asking again"),
        "{text}"
    );
    assert!(roll.log().contains(&"roll relay-2 executed".to_string()));
}

#[test]
pub fn refusing_the_question_changes_no_stack() {
    let roll = Roll::new().preview("relay-2", "security_group_change.json");
    let out = roll.run(&[], Some("n\n"));
    let text = printed(&out);
    assert!(!out.status.success(), "the roll went ahead: {text}");
    assert!(text.contains("not rolled; no stack was changed"), "{text}");
    assert_eq!(
        roll.log(),
        vec!["preview relay-1 dry-run", "preview relay-2 dry-run"],
        "nothing built or executed"
    );
}

#[test]
pub fn a_stack_that_cannot_be_previewed_stops_the_roll_before_any_change() {
    let roll = Roll::new().preview("relay-2", "failed.json");
    let out = roll.run(&[], None);
    let text = printed(&out);
    assert!(!out.status.success(), "the roll went ahead: {text}");
    assert!(
        text.contains("pre-flight: a change set could not be created; no stack was changed"),
        "{text}"
    );
    let log = roll.log();
    assert!(
        !log.iter()
            .any(|l| l == "build" || l.starts_with("execute") || l.starts_with("roll")),
        "nothing built or executed: {log:?}"
    );
}

#[test]
pub fn a_change_not_approved_at_the_start_asks_again_for_that_stack() {
    let roll = Roll::new()
        .preview("relay-2", "security_group_change.json")
        .roll("relay-2", "iam_change.json");
    // Yes at the start; no when relay-2's real change set differs.
    let out = roll.run(&[], Some("y\nn\n"));
    let text = printed(&out);
    assert!(!out.status.success(), "relay-2 was declined: {text}");
    assert_eq!(count(&text, ONE_QUESTION), 1, "{text}");
    assert!(
        text.contains("These differ from the changes approved at the start of the roll"),
        "{text}"
    );
    assert_eq!(count(&text, STACK_QUESTION), 1, "{text}");
    let log = roll.log();
    assert!(
        log.contains(&"roll relay-1 executed".to_string()),
        "{log:?}"
    );
    assert!(
        log.contains(&"roll relay-2 declined".to_string()),
        "{log:?}"
    );
    assert!(!log.contains(&"execute relay-2".to_string()), "{log:?}");
}

#[test]
pub fn yes_rolls_the_stacks_that_need_review_without_asking() {
    let roll = Roll::new().preview("relay-2", "security_group_change.json");
    let out = roll.run(&["--yes"], None);
    let text = printed(&out);
    assert!(out.status.success(), "the roll failed: {text}");
    assert!(!text.contains(ONE_QUESTION), "{text}");
    assert!(text.contains("rolling them because of --yes"), "{text}");
    assert!(roll.log().contains(&"roll relay-2 executed".to_string()));
}

#[test]
pub fn with_no_terminal_a_stack_that_needs_review_stops_before_any_change() {
    let roll = Roll::new().preview("relay-1", "security_group_change.json");
    let out = roll.run(&[], None);
    let text = printed(&out);
    assert!(!out.status.success(), "the roll went ahead: {text}");
    assert!(
        text.contains("need review and there is no terminal to ask on"),
        "{text}"
    );
    assert_eq!(
        roll.log(),
        vec!["preview relay-1 dry-run", "preview relay-2 dry-run"],
        "log at {}",
        roll.log_path().display()
    );
}

#[test]
pub fn a_dry_run_is_the_pre_flight_alone() {
    let roll = Roll::new().preview("relay-2", "security_group_change.json");
    let out = roll.run(&["--dry-run"], None);
    let text = printed(&out);
    assert!(out.status.success(), "the dry run failed: {text}");
    assert!(
        text.contains("dry run: a roll would ask once, here, before changing any stack"),
        "{text}"
    );
    assert_eq!(
        roll.log(),
        vec!["preview relay-1 dry-run", "preview relay-2 dry-run"]
    );
}
