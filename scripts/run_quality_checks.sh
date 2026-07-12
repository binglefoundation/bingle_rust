#!/usr/bin/env bash
# scripts/run_quality_checks.sh
# Code-quality gate for the Bingle Rust workspace: rustfmt + clippy.
#
# These are the tools used in the `chore/rustfmt-clippy-cleanup` pass and are
# intended to run as a (manual, for now) CI step. Run from the repo root.
#
# Usage:
#   scripts/run_quality_checks.sh            # check mode (default): fails on any issue
#   scripts/run_quality_checks.sh check      # same as above
#   scripts/run_quality_checks.sh fix        # auto-apply rustfmt + clippy --fix, then re-check
#
# Exit code is non-zero if any check fails, so it is CI-gate ready. Individual
# checks are governed by env vars so a partially-clean tree can still gate the
# clean parts:
#   RUN_FMT=0      skip the rustfmt check
#   RUN_CLIPPY=0   skip the clippy check
#   CLIPPY_DENY=0  report clippy warnings without failing (useful while burning
#                  down an existing backlog); default 1 = treat warnings as errors

set -uo pipefail

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m'

MODE="${1:-check}"
RUN_FMT="${RUN_FMT:-1}"
RUN_CLIPPY="${RUN_CLIPPY:-1}"
CLIPPY_DENY="${CLIPPY_DENY:-1}"

# clippy lints every target (lib, bins, tests, examples). Test targets pull in
# the `test-hooks` feature automatically via bingle_core's self dev-dependency.
CLIPPY_ARGS=(--workspace --all-targets)
if [[ "${CLIPPY_DENY}" == "1" ]]; then
  CLIPPY_TAIL=(-- -D warnings)
else
  CLIPPY_TAIL=(-- -W warnings)
fi

fail=0

section() { echo -e "\n${YELLOW}== $1 ==${NC}"; }

if [[ "${MODE}" == "fix" ]]; then
  section "rustfmt (apply)"
  cargo fmt --all || fail=1
  section "clippy --fix (apply)"
  # --allow-dirty/--allow-staged so it runs against an uncommitted tree.
  cargo clippy "${CLIPPY_ARGS[@]}" --fix --allow-dirty --allow-staged || fail=1
  echo -e "\n${YELLOW}Applied fixes. Re-run without 'fix' to verify a clean tree.${NC}"
  exit "${fail}"
fi

# ---- check mode ----
if [[ "${RUN_FMT}" == "1" ]]; then
  section "rustfmt --check (workspace)"
  if cargo fmt --all -- --check; then
    echo -e "${GREEN}rustfmt: clean${NC}"
  else
    echo -e "${RED}rustfmt: formatting differences found (run: scripts/run_quality_checks.sh fix)${NC}"
    fail=1
  fi
fi

if [[ "${RUN_CLIPPY}" == "1" ]]; then
  section "clippy (${CLIPPY_ARGS[*]} ${CLIPPY_TAIL[*]})"
  if cargo clippy "${CLIPPY_ARGS[@]}" "${CLIPPY_TAIL[@]}"; then
    echo -e "${GREEN}clippy: clean${NC}"
  else
    echo -e "${RED}clippy: lints reported${NC}"
    fail=1
  fi
fi

echo
if [[ "${fail}" == "0" ]]; then
  echo -e "${GREEN}OK: quality checks passed.${NC}"
else
  echo -e "${RED}FAIL: quality checks reported issues (see above).${NC}"
fi
exit "${fail}"
