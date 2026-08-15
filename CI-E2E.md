# CI: bingle_jsi localnet end-to-end (e2e)

The `bingle_jsi` Detox e2e runs in CI against a localnet booted inside the
workflow, so a push to `staging` is smoke-tested end to end. It is a separate
workflow from the pull-request checks (see `CI-UNIT-TESTS.md`) because it needs a
macOS runner and is far heavier.

| Workflow | Job | What it does |
| --- | --- | --- |
| `.github/workflows/e2e-ios.yml` | `localnet-provision` | boots a localnet on macOS and provisions it headless (issue #127) |

This is delivered in two stages under issue #110: the current stage (#127) proves
the localnet bring-up; a follow-up (#128) adds the iOS simulator + the three
Detox suites to the same job.

## Why macOS + Colima

The iOS simulator forces a **macOS runner**, and GitHub-hosted macOS runners have
**no Docker daemon**. `algokit localnet` is Docker Compose, so the workflow brings
Docker up via **Colima** (Virtualization.framework on Apple silicon) on the same
runner. Everything is co-located on one host because the localnet provisioner
(`bingle_test`'s `localnet_e2e_provisioner`) hosts in-process relays + an echo
peer on loopback that the simulator reaches via the host's `127.0.0.1`, and it
talks to algod/indexer at `localhost:4001/8980`.

## What runs, and when

| Trigger | Event |
| --- | --- |
| Push to `staging` | `push` (post-merge smoke run) |
| Manual | `workflow_dispatch` |

It does **not** run on pull requests: macOS runner time is a limited public-repo
resource, so this is a post-merge check on `staging`, not a PR gate. A
`concurrency` group with `cancel-in-progress` means a newer push to `staging`
cancels the older in-flight run ("heap not queue") — `staging` is always graded on
its most recent push.

## What the job does

1. Checks out the code and installs the stable Rust toolchain; caches cargo
   (`Swatinem/rust-cache`).
2. Installs `algokit` (via `pipx`).
3. Brings Docker up with Colima and points `DOCKER_HOST` at the Colima socket.
4. `algokit localnet start` — pulls the algod/indexer images and waits until they
   are healthy.
5. Builds `localnet_e2e_provisioner` (`bingle_test`, `localnet` feature).
6. Runs the provisioner in one-shot mode (`BINGLE_E2E_PROVISION_ONESHOT=1`): it
   deploys the app + asset, brings up relays/STUN/echo peer, registers the sender
   and offline fixtures, writes the readiness env file, and exits 0. The job
   asserts that `/tmp/bingle_e2e_localnet.env` was written with the expected
   `BINGLE_E2E_*` keys — which only happens after the relays reach Available and
   the handles are indexer-visible.
7. Tears down localnet + Colima (`if: always()`).

A 40-minute `timeout-minutes` bounds the run; image-pull time dominates.

## Run it manually

Dispatch on any branch that contains the workflow file (handy for iterating
before merge to `staging`):

    gh workflow run e2e-ios.yml --ref <branch>

Or use the **Actions → e2e-ios → Run workflow** button in the GitHub UI.

## Reproduce locally

With `algokit localnet` running and the `algokit`/`goal` CLIs on `PATH`:

    BINGLE_E2E_PROVISION_ONESHOT=1 cargo run -p bingle_test --features localnet --bin localnet_e2e_provisioner

The full simulator e2e (added in #128) is driven by
`bingle_jsi/example/scripts/run_e2e_ios.sh` (see `bingle_jsi/README.md`).
