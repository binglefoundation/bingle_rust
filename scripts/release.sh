#!/usr/bin/env bash
# Release the staging commit TestNet runs: publish it to crates.io and npm, and open the next
# version on staging (issue #399).
#
#   scripts/release.sh --dry-run          # check everything and show the plan; change nothing
#   scripts/release.sh                    # release the head of origin/staging
#   scripts/release.sh --commit <sha>     # release an earlier staging commit instead
#   scripts/release.sh --next-minor       # open X.(Y+1).0 afterwards instead of X.Y.(Z+1) (or --next-major)
#   scripts/release.sh --allow-untested "<reason>"
#         release a commit with no successful testnet deployment (an urgent fix): the reason goes
#         into the release tag, which the release guard reads
#   scripts/release.sh --yes              # do not ask before promoting
#
# Run from any checkout, signed in to gh with push access. It never holds a registry or AWS
# credential: publishing is deploy.yml's job, through Trusted Publishing. One run:
#   1. picks the commit and runs scripts/release_guard.sh on it (on staging, one version whose tag
#      names no other commit, a successful testnet deployment from scripts/roll_relays.sh);
#   2. checks the commit's staging CI passed: quality, unit, integration and android-e2e;
#   3. fast-forwards `deployed` to it (no merge, no commit of its own; a push that is not a
#      fast-forward is refused), which starts deploy.yml, and waits for that run;
#   4. opens the next version: commits `chore: open X.Y.Z` (scripts/bump_version.sh) onto staging and
#      pushes it — the one commit that reaches staging without a pull request, as it is mechanical;
#   5. prints the follow-ups: roll MainNet, and the bingle_ux_rn pickup.
# Each step is skipped when already done, so re-running the same command resumes a release that
# stopped part way. See RELEASING.md.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

GH_REPO=${GH_REPO:-binglefoundation/bingle_rust}
GATES=(quality unit integration android-e2e)

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
ok()   { printf '    \033[1;32mok\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarn:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

DRY_RUN=0 YES=0 COMMIT="" NEXT=--next-patch UNTESTED=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --yes) YES=1; shift ;;
    --commit) [[ -n "${2:-}" ]] || die "--commit needs a sha"; COMMIT=$2; shift 2 ;;
    --next-minor|--next-major) NEXT=$1; shift ;;
    --allow-untested) [[ -n "${2:-}" ]] || die "--allow-untested needs a reason"; UNTESTED=$2; shift 2 ;;
    -h|--help) sed -n '2,27p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

confirm() { # <question>
  [[ $YES -eq 1 ]] && return 0
  [[ -t 0 ]] || die "$1 — not a terminal; re-run with --yes"
  local ans
  read -rp "    $1 [y/N]: " ans
  [[ "$ans" =~ ^[Yy] ]]
}

# ── preflight ─────────────────────────────────────────────────────────
command -v gh >/dev/null || die "gh not found"
gh auth status >/dev/null 2>&1 || die "gh is not signed in; run: gh auth login"
git fetch --quiet --tags origin staging deployed || die "could not fetch origin"

