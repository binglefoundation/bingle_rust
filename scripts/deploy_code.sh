#!/usr/bin/env bash
# Cut a release: bump every crate + the npm module to the given version, build,
# and publish the crates to crates.io and the module to npm.
#
#   scripts/deploy_code.sh <version>            e.g. scripts/deploy_code.sh 0.2.2
#   scripts/deploy_code.sh --dry-run <version>  validate + dry-run only, publish nothing
#   scripts/deploy_code.sh --skip-native-build <version>
#   scripts/deploy_code.sh --allow-behind-staging <version>
#         deploy even if `deployed` is missing commits that are on `staging`
#         (use when intentionally cutting a release from an earlier point).
#   scripts/deploy_code.sh --skip-resolution-check <version>
#         skip the from-scratch dependency-resolution build check (not advised).
#   scripts/deploy_code.sh --publish-only <version>
#         skip the compile / resolution / native-build / dry-run gates and jump
#         straight to publishing. Use to resume a release whose build already
#         passed — e.g. to retry just the npm step after a missed 2FA window.
#   scripts/deploy_code.sh --clean <version>
#         on success, reclaim the build cache (workspace target/ + the JSI
#         multi-arch native caches) via scripts/clean_build_cache.sh. The next
#         build rebuilds from scratch. Never runs under --dry-run or on failure.
#
#   NPM_TOKEN=<automation-token>  publish to npm non-interactively (bypasses npm
#                                 2FA), so an unattended release needs no browser
#                                 click after the long native build. Create one at
#                                 npmjs.com > Access Tokens > Generate > Automation.
#
# Only runs on the `deployed` branch. Publishing to crates.io and npm is
# irreversible and cannot be made truly transactional, so the script front-loads
# every check that can fail cheaply — auth, a clean build, and `--dry-run`
# packaging of all artifacts — and only starts the real, irreversible pushes once
# those all pass. If a push fails partway, already-published versions are
# detected and skipped, so re-running the same command resumes where it stopped.
set -euo pipefail

# ── configuration ─────────────────────────────────────────────────────
BRANCH="deployed"
STAGING_BRANCH="staging"   # every commit here must be in `deployed` before a release (unless overridden)
PUBLISH_CRATES=(bingle_core bingle_local bingle_cli)   # crates.io, in dependency order (bingle_cli depends on the other two, so it publishes last)
NPM_DIR="bingle_jsi"
NPM_PKG="react-native-bingle-jsi"
BUMP_FILES=(
  bingle_core/Cargo.toml bingle_local/Cargo.toml bingle_jsi/Cargo.toml
  bingle_webserver/Cargo.toml bingle_test/Cargo.toml bingle_cli/Cargo.toml
  bingle_jsi/package.json Cargo.lock
)

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ── logging ───────────────────────────────────────────────────────────
info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
ok()   { printf '    \033[1;32mok\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarn:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# ── argument parsing ──────────────────────────────────────────────────
DRY_RUN_ONLY=0
SKIP_NATIVE=0
ALLOW_BEHIND_STAGING=0
SKIP_RESOLUTION_CHECK=0
PUBLISH_ONLY=0
CLEAN_AFTER=0
VERSION=""
for arg in "$@"; do
  case "$arg" in
    --dry-run)              DRY_RUN_ONLY=1 ;;
    --skip-native-build)    SKIP_NATIVE=1 ;;
    --allow-behind-staging) ALLOW_BEHIND_STAGING=1 ;;
    --skip-resolution-check) SKIP_RESOLUTION_CHECK=1 ;;
    --publish-only)         PUBLISH_ONLY=1 ;;
    --clean)                CLEAN_AFTER=1 ;;
    -h|--help)              sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*)                     die "unknown option: $arg" ;;
    *)                      [[ -n "$VERSION" ]] && die "unexpected extra argument: $arg"; VERSION="$arg" ;;
  esac
