# CI: integration (localnet) tests

The `integration` suite runs in CI against a real, in-CI **algokit localnet** on a Linux runner
(issue #136). It is separate from the pull-request checks (`CI-UNIT-TESTS.md`) because it needs
Docker/localnet and is heavier, and from the Android e2e (`CI-E2E.md`) though it reuses the same
localnet + puya-compile tooling (issue #32).

| Workflow | Job | What it does |
| --- | --- | --- |
| `.github/workflows/integration-tests.yml` | `integration` | starts algokit localnet, compiles the dapp, runs the `integration` test target for `bingle_core` + `bingle_cli` |

## Why Linux (and post-merge)

The tests deploy the BingleDapp, register handles/relays on-chain, and drive chain + transport end to
end, so they need `algokit localnet` (Docker). `ubuntu-latest` has **native Docker** (unlike
GitHub-hosted macOS runners — see #110), so localnet runs hermetically in CI. The suite is heavy and
needs a live localnet, so it runs **post-merge on `staging`**, not on every pull request.

## What runs, and when

| Trigger | Event |
| --- | --- |
| Push to `staging` | `push` (post-merge run) |
| Manual | `workflow_dispatch` |

A `concurrency` group with `cancel-in-progress` means a newer push to `staging` cancels the older
in-flight run rather than queueing behind it ("heap not queue", issue #136).

## What the job does

1. Checks out the code and installs the stable Rust toolchain (`edition = "2024"`). OpenSSL is
   vendored, so it compiles from source — no system packages required.
2. Caches the cargo registry and `target/` (`Swatinem/rust-cache`) and installs `cargo-nextest`.
3. **Starts `algokit localnet`** (native Docker) and waits until algod (`localhost:4001`) and the
   indexer (`8980`) are reachable. This wait is a real gate: the tests probe algod and *skip cleanly*
   when it is down, so without it a broken localnet would silently pass.
4. **Compiles the dapp artifacts** with puya (`dapp_projects`) — the harness deploys the BingleDapp
   from its `.approval.teal` / `.clear.teal` / `.arc56.json`, which are gitignored build outputs. Same
   puya invocation as e2e-android.
5. **Builds** the `integration` test target for `bingle_core` + `bingle_cli` with `--locked`. A
   compile break here fails the job.
6. **Runs** `cargo nextest run --profile integration --test integration` across those crates.
7. Publishes a pass/fail table (`dorny/test-reporter`), even on failure.

A 45-minute `timeout-minutes` bounds the run. `RUSTFLAGS: -D warnings` turns any compiler warning into
a failure, matching the "no warnings" rule in `CLAUDE.md`.

## The `integration` nextest profile

`[profile.integration]` in `.config/nextest.toml` runs the suite **single-threaded**
(`test-threads = 1`). The cases share one localnet and its fixed test accounts, so they must not run
concurrently; `serial_test`'s `#[serial]` only locks *within* a process, which cannot serialize
nextest's process-per-test model — so nextest itself must run one test process at a time. The profile
also gives a generous `slow-timeout` (300s period, hard kill at 600s) because chain deploys and
indexer waits are slow, and sets `retries = 0`: several cases mutate on-chain state (e.g. handle
registration), so a retry would hit a collision rather than a clean rerun.

## Which tests

The `bingle_core` `integration` target (`tests/integration_all.rs`) covers the localnet blockchain,
API, engine, and relay cases plus the live-internet STUN case; the `bingle_cli` `integration` target
(`tests/integration.rs`) drives the compiled `chat` binary over localnet. The `integration_blockchain`
target is a Docker-run subset of the same cases and is not run here.

## Reproduce locally

With `algokit` installed and localnet up:

```bash
scripts/run_localnet_tests.sh        # starts localnet if needed, runs the localnet cases
```

Or directly against a running localnet:

```bash
cargo nextest run --profile integration -p bingle_core -p bingle_cli --test integration
```

The dapp artifacts must exist first (`scripts/build_dapp.sh`, or the puya compile in the workflow).