SHA=$(git rev-parse --verify "${COMMIT:-origin/staging}^{commit}") || die "${COMMIT} is not a commit"
SHORT=$(git rev-parse --short=8 "$SHA")
VERSION=$(git show "$SHA:bingle_core/Cargo.toml" | python3 -c '
import re, sys
m = re.search(r"\[package\][^\[]*?\nversion\s*=\s*\"([^\"]*)\"", sys.stdin.read(), re.S)
print(m.group(1) if m else "")')
[[ -n "$VERSION" ]] || die "could not read the version of $SHORT"
TAG="v$VERSION"
info "releasing $SHORT as $VERSION"
[[ -z "$COMMIT" ]] || warn "releasing --commit $SHORT, not the head of staging ($(git rev-parse --short=8 origin/staging))"

# ── the untested override: a tag pushed before the release ────────────
if [[ -n "$UNTESTED" ]]; then
  if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    ok "$TAG already exists; the guard reads its message"
  elif [[ $DRY_RUN -eq 1 ]]; then
    warn "dry run: would tag $TAG on $SHORT with Allow-untested: $UNTESTED"
  else
    warn "releasing $SHORT without a successful testnet deployment: $UNTESTED"
    confirm "tag $TAG as released untested?" || die "stopped"
    git tag -a "$TAG" "$SHA" -m "release $TAG" -m "Allow-untested: $UNTESTED"
    git push --quiet origin "refs/tags/$TAG"
    ok "pushed $TAG with its Allow-untested line"
  fi
fi

# ── 1. the guard ──────────────────────────────────────────────────────
info "release guard"
if ! scripts/release_guard.sh "$SHA"; then
  [[ $DRY_RUN -eq 1 && -n "$UNTESTED" ]] && warn "(a real run would push the Allow-untested tag first)"
  die "the release guard refused $SHORT"
fi

# ── 2. staging CI ─────────────────────────────────────────────────────
# deploy.yml re-runs every gate, but checking the staging runs first fails in seconds, not an hour.
info "staging CI on $SHORT"
runs=$(gh api "repos/$GH_REPO/commits/$SHA/check-runs?per_page=100" -q '.check_runs[] | "\(.name)\t\(.status)\t\(.conclusion)"')
ci_ok=1
# a gate passes when any of its runs on the commit succeeded (a re-run after a flake counts).
for gate in "${GATES[@]}"; do
  lines=$(awk -F'\t' -v g="$gate" '$1 == g' <<< "$runs")
  if [[ -z "$lines" ]]; then
    warn "$gate has no run on $SHORT (a newer staging push may have cancelled it; release the head of staging)"
    ci_ok=0
  elif ! awk -F'\t' '$3 == "success"' <<< "$lines" | grep -q .; then
    warn "$gate on $SHORT: $(head -1 <<< "$lines" | cut -f2,3 | tr '\t' ' ')"
    ci_ok=0
  else
    ok "$gate passed"
  fi
done
[[ $ci_ok -eq 1 ]] || die "staging CI has not passed on $SHORT; wait for it or fix it first"

# ── the plan ──────────────────────────────────────────────────────────
deployed=$(git rev-parse origin/deployed)
info "plan"
if [[ "$deployed" == "$SHA" ]]; then
  echo "    deployed is already $SHORT"
else
  git merge-base --is-ancestor "$deployed" "$SHA" \
    || die "deployed ($(git rev-parse --short=8 "$deployed")) is not an ancestor of $SHORT, so it cannot fast-forward; see RELEASING.md, \"Moving deployed onto staging\""
  echo "    fast-forward deployed $(git rev-parse --short=8 "$deployed") -> $SHORT ($(git rev-list --count "$deployed..$SHA") commits)"
fi
echo "    wait for deploy.yml to publish $VERSION and tag $TAG"
echo "    open the next version on staging ($NEXT)"
if [[ $DRY_RUN -eq 1 ]]; then
  info "dry run: nothing changed"
  exit 0
fi

# ── 3. promote and publish ────────────────────────────────────────────
if [[ "$deployed" != "$SHA" ]]; then
  confirm "fast-forward deployed to $SHORT and publish $VERSION? A published version is permanent." || die "stopped"
  git push --quiet origin "$SHA:refs/heads/deployed"
  ok "deployed is $SHORT"
fi

info "deploy.yml on $SHORT"
run_id=""
for _ in $(seq 1 30); do
  run_id=$(gh run list -R "$GH_REPO" --workflow deploy.yml --branch deployed --limit 20 --json databaseId,headSha,conclusion \
    -q "[.[] | select(.headSha == \"$SHA\")] | (map(select(.conclusion == \"success\")) + .) | .[0].databaseId // empty")
  [[ -n "$run_id" ]] && break
  sleep 10
done
[[ -n "$run_id" ]] || die "no deploy.yml run started for $SHORT; start one with: gh workflow run deploy.yml --ref deployed"
echo "    $(gh run view -R "$GH_REPO" "$run_id" --json url -q .url)"
if [[ "$(gh run view -R "$GH_REPO" "$run_id" --json conclusion -q .conclusion)" != success ]]; then
  gh run watch -R "$GH_REPO" "$run_id" --exit-status --interval 60 >/dev/null \
    || die "deploy.yml failed; fix the cause, then: gh run rerun $run_id --failed, and re-run this command"
fi
ok "published $VERSION and tagged $TAG"

# ── 4. open the next version ──────────────────────────────────────────
git fetch --quiet origin staging
staging_version=$(git show origin/staging:bingle_core/Cargo.toml | python3 -c '
import re, sys
m = re.search(r"\[package\][^\[]*?\nversion\s*=\s*\"([^\"]*)\"", sys.stdin.read(), re.S)
print(m.group(1) if m else "")')
if [[ "$staging_version" != "$VERSION" ]]; then
  ok "staging already carries $staging_version"
else
  work="tmp/release-next-$VERSION"
  rm -rf "$work"; git worktree prune
  git worktree add --quiet --detach "$work" origin/staging
  next=$( (cd "$work" && scripts/bump_version.sh "$NEXT") | sed -n 's/.* -> //p')
  (cd "$work" && git commit --quiet -am "chore: open $next" && git push --quiet origin HEAD:refs/heads/staging) \
    || die "could not push the $next bump to staging (did staging move? re-run to bump the new head)"
  git worktree remove --force "$work"
  ok "staging carries $next"
fi

info "released $VERSION ($SHORT). Next:"
echo "    roll MainNet: check out deployed (git worktree add tmp/deployed origin/deployed), bingle_env mainnet, then scripts/roll_relays.sh"
echo "    bingle_ux_rn: move the app and react-native-bingle-jsi to $VERSION"