done
[[ -n "$VERSION" ]] || die "usage: scripts/deploy_code.sh [--dry-run] [--skip-native-build] [--publish-only] <version>"
# --publish-only resumes publishing a release whose build already passed, so it turns
# off every build/verify gate (they cannot fail cheaply and were already run). It is
# incompatible with --dry-run, which exists to run those gates and publish nothing.
if [[ $PUBLISH_ONLY -eq 1 ]]; then
  [[ $DRY_RUN_ONLY -eq 1 ]] && die "--publish-only and --dry-run are mutually exclusive"
  SKIP_NATIVE=1
  SKIP_RESOLUTION_CHECK=1
fi
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$ ]] \
  || die "not a valid semver version: '$VERSION' (expected e.g. 0.2.2)"

# ── abort cleanup ─────────────────────────────────────────────────────
# Before the release is committed, a failure should leave the tree exactly as we
# found it (discard the in-progress bump). After the commit — i.e. once the
# irreversible publishing has begun — reverting would be misleading, so we point
# the user at resuming instead.
BUMP_APPLIED=0
RELEASE_COMMITTED=0
cleanup() {
  local ec=$?
  # Always remove the temporary npmrc holding the automation token, on success or failure.
  [[ -n "${NPM_USERCONFIG:-}" ]] && rm -f "$NPM_USERCONFIG"
  [[ $ec -eq 0 ]] && return 0
  if [[ $RELEASE_COMMITTED -eq 1 ]]; then
    warn "release v$VERSION was committed and publishing may be partially done."
    warn "re-run 'scripts/deploy_code.sh $VERSION' to resume — published versions are skipped."
  elif [[ $BUMP_APPLIED -eq 1 ]]; then
    warn "reverting version bump in the working tree"
    git checkout -- "${BUMP_FILES[@]}" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# ── preflight: environment, branch, tree ──────────────────────────────
info "preflight checks"
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || die "not inside a git repository"

cur_branch="$(git rev-parse --abbrev-ref HEAD)"
[[ "$cur_branch" == "$BRANCH" ]] \
  || die "must be on the '$BRANCH' branch (currently on '$cur_branch'). Run: git checkout $BRANCH"

git diff --quiet && git diff --cached --quiet \
  || die "working tree has uncommitted changes to tracked files. Commit or stash them first."

# ── preflight: deployed must include all of staging ───────────────────
# A release is cut from `deployed`; if `staging` has commits that were never
# merged down into `deployed`, we would ship a version that is *behind* the work
# on staging (silently dropping merged PRs from the release). Fail unless the
# operator is deliberately releasing an earlier point (--allow-behind-staging).
info "checking $BRANCH includes all of $STAGING_BRANCH"
if git fetch --quiet origin "$STAGING_BRANCH" 2>/dev/null; then
  behind_count="$(git rev-list --count "HEAD..origin/$STAGING_BRANCH" 2>/dev/null || echo "")"
  [[ -n "$behind_count" ]] || die "could not compare HEAD with origin/$STAGING_BRANCH"
  if [[ "$behind_count" -gt 0 ]]; then
    warn "origin/$STAGING_BRANCH has $behind_count commit(s) not in $BRANCH:"
    git --no-pager log --oneline --no-decorate "HEAD..origin/$STAGING_BRANCH" | sed 's/^/      /' >&2
    if [[ $ALLOW_BEHIND_STAGING -eq 1 ]]; then
      warn "--allow-behind-staging set: releasing from $BRANCH anyway (an earlier point than $STAGING_BRANCH)"
    else
      die "refusing to release: merge $STAGING_BRANCH into $BRANCH first, or pass --allow-behind-staging to release this earlier point on purpose"
    fi
  else
    ok "$BRANCH is up to date with origin/$STAGING_BRANCH"
  fi
elif [[ $ALLOW_BEHIND_STAGING -eq 1 ]]; then
  warn "could not fetch origin/$STAGING_BRANCH; --allow-behind-staging set, continuing without the check"
else
  die "could not fetch origin/$STAGING_BRANCH to verify $BRANCH is up to date; fix connectivity or pass --allow-behind-staging"
fi

need() { command -v "$1" >/dev/null 2>&1 || die "required tool '$1' not found on PATH${2:+ — $2}"; }
need git; need cargo; need npm; need python3; need curl
if [[ $SKIP_NATIVE -eq 0 ]]; then
  need rustup
  need xcodebuild "install Xcode command-line tools, or pass --skip-native-build"
  if [[ -z "${ANDROID_NDK_HOME:-}" ]] \
     && [[ ! -d "${ANDROID_HOME:-}/ndk" ]] \
     && [[ ! -d "$HOME/Library/Android/sdk/ndk" ]]; then
    die "Android NDK not found. Set ANDROID_NDK_HOME (or install via Android Studio), or pass --skip-native-build"
  fi
fi
ok "environment, branch ($BRANCH), and clean tree"

# ── preflight: version must not go backwards ──────────────────────────
# Guard against a fumbled version string (a dropped digit, or "0.3.1" typed as
# "3.0.1") shipping a release that sorts below the last one. The requested
# version must be >= the version currently in the tree — which, because every
# deploy commits its own bump, is the last released version. Equal is permitted
# so a partially-completed release can be resumed by re-running the same command
# (the per-crate/-npm "already published, skipping" checks make that safe).
# Note: this only catches going *backwards*; a typo that happens to sort higher
# (e.g. 0.3.0 -> 3.0.1) still passes, so eyeball the version before confirming.
current_version="$(python3 - <<'PY'
import re, pathlib
text = pathlib.Path("bingle_core/Cargo.toml").read_text()
m = re.search(r'\[package\][^\[]*?\nversion\s*=\s*"([^"]*)"', text, re.S)
print(m.group(1) if m else "")
PY
)"
[[ -n "$current_version" ]] || die "could not read the current version from bingle_core/Cargo.toml"
if ! python3 - "$current_version" "$VERSION" <<'PY'
import sys
def core(v):  # numeric X.Y.Z, ignoring any pre-release/build suffix
    return tuple(int(x) for x in v.split('-', 1)[0].split('.')[:3])
