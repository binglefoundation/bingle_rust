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

section() { echo -e "\n${BOLD}${YELLOW}== $1 ==${NC}"; }

# ------------------------------------------------------------------ fix mode
if [[ "${MODE}" == "fix" ]]; then
  section "rustfmt (apply)"
  cargo fmt --all
  section "clippy --fix (apply)"
  # --allow-dirty/--allow-staged so it runs against an uncommitted tree.
  cargo clippy "${CLIPPY_TARGETS[@]}" --fix --allow-dirty --allow-staged
  echo -e "\n${YELLOW}Applied mechanical fixes. Re-run '$0' to see what remains.${NC}"
  exit 0
fi

STRICT=0
[[ "${MODE}" == "strict" ]] && STRICT=1
fail=0

# --------------------------------------------------------------------- fmt
if [[ "${RUN_FMT}" == "1" ]]; then
  section "rustfmt"
  fmt_out="$(cargo fmt --all -- --check 2>/dev/null)"
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
