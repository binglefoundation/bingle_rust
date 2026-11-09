# Releasing

How a change gets from a pull request to TestNet, to crates.io and npm, and to MainNet (issue #399). The flow follows the sidewinder repository's.

| Stage | What it is | How it moves |
| --- | --- | --- |
| `staging` | the default branch: every PR merges here; it carries the next, unreleased version | PR merge |
| TestNet relays | the TestNet relays an operator runs | on demand, from a `staging` commit: `scripts/roll_relays.sh` |
| `deployed`, crates.io, npm | the released commit and the packages built from it | only by a release: `scripts/release.sh` fast-forwards `deployed`, and `deploy.yml` publishes |
| MainNet relays | the MainNet relays an operator runs | after a release: `scripts/roll_relays.sh` on MainNet, from the release tag |

**One version names one commit.** The crates, the npm module, the tag `vX.Y.Z` and the MainNet relays all come from the same `staging` commit, and it is the one TestNet ran.

## How to

### Get a change onto `staging`

1. Branch from `staging` and open a PR against it. A ready PR on any other base fails `base-is-staging`.
2. The PR must pass `quality` and `unit` (see `CI-UNIT-TESTS.md`).
3. The merge reruns them and starts `integration` (`CI-INTEGRATION.md`) and `android-e2e` (`CI-E2E.md`), which do not run on PRs. Check they passed: `gh run list --branch staging --limit 8`.

A merge does not roll TestNet, publish anything or bump the version.

### Roll TestNet

Your relays are listed in an inventory file outside this repository (format: `aws/relays.example.env`). From a clean checkout of the head of `staging`:

```bash
source ~/.config/bingle/bingle_env.sh testnet
```

```bash
scripts/roll_relays.sh --dry-run
```

```bash
scripts/roll_relays.sh
```

`bingle_env.sh` is the maintainers' own environment script: it signs in to the network's AWS account and sets `BINGLE_NETWORK` and `BINGLE_RELAYS_FILE`. Any other way of setting those works. The roll records a successful `testnet` deployment of the commit on GitHub, and that record is what a release checks. Details: `DEVELOPER.md`, "Rolling every relay of a network".

### Release

Roll TestNet onto the commit first (above). Then, from any checkout, signed in to `gh` with push access:

```bash
scripts/release.sh --dry-run
```

```bash
scripts/release.sh
```

It checks the commit, fast-forwards `deployed` to it, waits for `deploy.yml` to publish the crates and the npm module and tag `vX.Y.Z`, then pushes the next patch version onto `staging`. Re-running it resumes a release that stopped part way. It holds no registry or AWS credential.

#### Release notes

Every release gets notes and a GitHub Release (issue #432):

1. **Written.** `release.sh` starts by running `scripts/release_notes.sh`. That script gathers the pull requests (PRs) merged since the previous `v*` tag and has Claude summarise them for a semi-technical reader. It runs `claude -p` locally, with no tools, using the prompt in `scripts/release_notes_prompt.md`. GitHub's list of those PRs follows the summary. Without the `claude` command, or if it fails, the notes are that list alone. It takes about a minute.
2. **Accepted.** `release.sh` prints the notes and asks you to accept, edit (in `$VISUAL` or `$EDITOR`), regenerate or quit. `--yes` accepts them as written. They are kept in `tmp/release_notes_vX.Y.Z.md`, so the notes from a `--dry-run` carry over to the real run on the same commit; edit that file to change them.
3. **Handed over.** After you confirm the promotion, `release.sh` pushes the notes as a git note on the release commit (`refs/notes/release`), then moves `deployed`. A note doesn't tie the version to the commit the way a tag does, so a release that fails to publish leaves no tag behind. An `--allow-untested` reason goes in the same note, as its last line, and the release guard reads it there.
4. **Tagged and released.** Only once `deploy.yml` has published the crates and the npm module does its `tag` job tag `vX.Y.Z`, with the notes as the tag's message. It then makes the tag a GitHub Release with the same notes. A release with no note gets GitHub's generated list instead.
5. **Kept.** The `chore: open X.Y.Z` commit also adds `release_notes/vX.Y.Z.md` on `staging`.

To try the notes on their own: `scripts/release_notes.sh origin/staging vX.Y.Z tmp/notes.md`.

### Roll MainNet

After the release, from a checkout of `deployed`:

```bash
git worktree add tmp/deployed origin/deployed
```

```bash
cd tmp/deployed && source ~/.config/bingle/bingle_env.sh mainnet && scripts/roll_relays.sh --dry-run
```

Then run `scripts/roll_relays.sh` there without `--dry-run`. MainNet rolls only the commit `deployed` points at, and only when its `vX.Y.Z` tag names it. Remove the worktree afterwards with `git worktree remove tmp/deployed`.

### Pick up the release in bingle_ux_rn

Move the app's version and its `react-native-bingle-jsi` dependency to the released version, once npm serves it (`npm view react-native-bingle-jsi version`).

## What is published

| Package | Registry | What it is |
| --- | --- | --- |
| `bingle_core` | crates.io | the protocol engine |
| `bingle_local` | crates.io | local and relay support |
| `bingle_cli` | crates.io | the `bingle_cli` binary (`cargo install bingle_cli`) |
| `react-native-bingle-jsi` | npm | the React Native module, with the iOS XCFramework and the Android libraries built in |

`bingle_jsi`, `bingle_webserver` and `bingle_test` are not published to crates.io.

## Versioning

Every crate and `bingle_jsi/package.json` carry one version. `scripts/bump_version.sh` sets it everywhere: each crate's `[package] version`, the version pins on the workspace's path dependencies, the npm module, the `bingle_core = "X.Y.Z"` and `bingle_local = "X.Y.Z"` install examples in those crates' READMEs (which crates.io shows), and the lockfile. `scripts/bump_version.sh --check` fails unless they all agree.

`staging` always carries the next, unreleased version. A release does not bump anything: it releases the version the commit already carries, and then `scripts/release.sh` commits `chore: open X.Y.(Z+1)` straight onto `staging`. That is the one commit that reaches `staging` without a pull request, because it is mechanical; the push skips `staging`'s required checks, which only a repository admin may do (one-time setup, below). `--next-minor` or `--next-major` open a different version; one can also be set by an ordinary PR before the next roll.

## `scripts/release.sh`

What one run does:

1. **Picks the commit:** the head of `origin/staging`, or `--commit <sha>`.
2. **Runs the release guard** (below) on it.
3. **Checks its staging CI:** `quality`, `unit`, `integration` and `android-e2e` must each have succeeded on the commit. A newer push to `staging` cancels an older commit's `integration` and `android-e2e` runs, so releasing the head of `staging` avoids a commit with no result.
4. **Promotes:** after a confirmation (`--yes` skips it), pushes the commit to `deployed`. The push is refused unless it is a fast-forward, so nothing is merged and `deployed` gains no commit of its own.
5. **Publishes:** the push starts `deploy.yml`, and the script waits for it.
6. **Opens the next version** on `staging`, unless `staging` has already moved past the released version.
7. **Prints the follow-ups:** roll MainNet, and the bingle_ux_rn pickup.

Every step is skipped when already done, so re-running the same command resumes. If `deploy.yml` failed, fix the cause, run `gh run rerun <run id> --failed`, then re-run the command.

**Releasing untested (an urgent fix only):** `scripts/release.sh --allow-untested "<why this cannot wait for TestNet>"` first pushes the release note on the commit with an `Allow-untested:` line (an older release's tag with that line still counts), which the guard accepts in place of a `testnet` deployment and prints as a warning. Roll TestNet onto the same commit afterwards, so both networks run it.

## The publish pipeline (`deploy.yml`)

A push to `deployed` runs `.github/workflows/deploy.yml`. It publishes with **no registry token stored anywhere**: crates.io and npm Trusted Publishing each swap the job's GitHub OpenID Connect (OIDC) identity for a short-lived credential.

| Job | What it does |
| --- | --- |
| `guard` | `scripts/release_guard.sh` on the pushed commit |
| `quality`, `unit`, `integration`, `android-e2e` | the same workflows that run on `staging`, called on the release commit, so the gates cannot drift from them |
| `native` | on `macos-latest`, builds the iOS XCFramework and the Android libraries (`bingle_jsi/scripts/build_ios.sh`, `build_android.sh`), each ending in the leak scan (`scripts/scan_native_leaks.sh`) |
| `publish` | the checks and dry runs (`scripts/check_fresh_resolution.sh`, `scripts/check_no_test_hooks_in_release.sh`, `DRY_RUN=1 scripts/publish_crates.sh`, `DRY_RUN=1 scripts/publish_npm.sh`), then the crates through crates.io Trusted Publishing and the npm module through npm Trusted Publishing, with provenance |
| `tag` | tags the commit `vX.Y.Z`, unless the release already pushed that tag |

**Why the native build runs on macOS.** The iOS slices are cross-compiled, but their C dependencies (vendored OpenSSL, `ring`) compile against Apple's iOS SDK, which ships only with Xcode and is licensed for Apple hardware only, and the packaging needs `lipo` and `xcodebuild -create-xcframework`. The runner image also carries the Android SDK and NDK, so both builds run there, as they do on a maintainer's Mac. This is a build only: the iOS tests still run on a local Mac.

**Rehearsing.** Actions, `deploy`, "Run workflow" on any branch with **rehearsal** set runs every gate, the native build and every dry run, and publishes and tags nothing. Use it to prove a change to the pipeline or the native build before a release.

**Re-running after a failure.** `scripts/publish_crates.sh` publishes one crate at a time in dependency order, and skips a crate crates.io already has at this version; `scripts/publish_npm.sh` skips a version npm already has. A run that stopped part way is finished with `gh run rerun <run id> --failed`, or with `gh workflow run deploy.yml --ref deployed`. No version bump is needed, and nothing is published twice.

### The release guard

`deployed` has no pull requests, so `scripts/release_guard.sh <sha>` checks the commit itself. It passes only when the commit:

1. **is on `staging`**, so it went through the pull-request gates;
2. **carries one version everywhere, whose tag, if it exists, names this same commit**, so one version never names two commits;
3. **had a successful deployment in the `testnet` GitHub environment** (from `scripts/roll_relays.sh`), unless the release note on the commit (or an existing release tag) carries an `Allow-untested:` line.

Run it locally before releasing by hand; it needs `gh` and the tags fetched.

## Rolling back

A published version is permanent: crates.io allows only a yank (`cargo yank --version X.Y.Z bingle_core`), and npm allows `npm deprecate`. To take MainNet back, roll it onto an earlier release: check out that release's tag in a worktree and run `aws/deploy_relay.sh --redeploy-only --handle <handle> --stack-name <stack> --region <region> --tag X.Y.Z` for each relay. `roll_relays.sh` refuses any commit but `deployed`, so a rollback is a deliberate hand step. The next release moves MainNet forward again.

## One-time setup

Done once, by a repository, crates.io and npm owner:

1. **Move `deployed` onto `staging`** (below), so that a fast-forward is possible.
2. **crates.io Trusted Publishing**, for each of `bingle_core`, `bingle_local` and `bingle_cli`: crates.io, the crate, Settings, Trusted Publishing, Add: owner `binglefoundation`, repository `bingle_rust`, workflow `deploy.yml`, no environment.
3. **npm Trusted Publishing** for `react-native-bingle-jsi`: npmjs.com, the package, Settings, Trusted Publisher, GitHub Actions: organization `binglefoundation`, repository `bingle_rust`, workflow `deploy.yml`, no environment, and tick **Allow npm publish** (without it only `npm stage publish` is allowed, and the publish fails with `403 ... OIDC permission denied for this action`).
4. **GitHub Environments** `testnet` and `mainnet` (Settings, Environments). `roll_relays.sh` records deployments in them; no protection rules are needed.
5. **Protect `deployed`:** no force pushes, no deletion, and require the `quality`, `unit`, `integration` and `android-e2e` checks, which ran on the commit when it reached `staging`: `gh api -X PUT repos/binglefoundation/bingle_rust/branches/deployed/protection --input - <<<'{"required_status_checks":{"strict":false,"contexts":["quality","unit","integration","android-e2e"]},"enforce_admins":false,"required_pull_request_reviews":null,"restrictions":null,"allow_force_pushes":false,"allow_deletions":false}'`
6. **Require `base-is-staging`** on `staging`, alongside `quality` and `unit`, leaving **Do not allow bypassing the above settings** off. Whoever runs `scripts/release.sh` must be a repository **admin**: the checks are enforced for everyone else, so a maintainer's push of `chore: open X.Y.Z` is refused with `protected branch hook declined`.
7. **Rehearse** `deploy.yml` on `staging` (above) before the first real release.
8. **After the first release through the pipeline:** revoke the local crates.io token (crates.io, Account Settings, API Tokens; then `cargo logout`), revoke any npm automation token, and set the npm package's publishing access to require two-factor authentication and disallow tokens.

### Moving `deployed` onto `staging`

Before this pipeline, a release merged `staging` into `deployed` and committed its own `chore: release vX` bump there, so `deployed` holds commits that are not on `staging`, and cannot fast-forward to a `staging` commit. Once, merge `deployed` into `staging` through a PR, keeping `staging`'s content, so that `deployed` becomes an ancestor of `staging`:

```bash
git fetch origin && git switch -c chore/merge-deployed origin/staging && git merge -s ours --no-edit origin/deployed
```

Then set `staging`'s version past the last release in the same PR (`scripts/bump_version.sh <next>`), and open it against `staging`.

## Manual fallback

Only if the pipeline cannot run (Trusted Publishing down, say). It needs a crates.io token scoped to `publish-update` on the three crates, and npm signed in; use short-lived credentials and revoke them straight afterwards.

1. Check out the release commit, clean, and check it: `scripts/release_guard.sh <sha>`.
2. Build the native libraries on a Mac: `bash bingle_jsi/scripts/build_ios.sh && bash bingle_jsi/scripts/build_android.sh`.
3. Run the checks and dry runs: `scripts/check_fresh_resolution.sh && scripts/check_no_test_hooks_in_release.sh && DRY_RUN=1 scripts/publish_crates.sh && DRY_RUN=1 scripts/publish_npm.sh`.
4. Publish: `scripts/publish_crates.sh`, then `(cd bingle_jsi && npm publish --access public)` (a hand publish has no provenance, so `publish_npm.sh` is not used).
5. Fast-forward `deployed` and tag: `git push origin <sha>:refs/heads/deployed` (this starts `deploy.yml`, which finds everything published and only tags), or tag by hand with `git tag -a vX.Y.Z <sha> -m "release vX.Y.Z" && git push origin vX.Y.Z`.
6. Open the next version on `staging`: `scripts/bump_version.sh --next-patch`, commit `chore: open X.Y.(Z+1)`, push to `staging` (as a repository admin).