sys.exit(0 if core(sys.argv[2]) >= core(sys.argv[1]) else 1)
PY
then
  die "requested version '$VERSION' is lower than the current version '$current_version' — refusing to release backwards"
fi
if [[ "$VERSION" == "$current_version" ]]; then
  warn "requested version '$VERSION' equals the current version — assuming a resumed or repeat release"
else
  ok "version $VERSION is ahead of current $current_version"
fi

# ── preflight: publish auth ───────────────────────────────────────────
# npm publishing is the one step that can block on an interactive browser prompt,
# and it lands *after* the ~10-minute native build: with account 2FA set to
# "auth and writes", an ordinary `npm publish` opens a short-lived web-auth window
# that fails the release if it isn't clicked promptly. An npm automation token
# bypasses 2FA, so when NPM_TOKEN is provided we publish non-interactively via a
# throwaway npmrc (cleaned up by the trap) and validate it now — a bad token fails
# in seconds rather than after the build. Without a token we keep the interactive
# flow but say so up front so the wait is expected, not a surprise.
NPM_USERCONFIG=""
NPM_PUBLISH_ARGS=()
if [[ -n "${NPM_TOKEN:-}" ]]; then
  NPM_USERCONFIG="$(mktemp -t bingle-npmrc.XXXXXX)"
  {
    printf 'registry=https://registry.npmjs.org/\n'
    printf '//registry.npmjs.org/:_authToken=%s\n' "$NPM_TOKEN"
  } > "$NPM_USERCONFIG"
  NPM_PUBLISH_ARGS=(--userconfig "$NPM_USERCONFIG")
  if ! npm_user="$(npm whoami "${NPM_PUBLISH_ARGS[@]}" 2>/dev/null)"; then
    die "NPM_TOKEN is set but is not valid for registry.npmjs.org. Generate an Automation token at npmjs.com > Access Tokens."
  fi
  ok "npm authenticated as '$npm_user' via NPM_TOKEN (non-interactive publish)"
