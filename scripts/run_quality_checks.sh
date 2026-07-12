#!/usr/bin/env bash
# scripts/run_quality_checks.sh
# Code-quality report for the Bingle Rust workspace: rustfmt + clippy.
#
# These are the tools used in the `chore/rustfmt-clippy-cleanup` pass, wrapped
# for use as a (manual, for now) CI step. Run from the repo root.
#
# The default is a *report*, not a gate: the workspace currently carries a lint
# backlog, so a strict fail-on-warning run would just bury you in output. The
# default therefore summarises fmt drift and clippy warnings (grouped by kind)
# and exits 0. Turn on enforcement with --strict once the backlog is cleared.
#
# Usage:
#   scripts/run_quality_checks.sh            # report: summary of fmt + clippy, exits 0
#   scripts/run_quality_checks.sh --detail   # report, but print full fmt diff + clippy output
#   scripts/run_quality_checks.sh --strict   # gate: fmt must be clean, clippy -D warnings; non-zero on any issue
#   scripts/run_quality_checks.sh fix        # auto-apply: cargo fmt --all + cargo clippy --fix
#
# Env toggles:
#   RUN_FMT=0      skip the rustfmt step
#   RUN_CLIPPY=0   skip the clippy step

set -uo pipefail

GREEN='\033[0;32m'; RED='\033[0;31m'; YELLOW='\033[0;33m'; BOLD='\033[1m'; NC='\033[0m'

RUN_FMT="${RUN_FMT:-1}"
RUN_CLIPPY="${RUN_CLIPPY:-1}"

MODE="report"
DETAIL=0
case "${1:-}" in
  fix)              MODE="fix" ;;
  --strict|strict)  MODE="strict" ;;
  --detail|detail)  DETAIL=1 ;;
  ""|report|check)  ;;
  *) echo "unknown argument: $1"; echo "usage: $0 [--detail|--strict|fix]"; exit 2 ;;
esac

# clippy lints every target (lib, bins, tests, examples). Test targets pull in
# the `test-hooks` feature automatically via bingle_core's self dev-dependency.
CLIPPY_TARGETS=(--workspace --all-targets)

# rustfmt package set: every workspace member except bingle_test. bingle_test's
# lib.rs declares iOS-only modules via `#[cfg(target_os = "ios")] #[path =
# "../../tests/..."]`; `cargo fmt` does not evaluate cfg, so on a non-iOS host it
# tries to resolve those paths and fails with "No such file or directory".
# Derived from the workspace member list so it stays correct as crates are added.
FMT_ARGS=()
while IFS= read -r pkg; do
  [[ -n "${pkg}" && "${pkg}" != "bingle_test" ]] && FMT_ARGS+=(-p "${pkg}")
done < <(sed -n '/members = \[/,/\]/p' Cargo.toml | grep -oE '"[^"]+"' | tr -d '"')
[[ "${#FMT_ARGS[@]}" -eq 0 ]] && FMT_ARGS=(--all)  # fallback if parsing found nothing

section() { echo -e "\n${BOLD}${YELLOW}== $1 ==${NC}"; }

# ------------------------------------------------------------------ fix mode
if [[ "${MODE}" == "fix" ]]; then
  # `cargo clippy --fix` rewrites source and has been observed to corrupt files
  # when many overlapping suggestions apply at once. To keep it safe we (1) refuse
  # to run on a dirty tree — so its edits are the only uncommitted changes and are
  # trivially reviewable/revertible — (2) verify the result still compiles across
  # all targets, and (3) auto-revert clippy's edits if it broke the build. Only
  # then do we run rustfmt (which is deterministic and safe).
  if [[ -n "$(git status --porcelain 2>/dev/null)" ]]; then
    echo -e "${RED}fix mode needs a clean git tree${NC} (so clippy's edits stay reviewable/revertible)."
    echo "Commit or stash your changes first, then re-run: $0 fix"
    exit 1
  fi

  section "clippy --fix (apply)"
  cargo clippy "${CLIPPY_TARGETS[@]}" --fix

  section "verify the tree still compiles"
  if ! cargo check "${CLIPPY_TARGETS[@]}"; then
    echo -e "${RED}clippy --fix produced code that does not compile — reverting its changes.${NC}"
    git restore .
    echo "Reverted. Apply clippy suggestions manually instead (see '$0 --detail')."
    exit 1
  fi

  section "rustfmt (apply)"
  cargo fmt "${FMT_ARGS[@]}"
  echo -e "\n${GREEN}Applied clippy --fix + rustfmt.${NC} Review 'git diff' before committing."
  exit 0
