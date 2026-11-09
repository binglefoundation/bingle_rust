#!/usr/bin/env bash
# Write the release notes for one release (issue #432).
#
#   scripts/release_notes.sh <commit> <tag> <out-file>
#
# Lists the pull requests merged on the commit's first-parent line since the previous release tag,
# asks Claude (`claude -p`, no tools) to summarise them for a semi-technical reader with the prompt in
# scripts/release_notes_prompt.md, and appends GitHub's generated list of those pull requests. Without
# `claude`, or if it fails, the notes are GitHub's list alone and a warning says so. Writes nothing but
# <out-file>; GitHub's generate-notes call creates no release.
#
# Env overrides:
#   GH_REPO               repository (default binglefoundation/bingle_rust)
#   RELEASE_NOTES_MODEL   model passed to `claude --model` (default: claude's own default)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

GH_REPO=${GH_REPO:-binglefoundation/bingle_rust}
PROMPT_FILE="scripts/release_notes_prompt.md"

warn() { printf '\033[1;33mwarn:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

[[ $# -eq 3 ]] || die "usage: scripts/release_notes.sh <commit> <tag> <out-file>"
SHA=$(git rev-parse --verify "$1^{commit}") || die "$1 is not a commit"
TAG=$2
OUT=$3

# the newest release tag behind the commit (not the commit's own tag, on a re-run)
PREV=$(git describe --tags --abbrev=0 --match 'v[0-9]*' "$SHA^" 2>/dev/null) \
  || die "no earlier v* tag behind $(git rev-parse --short=8 "$SHA"); fetch the tags first"

# PR numbers from the first-parent merges ("Merge pull request #N") or squash merges ("… (#N)")
prs=$(git log --first-parent --format=%s "$PREV..$SHA" \
  | sed -nE -e 's/^Merge pull request #([0-9]+) .*/\1/p' -e 's/.*\(#([0-9]+)\)$/\1/p' | awk '!seen[$0]++')
[[ -n "$prs" ]] || die "no pull requests merged between $PREV and $(git rev-parse --short=8 "$SHA")"

work=$(mktemp -d "${TMPDIR:-/tmp}/release_notes.XXXXXX")
trap 'rm -rf "$work"' EXIT

{
  echo "Release: $TAG (previous release: $PREV)"
  echo
  for n in $prs; do
    gh pr view "$n" -R "$GH_REPO" --json number,title,body \
      -q '"## PR #\(.number): \(.title)\n\n\(.body)\n"'
  done
} > "$work/prs.md"

gh api "repos/$GH_REPO/releases/generate-notes" -f tag_name="$TAG" -f target_commitish="$SHA" \
  -f previous_tag_name="$PREV" -q .body > "$work/generated.md" \
  || die "GitHub's generate-notes call failed"

summary=""
if ! command -v claude >/dev/null; then
  warn "claude is not installed; the notes are GitHub's pull-request list only"
else
  model_args=()
  [[ -n "${RELEASE_NOTES_MODEL:-}" ]] && model_args=(--model "$RELEASE_NOTES_MODEL")
  if ! summary=$(claude -p --tools "" --no-session-persistence ${model_args[@]+"${model_args[@]}"} \
      --system-prompt "$(cat "$PROMPT_FILE")" \
      "Write the release notes for $TAG from the pull requests on stdin." < "$work/prs.md"); then
    warn "claude failed; the notes are GitHub's pull-request list only"
    summary=""
  fi
fi

{
  if [[ -n "$summary" ]]; then
    printf '%s\n\n' "$summary"
  fi
  echo "## Pull requests"
  echo
  # GitHub's list starts with its own "## What's Changed" heading; ours replaces it
  sed '/^## What.s Changed/d' "$work/generated.md"
} > "$OUT"