else
  # whoami fails (non-zero) when not logged in.
  if ! npm_user="$(npm whoami 2>/dev/null)"; then
    die "not authenticated to npm. Run: npm login"
  fi
  ok "npm authenticated as '$npm_user'"
  warn "no NPM_TOKEN set — the final 'npm publish' will require an interactive 2FA browser click."
  warn "It is gated on a keypress (so the short-lived window opens only when you're present) and"
  warn "retried on failure, so a missed window is recoverable. For a fully unattended release,"
  warn "create an npm Automation token (npmjs.com > Access Tokens) and re-run with NPM_TOKEN=... set."
fi

# crates.io: confirm a token is present in the environment or the cargo credentials
# file. crates.io deliberately restricts its identity endpoint (/api/v1/me) to the
# website, so an API token cannot be pre-validated here; an invalid token surfaces
# at publish time, which the dry run still gates everything before.
CRATES_TOKEN="${CARGO_REGISTRY_TOKEN:-}"
if [[ -z "$CRATES_TOKEN" ]]; then
  for cred in "${CARGO_HOME:-$HOME/.cargo}/credentials.toml" "${CARGO_HOME:-$HOME/.cargo}/credentials"; do
    if [[ -f "$cred" ]]; then
      CRATES_TOKEN="$(awk -F'"' '/^[[:space:]]*token[[:space:]]*=/{print $2; exit}' "$cred")"
      [[ -n "$CRATES_TOKEN" ]] && break
    fi
  done
fi
[[ -n "$CRATES_TOKEN" ]] || die "not authenticated to crates.io. Run: cargo login"
ok "crates.io token found"

# ── crates.io / npm existence helpers ─────────────────────────────────
crate_version_exists() { # <crate> <version>
  curl -sfL -A "bingle-deploy" -o /dev/null "https://crates.io/api/v1/crates/$1/$2"
}
wait_for_crate() { # <crate> <version> — poll the index after a publish
  local i
  for i in $(seq 1 30); do
    crate_version_exists "$1" "$2" && return 0
    sleep 5
  done
  return 1
}
npm_version_exists() { # <pkg> <version>
  [[ -n "$(npm view "$1@$2" version 2>/dev/null || true)" ]]
}

# Publish the npm module on the interactive (no-NPM_TOKEN) path. The 2FA browser
# window npm opens is short-lived, so we gate each attempt on an operator keypress —
# the window then opens only when someone is present to approve it, not unattended at
# an unpredictable point after the long build — and retry on failure so a missed or
# expired window is recoverable without re-running the whole build. Token publishes
# are non-interactive and never call this.
publish_npm_interactive() { # <pkg> <version>
  local attempt=1 ans
  while true; do
    if [[ -t 0 ]]; then
      printf '\n'
      info "ready to publish $1@$2 to npm (attempt $attempt)."
      warn "npm will open a browser 2FA window that expires quickly — be ready to approve it."
      read -rp "    press Enter to start the npm publish (Ctrl-C to abort): " _ || return 1
    fi
    if ( cd "$NPM_DIR" && npm publish ${NPM_PUBLISH_ARGS[@]+"${NPM_PUBLISH_ARGS[@]}"} ); then
      return 0
    fi
    # A prior attempt may have actually landed the version before the client errored.
    if npm_version_exists "$1" "$2"; then
      ok "$1@$2 is already on npm — treating as published"
      return 0
    fi
    warn "npm publish attempt $attempt failed (the 2FA window may have expired)."
    [[ -t 0 ]] || return 1   # non-interactive: nothing to retry against, give up
    read -rp "    retry npm publish? [Y/n]: " ans || return 1
    [[ "$ans" =~ ^[Nn] ]] && return 1
    attempt=$((attempt + 1))
  done
}