fi

STRICT=0
[[ "${MODE}" == "strict" ]] && STRICT=1
fail=0

# --------------------------------------------------------------------- fmt
if [[ "${RUN_FMT}" == "1" ]]; then
  section "rustfmt"
  fmt_out="$(cargo fmt "${FMT_ARGS[@]}" -- --check)"
  if [[ -z "${fmt_out}" ]]; then
    echo -e "${GREEN}clean${NC}"
  else
    files="$(echo "${fmt_out}" | grep -cE '^Diff in ')"
    echo -e "${RED}${files} formatting difference(s)${NC}  (fix with: $0 fix)"
    [[ "${DETAIL}" == "1" ]] && echo "${fmt_out}"
    [[ "${STRICT}" == "1" ]] && fail=1
  fi
fi

# ------------------------------------------------------------------ clippy
if [[ "${RUN_CLIPPY}" == "1" ]]; then
  section "clippy (${CLIPPY_TARGETS[*]})"
  clippy_log="$(mktemp)"
  if [[ "${STRICT}" == "1" ]]; then
    cargo clippy "${CLIPPY_TARGETS[@]}" -- -D warnings 2>"${clippy_log}"
    clippy_rc=$?
  else
    cargo clippy "${CLIPPY_TARGETS[@]}" 2>"${clippy_log}"
    clippy_rc=$?
  fi

  errors="$(grep -cE '^error(\[|:)' "${clippy_log}")"
  # per-warning lines only (exclude the "generated N warnings" summary lines)
  warns="$(grep -E '^warning: ' "${clippy_log}" | grep -vcE 'generated .* warning')"

  if [[ "${errors}" -gt 0 ]]; then
    echo -e "${RED}${errors} error(s), ${warns} warning(s)${NC}"
  elif [[ "${warns}" -gt 0 ]]; then
    echo -e "${YELLOW}${warns} warning(s), 0 errors${NC}  — top lints:"
    grep -E '^warning: ' "${clippy_log}" | grep -vE 'generated .* warning' \
      | sed -E 's/^warning: //' | sort | uniq -c | sort -rn | head -12 \
      | sed 's/^/    /'
    echo -e "    ${BOLD}...${NC} (many are auto-fixable: $0 fix)"
  else
    echo -e "${GREEN}clean${NC}"
  fi

  [[ "${DETAIL}" == "1" ]] && { echo; cat "${clippy_log}"; }
  # In strict mode, clippy's own exit code (with -D warnings) decides pass/fail.
  [[ "${STRICT}" == "1" && "${clippy_rc}" -ne 0 ]] && fail=1
  rm -f "${clippy_log}"
fi

# ------------------------------------------------------------------ summary
echo
if [[ "${STRICT}" == "1" ]]; then
  if [[ "${fail}" == "0" ]]; then
    echo -e "${GREEN}OK: strict quality gate passed.${NC}"
  else
    echo -e "${RED}FAIL: strict quality gate reported issues (see above).${NC}"
  fi
  exit "${fail}"
fi

echo -e "${BOLD}Report only${NC} (exit 0). Use ${BOLD}--strict${NC} to enforce as a CI gate, ${BOLD}fix${NC} to auto-apply, ${BOLD}--detail${NC} for full output."
exit 0
