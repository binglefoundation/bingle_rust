# CI: bingle_jsi end-to-end (e2e)

The `bingle_jsi` Detox e2e runs in CI on **Android**, on a Linux runner. It is separate from the
pull-request checks (`CI-UNIT-TESTS.md`) because it needs an emulator and is heavier.

| Workflow | Job | What it does |
| --- | --- | --- |
| `.github/workflows/e2e-android.yml` | `android-e2e` | builds the native lib + app, runs the Detox suites on a KVM Android emulator (issue #132) |

## Why Android on Linux (and not iOS)

The iOS simulator forces a macOS runner, and GitHub-hosted macOS runners have **no nested
virtualization** — so no Docker/localnet and no accelerated emulator (see #110). iOS e2e is therefore
run **manually before release** (it is green locally on testnet and localnet).

Linux runners (`ubuntu-latest`) have **KVM** (accelerated Android emulator) and **native Docker**
(`algokit localnet`), so the Android e2e runs hermetically in CI.

## What runs, and when

| Trigger | Event |
| --- | --- |
| Push to `staging` | `push` (post-merge smoke run) |
| Manual | `workflow_dispatch` |

Not on pull requests: emulator runs are heavy, so this is a post-merge check on `staging`. A
`concurrency` group with `cancel-in-progress` means a newer push cancels the older in-flight run
("heap not queue").

## What the job does

1. Installs JDK 17, the Rust toolchain + Android targets, Node, the Android SDK/NDK, and `ktlint`.
2. Starts `algokit localnet` (native Docker) — the smoke suite's default `init` queries the chain at
   `localhost:4001`, which Detox adb-reverses to the runner host.
3. Builds the Android native library (`build_android.sh`) and the Detox app + `androidTest` APKs.
4. Boots a KVM-accelerated x86_64 emulator (`reactivecircus/android-emulator-runner`) and runs
   `bingle_jsi/example/scripts/ci_run_android_e2e.sh`: harden the emulator, stage the testnet
   backend, Metro headless + pre-warm, then `detox test --headless`.
5. Uploads Detox artifacts on failure.

A 60-minute `timeout-minutes` bounds the run.

## Caching

To cut wall-clock time the job caches build inputs across runs (issue #146). Because e2e-android
runs only on `staging` pushes + manual dispatch, and `staging` is the repo's default branch, these
caches are populated by staging runs and reused by the next one.

- **Gradle** (`gradle/actions/setup-gradle`, step "Cache Gradle") — caches the Gradle User Home
  (`~/.gradle/caches`, wrapper, configuration cache) so the "Build the app + androidTest APKs"
  step doesn't redownload dependencies or re-run configuration. `cache-read-only: false` is set so
  staging runs write the cache. **Busts** automatically on Gradle wrapper / build-file changes
  (the action keys on those); to force a miss, bump the Gradle wrapper version or clear the repo's
  Actions caches (Settings → Actions → Caches, or `gh cache delete`).

## Suites

- **smoke** runs in CI (launch, `version()`, keypair round-trip against the in-CI localnet reached
  via adb-reverse).
- **messaging** and **failure_causes** do **not** run in CI. They need a real network, and the CI
  Android emulator has **no outbound internet** (verified: no route to `8.8.8.8`), so the testnet
  path is impossible here. Hermetic network coverage requires the localnet emulator-mode provisioner
  (host services over `10.0.2.2`, no internet) — follow-up **#37**. Run them **locally** instead
  (`run_e2e_android.sh` on an emulator with internet; the Android HTTPS fix is #135):

      BINGLE_E2E_PASSPHRASE="word word … word" BINGLE_E2E_HANDLE=my-handle bash bingle_jsi/example/scripts/run_e2e_android.sh

## Run it manually

Dispatch on any branch that has the workflow (add the branch to the `push` trigger, or use the
Actions UI / `gh workflow run e2e-android.yml --ref <branch>` once it is on the default branch).

## Reproduce locally

With an Android emulator + JDK 17:

    BINGLE_E2E_PASSPHRASE="word word … word" BINGLE_E2E_HANDLE=my-handle bash bingle_jsi/example/scripts/run_e2e_android.sh

The smoke suite needs only a running `algokit localnet` (Detox adb-reverses `4001`/`8980`); the
network suites need the testnet sender above. iOS uses `run_e2e_ios.sh` (see `bingle_jsi/README.md`).

## Follow-ups

- Hermetic **localnet** for the Android network suites (#37): the emulator reaches the host only over
  `10.0.2.2` and Bingle's transport is UDP (adb-reverse is TCP-only), so it needs an "emulator mode"
  provisioner that advertises `10.0.2.2` and forces relay use.