# ── bump versions ─────────────────────────────────────────────────────
info "bumping workspace + npm module to $VERSION"
BUMP_APPLIED=1
python3 - "$VERSION" <<'PY'
import re, sys, pathlib
version = sys.argv[1]
members = ["bingle_core", "bingle_local", "bingle_jsi", "bingle_webserver", "bingle_test", "bingle_cli"]
for m in members:
    p = pathlib.Path(m) / "Cargo.toml"
    text = p.read_text()
    # [package] version — stay inside the [package] table (stop before next '[')
    text, n = re.subn(r'(\[package\][^\[]*?\nversion\s*=\s*")[^"]*(")',
                      lambda mo: mo.group(1) + version + mo.group(2), text, count=1, flags=re.S)
    if n != 1:
        sys.exit(f"failed to bump [package].version in {p}")
    # workspace path-dependency version pins, e.g. bingle_core = { path = "../bingle_core", version = "X" }
    text = re.sub(r'(path\s*=\s*"\.\./bingle_[a-z_]+"\s*,\s*version\s*=\s*")[^"]*(")',
                  lambda mo: mo.group(1) + version + mo.group(2), text)
    p.write_text(text)

pj = pathlib.Path("bingle_jsi/package.json")
t = pj.read_text()
t, n = re.subn(r'("version"\s*:\s*")[^"]*(")',
               lambda mo: mo.group(1) + version + mo.group(2), t, count=1)
if n != 1:
    sys.exit("failed to bump version in bingle_jsi/package.json")
pj.write_text(t)
print(f"    set version {version} across {len(members)} crates + the npm module")
PY
# Refresh the lockfile's workspace-member versions (no network, members only).
cargo update --workspace >/dev/null 2>&1 || cargo update -w >/dev/null
ok "versions bumped"

# ── build / compile gate ──────────────────────────────────────────────
if [[ $PUBLISH_ONLY -eq 1 ]]; then
  warn "--publish-only: skipping compile, resolution, native-build, and dry-run gates"
  warn "(resuming publish of a build that already passed these checks)"
fi
if [[ $PUBLISH_ONLY -eq 0 ]]; then
info "compiling workspace at $VERSION"
cargo check --workspace
ok "workspace compiles"

# ── from-scratch dependency-resolution check ──────────────────────────
# `cargo check --workspace` above uses the committed Cargo.lock, so it cannot
# catch what a downstream `cargo install` hits: that IGNORES the lockfile and
# re-resolves every dependency to the newest semver-compatible version. A newer
# release of a transitive dep can break that fresh resolve while the locked build
# stays green (e.g. precis-profiles 0.1.14 pulling precis-core 0.2.0, which breaks
# stun-rs 0.1.11). Reproduce the fresh resolve here — bump the lock to
# latest-compatible, compile, then restore the committed lock — so the break is
# caught before publishing rather than by users after `cargo install`.
if [[ $SKIP_RESOLUTION_CHECK -eq 0 ]]; then
  info "checking a from-scratch dependency resolution (what 'cargo install' does)"
  fresh_lock_backup="$(mktemp -t bingle-cargo-lock.XXXXXX)"
  cp Cargo.lock "$fresh_lock_backup"
  restore_lock() { cp "$fresh_lock_backup" Cargo.lock; rm -f "$fresh_lock_backup"; }
  if ! cargo update >/dev/null 2>&1; then
    restore_lock
    die "fresh dependency resolution failed (cargo update); a dependency's version requirements may be unsatisfiable."
  fi
  if ! cargo check --workspace; then
    restore_lock
    die "workspace fails to compile under a from-scratch dependency resolution — a downstream 'cargo install' would fail even though the locked build passed. A newer semver-compatible dependency broke the build; pin the offending crate in the relevant Cargo.toml (see the precis-* pins in bingle_core/Cargo.toml), commit, then re-run. To bypass, pass --skip-resolution-check."
  fi
  restore_lock
  ok "from-scratch dependency resolution compiles"
else
  warn "skipping from-scratch dependency-resolution check (--skip-resolution-check)"
fi

if [[ $SKIP_NATIVE -eq 0 ]]; then
  info "building native libraries (iOS + Android)"
  bash "$NPM_DIR/scripts/build_ios.sh"
  bash "$NPM_DIR/scripts/build_android.sh"
  ok "native libraries built and leak-scanned"
