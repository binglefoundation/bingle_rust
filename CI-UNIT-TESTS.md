# CI: pull-request checks

Two GitHub Actions workflows run whenever a PR is raised or amended, and on
pushes to `master`. Each defines one required status check:

| Workflow | Job / check | What it enforces |
| --- | --- | --- |
| `.github/workflows/unit-tests.yml` | `unit` | workspace unit tests pass (nextest) |
| `.github/workflows/quality-checks.yml` | `quality` | `scripts/run_quality_checks.sh --strict`: rustfmt clean + clippy `-D warnings` |

The sections below describe the unit-tests workflow; the quality-checks
workflow is covered under [Quality checks](#quality-checks). Branch protection
(below) requires **both** checks.

## What runs, and when

| Trigger | Event |
| --- | --- |
| PR opened | `pull_request: opened` |
| PR amended (new commits pushed) | `pull_request: synchronize` |
| PR reopened | `pull_request: reopened` |
| Draft PR marked ready | `pull_request: ready_for_review` |
| Push to `master` | `push` (post-merge sanity run) |

Draft PRs are skipped and run automatically once marked ready. A
`concurrency` group cancels the previous run when you push more commits, so an
amended PR is always graded on its latest state.

## What the job does

1. Checks out the code and installs the stable Rust toolchain (needed for
   `edition = "2024"`). OpenSSL is vendored, so it compiles from source — no
   system packages required.
2. Caches the cargo registry and `target/` (`Swatinem/rust-cache`).
3. Installs `cargo-nextest`.
4. **Builds** the `unit` test target for `bingle_core`, `bingle_webserver`,
   `bingle_local`, `bingle_jsi` with `--locked`. A compile break here fails the
   job — an *incomplete* run counts as a failure and blocks merge.
5. **Runs** `cargo nextest run --profile ci --test unit` across those crates.
   The `ci` profile (from `.config/nextest.toml`) gives a 20s per-test timeout,
   one retry, and `junit.xml`.
6. Publishes a pass/fail table onto the PR (`dorny/test-reporter`), even on
   failure.

`RUSTFLAGS: -D warnings` turns any compiler warning into a failure, matching the
"all tests pass with no warnings" rule in `CLAUDE.md`.

The job is named **`unit`** — that is the status-check name you require in
branch protection.

## Make it block merge (require a passing run)

The workflow reports status; **GitHub branch protection** is what actually
blocks the merge button. Configure it once against `master`.

### Option A — GitHub UI

1. Repo → **Settings** → **Branches** → **Add branch ruleset** (or **Add rule**
   under "Branch protection rules").
2. Branch name pattern: `master`.
3. Enable **Require status checks to pass before merging**.
4. Enable **Require branches to be up to date before merging** (optional but
   recommended — forces a re-run against the latest `master`).
5. In the search box, add the checks named **`unit`** and **`quality`**.
   - A check only appears in that list after its workflow has run at least
     once, so open a throwaway PR (or push these workflows) first.
6. Save.

An in-progress run leaves the check *pending*, which also keeps merge blocked
until it completes — so "incomplete" blocks merge too.

### Option B — `gh` CLI

Run once after the workflow has appeared at least once:

```bash
gh api -X PUT repos/bingle-foundation/bingle_rust/branches/master/protection \
  -H "Accept: application/vnd.github+json" \
  -f 'required_status_checks[strict]=true' \
  -f 'required_status_checks[checks][][context]=unit' \
  -f 'required_status_checks[checks][][context]=quality' \
  -f 'enforce_admins=false' \
  -f 'required_pull_request_reviews[required_approving_review_count]=0' \
  -F 'restrictions='
```

`enforce_admins=false` is what lets
you **override / merge anyway** (see below); set it to `true` if you ever want
the block to apply to admins as well.

## Overriding the block when you need to

Because `enforce_admins` is left off, a repo admin can still merge a PR whose
`unit` check failed or is incomplete:

- **UI:** the merge button shows a warning and an admin sees a
  **"Merge without waiting for requirements to be met (bypass rules)"** option.
- **CLI:** `gh pr merge <number> --admin --squash` (the `--admin` flag bypasses
  the failing required check).

Regular contributors without admin/bypass rights cannot merge until `unit`
passes.

## Quality checks

`.github/workflows/quality-checks.yml` runs `scripts/run_quality_checks.sh
--strict` on the same PR/push triggers, as the job named **`quality`**. Strict
mode is a gate: rustfmt must report no drift and clippy must pass with
`-D warnings`, so the script exits non-zero (failing the job, blocking merge)
on any formatting or lint regression.

The large clippy backlog is silenced centrally in `[workspace.lints.clippy]`
(root `Cargo.toml`, tracked by issues #33–#39), so `--strict` only fails on
*new* lints outside that backlog and on formatting drift. Override / bypass
works exactly as for the `unit` check above (admin merge).

The separate **`typecheck-example`** job (`tsc --noEmit` for the bingle_jsi
example) caches the npm download cache via `actions/setup-node` (`cache: npm`,
keyed on `bingle_jsi/example/package-lock.json`) so `npm install` doesn't
refetch the dependency tree each run (issue #150). It busts on a
`package-lock.json` change.

Fix locally before pushing:

```bash
scripts/run_quality_checks.sh            # report only (never fails)
scripts/run_quality_checks.sh --strict   # same gate CI runs
scripts/run_quality_checks.sh fix        # auto-apply cargo fmt + clippy --fix
scripts/run_quality_checks.sh --detail   # full fmt diff + clippy output
```

## Local equivalent

The same tests run locally via:

```bash
scripts/run_unit_tests.sh            # plain cargo test, per-test timeout
scripts/run_unit_tests_nextest.sh    # nextest equivalent
```
