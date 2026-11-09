#!/usr/bin/env bash
# The promotion guard: may this commit be released (issue #399)?
#
#   scripts/release_guard.sh <sha>
#
# `deployed` takes no pull requests, so the commit itself is checked. It passes only when the commit:
#   1. is on origin/staging, so it went through the pull-request gates;
#   2. carries one version everywhere (every crate and bingle_jsi/package.json), and that version's
#      tag vX.Y.Z, if it exists, names this same commit — one version never names two commits;
#   3. had a successful deployment in the `testnet` GitHub environment (scripts/roll_relays.sh), so
#      TestNet ran it. The one override is an `Allow-untested: <reason>` line in the release note on
#      the commit (refs/notes/release, pushed by scripts/release.sh --allow-untested) or in a release
#      tag pushed first; the reason is printed as a warning.
#
# Needs git with the tags fetched, and gh signed in (GH_TOKEN in CI). On success it prints the
# version and, under GitHub Actions, writes sha, version and tag to $GITHUB_OUTPUT.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

GH_REPO=${GH_REPO:-${GITHUB_REPOSITORY:-binglefoundation/bingle_rust}}

fail() { printf 'release guard: %s\n' "$*" >&2; [[ -n "${GITHUB_ACTIONS:-}" ]] && printf '::error::%s\n' "$*"; exit 1; }
note() { printf 'release guard: %s\n' "$*"; }

[[ $# -eq 1 ]] || fail "usage: scripts/release_guard.sh <sha>"
SHA=$(git rev-parse --verify "$1^{commit}" 2>/dev/null) || fail "$1 is not a commit"
SHORT=$(git rev-parse --short=8 "$SHA")

git fetch --quiet --tags origin staging || fail "could not fetch origin"

# 1. on staging
git merge-base --is-ancestor "$SHA" origin/staging \
  || fail "$SHORT is not on staging; only a staging commit that passed its pull-request gates is released"
note "$SHORT is on staging"

# 2. one version, and its tag (if any) names this commit
VERSION=$(git show "$SHA:bingle_core/Cargo.toml" | python3 -c '
import re, sys
m = re.search(r"\[package\][^\[]*?\nversion\s*=\s*\"([^\"]*)\"", sys.stdin.read(), re.S)
print(m.group(1) if m else "")')
[[ -n "$VERSION" ]] || fail "could not read the version of $SHORT"
for f in bingle_local/Cargo.toml bingle_cli/Cargo.toml bingle_jsi/Cargo.toml bingle_jsi/package.json; do
  git show "$SHA:$f" | grep -q "\"$VERSION\"" || fail "$f at $SHORT does not carry version $VERSION (run scripts/bump_version.sh --check)"
done
TAG="v$VERSION"
tag_sha=$(git rev-parse -q --verify "refs/tags/$TAG^{commit}" 2>/dev/null || true)
if [[ -n "$tag_sha" && "$tag_sha" != "$SHA" ]]; then
  fail "$TAG already names $(git rev-parse --short=8 "$tag_sha"), not $SHORT; one version names one commit, so $SHORT needs a new version"
fi
note "version $VERSION${tag_sha:+ (tag $TAG already names $SHORT)}"

# 3. ran on TestNet, or a recorded override
override=""
if [[ -n "$tag_sha" ]]; then
  override=$(git for-each-ref --format='%(contents)' "refs/tags/$TAG" | sed -n 's/^Allow-untested:[[:space:]]*//p' | head -1)
fi
if [[ -z "$override" ]] && git fetch --quiet origin "+refs/notes/release:refs/notes/release" 2>/dev/null; then
  override=$(git notes --ref=release show "$SHA" 2>/dev/null | sed -n 's/^Allow-untested:[[:space:]]*//p' | head -1)
fi
tested=0
for id in $(gh api "repos/$GH_REPO/deployments?environment=testnet&sha=$SHA&per_page=30" -q '.[].id'); do
  state=$(gh api "repos/$GH_REPO/deployments/$id/statuses?per_page=1" -q '.[0].state // ""')
  if [[ "$state" == success ]]; then tested=1; break; fi
done
if [[ $tested -eq 1 ]]; then
  note "$SHORT has a successful testnet deployment"
elif [[ -n "$override" ]]; then
  note "WARNING: $SHORT has no successful testnet deployment; released untested: $override"
  [[ -n "${GITHUB_ACTIONS:-}" ]] && printf '::warning::%s released without a TestNet deployment: %s\n' "$SHORT" "$override"
else
  fail "$SHORT has no successful testnet deployment; roll TestNet onto it first (scripts/roll_relays.sh), or release it untested with scripts/release.sh --allow-untested (RELEASING.md)"
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  { echo "sha=$SHA"; echo "version=$VERSION"; echo "tag=$TAG"; } >> "$GITHUB_OUTPUT"
fi
note "$SHORT may be released as $VERSION"