else
  warn "skipping native build (--skip-native-build): npm tarball uses existing artifacts"
fi

# ── dry-run every publish before touching a registry ──────────────────
info "dry-run: cargo packages"
# Dry-run all crates together (cargo orders them by the dependency graph). With
# multiple -p, cargo packages each crate into a temporary local registry and
# verifies dependents against it, so a dependent resolves the not-yet-published
# dependency version locally instead of failing against the crates.io index.
# This fully verifies every crate — no --no-verify escape hatch needed.
pkg_args=()
for crate in "${PUBLISH_CRATES[@]}"; do pkg_args+=(-p "$crate"); done
cargo publish --dry-run --allow-dirty "${pkg_args[@]}"
for crate in "${PUBLISH_CRATES[@]}"; do ok "packaged $crate@$VERSION"; done

info "dry-run: npm module"
( cd "$NPM_DIR" && npm publish --dry-run ${NPM_PUBLISH_ARGS[@]+"${NPM_PUBLISH_ARGS[@]}"} )
ok "packaged $NPM_PKG@$VERSION"
fi  # end: build / dry-run gates (skipped under --publish-only)

if [[ $DRY_RUN_ONLY -eq 1 ]]; then
  info "--dry-run: all checks passed, publishing nothing; reverting version bump"
  git checkout -- "${BUMP_FILES[@]}"
  BUMP_APPLIED=0
  info "dry run complete for v$VERSION"
  exit 0
fi

# ── point of no return: commit, then publish ──────────────────────────
info "committing release v$VERSION"
if ! git diff --quiet || ! git diff --cached --quiet; then
  git commit -aqm "chore: release v$VERSION"
fi
if ! git rev-parse -q --verify "refs/tags/v$VERSION" >/dev/null; then
  git tag -a "v$VERSION" -m "release v$VERSION"
fi
RELEASE_COMMITTED=1
ok "committed and tagged v$VERSION"

info "publishing crates to crates.io"
for crate in "${PUBLISH_CRATES[@]}"; do
  if crate_version_exists "$crate" "$VERSION"; then
    ok "$crate@$VERSION already published — skipping"
  else
    cargo publish -p "$crate"
    ok "published $crate@$VERSION"
  fi
  wait_for_crate "$crate" "$VERSION" \
    || die "$crate@$VERSION not visible on crates.io after publish (index lag?); re-run to resume"
done

info "publishing $NPM_PKG to npm"
if npm_version_exists "$NPM_PKG" "$VERSION"; then
  ok "$NPM_PKG@$VERSION already published — skipping"
elif [[ -n "${NPM_TOKEN:-}" ]]; then
  # Token publish: non-interactive, no 2FA window, so no gating/retry needed.
  ( cd "$NPM_DIR" && npm publish ${NPM_PUBLISH_ARGS[@]+"${NPM_PUBLISH_ARGS[@]}"} )
  ok "published $NPM_PKG@$VERSION"
else
  publish_npm_interactive "$NPM_PKG" "$VERSION" \
    || die "npm publish did not complete for $NPM_PKG@$VERSION. The crates are already published; re-run 'scripts/deploy_code.sh --publish-only $VERSION' to retry just the npm step (skips the build)."
  ok "published $NPM_PKG@$VERSION"
fi

info "pushing branch and tag"
git push origin "$BRANCH"
git push origin "v$VERSION"

info "release v$VERSION complete: crates.io (${PUBLISH_CRATES[*]}) + npm ($NPM_PKG)"

# Opt-in post-release cleanup: the native (JSI multi-arch) builds above and the
# workspace target/ leave tens of GB of regenerable cache. With the release fully
# published and pushed there is nothing left to retry, so it is safe to reclaim it.
# Only on real success (never under --dry-run, and past the success point here so a
# failed/abortable release keeps its cache for a resume).
if [[ $CLEAN_AFTER -eq 1 ]]; then
  info "cleaning build cache (--clean)"
  bash "$REPO_ROOT/scripts/clean_build_cache.sh" --yes
fi
