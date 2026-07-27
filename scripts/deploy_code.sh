#!/usr/bin/env bash
# Cut a release: bump every crate + the npm module to the given version, build,
# and publish the crates to crates.io and the module to npm.
#
#   scripts/deploy_code.sh <version>            e.g. scripts/deploy_code.sh 0.2.2
#   scripts/deploy_code.sh --dry-run <version>  validate + dry-run only, publish nothing
#   scripts/deploy_code.sh --skip-native-build <version>
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
PUBLISH_CRATES=(bingle_core bingle_local)   # crates.io, in dependency order
NPM_DIR="bingle_jsi"
NPM_PKG="react-native-bingle-jsi"
BUMP_FILES=(
  bingle_core/Cargo.toml bingle_local/Cargo.toml bingle_jsi/Cargo.toml
  bingle_webserver/Cargo.toml bingle_test/Cargo.toml
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
VERSION=""
for arg in "$@"; do
  case "$arg" in
    --dry-run)           DRY_RUN_ONLY=1 ;;
    --skip-native-build) SKIP_NATIVE=1 ;;
    -h|--help)           sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*)                  die "unknown option: $arg" ;;
    *)                   [[ -n "$VERSION" ]] && die "unexpected extra argument: $arg"; VERSION="$arg" ;;
  esac
done
[[ -n "$VERSION" ]] || die "usage: scripts/deploy_code.sh [--dry-run] [--skip-native-build] <version>"
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
  warn "no NPM_TOKEN set — the final 'npm publish' will require an interactive 2FA browser click,"
  warn "which appears only after the native build. For an unattended release, create an npm"
  warn "Automation token (npmjs.com > Access Tokens) and re-run with NPM_TOKEN=... set."
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

# ── bump versions ─────────────────────────────────────────────────────
info "bumping workspace + npm module to $VERSION"
BUMP_APPLIED=1
python3 - "$VERSION" <<'PY'
import re, sys, pathlib
version = sys.argv[1]
members = ["bingle_core", "bingle_local", "bingle_jsi", "bingle_webserver", "bingle_test"]
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
info "compiling workspace at $VERSION"
cargo check --workspace
ok "workspace compiles"

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
else
  ( cd "$NPM_DIR" && npm publish ${NPM_PUBLISH_ARGS[@]+"${NPM_PUBLISH_ARGS[@]}"} )
  ok "published $NPM_PKG@$VERSION"
fi

info "pushing branch and tag"
git push origin "$BRANCH"
git push origin "v$VERSION"

info "release v$VERSION complete: crates.io (${PUBLISH_CRATES[*]}) + npm ($NPM_PKG)"
