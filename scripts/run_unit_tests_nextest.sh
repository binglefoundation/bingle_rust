#!/usr/bin/env bash
# scripts/run_unit_tests_nextest.sh
# cargo-nextest based unit test runner for the Bingle Rust project.
#
# Drop-in equivalent of scripts/run_unit_tests.sh, but delegates per-test
# isolation and timeouts to nextest (see .config/nextest.toml) instead of the
# hand-rolled perl/xargs harness. Runs every crate's `unit` test target and
# prints the same per-crate summary at the end.
#
# Env overrides:
#   UNIT_TEST_TIMEOUT_SECONDS  per-test timeout in seconds (default from nextest config: 20)
#   UNIT_TEST_JOBS             parallel test threads (default 2)
#   NEXTEST_PROFILE            nextest profile to use (default: default)

set -uo pipefail

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m'

CRATES=(
  bingle_core
  bingle_webserver
  bingle_local
  bingle_jsi
)

if ! command -v cargo-nextest >/dev/null 2>&1; then
  echo -e "${RED}cargo-nextest is not installed.${NC}" >&2
  echo "Install it with:  cargo binstall cargo-nextest" >&2
  echo "            or:  cargo install cargo-nextest --locked" >&2
  echo "See https://nexte.st/docs/installation/ for prebuilt binaries." >&2
  exit 1
fi

SUMMARY_CRATES=()
SUMMARY_STATUSES=()
SUMMARY_SECONDS=()
SUMMARY_PASSED=()
SUMMARY_FAILED=()
SUMMARY_SKIPPED=()
OVERALL_STATUS=0
TOTAL_START_SECONDS=$(date +%s)

# Build the extra nextest args shared by every crate.
#
# NOTE: nextest's `--config profile.default.slow-timeout...` override is a no-op in
# this version (0.9.x) — it neither accepts inline tables nor merges dotted leaves
# into slow-timeout. The only reliable way to change the per-test timeout at runtime
# is a full config file via `--config-file`. So when UNIT_TEST_TIMEOUT_SECONDS is set
# we generate a self-contained override file; otherwise we use the repo
# .config/nextest.toml default (20s).
NEXTEST_ARGS=()
mkdir -p tmp
if [[ -n "${UNIT_TEST_TIMEOUT_SECONDS:-}" ]]; then
  OVERRIDE_CONFIG="tmp/nextest_override.toml"
  {
    echo "[profile.default]"
    echo "test-threads = ${UNIT_TEST_JOBS:-2}"
    echo ""
    echo "[profile.default.slow-timeout]"
    echo "period = \"${UNIT_TEST_TIMEOUT_SECONDS}s\""
    echo "terminate-after = 1"
  } > "$OVERRIDE_CONFIG"
  NEXTEST_ARGS+=(--config-file "$OVERRIDE_CONFIG")
fi
if [[ -n "${UNIT_TEST_JOBS:-}" ]]; then
  # --test-threads is a first-class flag and works regardless of the config source.
  NEXTEST_ARGS+=(--test-threads "${UNIT_TEST_JOBS}")
fi

# Parse nextest's final "Summary" line into "passed failed skipped".
# Examples:
#   `     Summary [   0.045s] 12 tests run: 11 passed, 1 failed, 2 skipped`
#   `     Summary [   1.007s] 1 test run: 0 passed, 1 timed out, 74 skipped`
# A timed-out (terminated) test is reported as "timed out", not "failed", so it is
# folded into the failed count here.
parse_summary_counts() {
  local output_file="$1"

  awk '
    /Summary \[/ {
      for (i = 1; i <= NF; i++) {
        if ($i == "passed" || $i == "passed,")       passed = $(i - 1)
        else if ($i == "failed" || $i == "failed,")  failed += $(i - 1)
        else if ($i == "timed")                      failed += $(i - 1)
        else if ($i == "skipped" || $i == "skipped,") skipped = $(i - 1)
      }
    }
    END { printf "%d %d %d\n", passed, failed, skipped }
  ' "$output_file"
}

run_crate_tests() {
  local crate="$1"
  local start_seconds end_seconds elapsed_seconds
  local status output_file command_status
  local passed failed skipped

  echo -e "\n${YELLOW}=== Running unit tests for ${crate} ===${NC}"
  echo "Command: cargo nextest run -p ${crate} --test unit --no-fail-fast"
  echo "Per-test timeout: ${UNIT_TEST_TIMEOUT_SECONDS:-20}s (nextest slow-timeout)"
  echo "Parallel test threads: ${UNIT_TEST_JOBS:-2}"

  mkdir -p tmp
  output_file=$(mktemp "tmp/run_unit_tests_nextest_${crate}.XXXXXX")
  start_seconds=$(date +%s)

  # --no-fail-fast: run every test and report full counts, matching the original
  # per-test runner (nextest defaults to cancelling the rest after the first failure).
  if cargo nextest run -p "$crate" --test unit --no-fail-fast "${NEXTEST_ARGS[@]}" 2>&1 | tee "$output_file"; then
    command_status=0
    status="PASS"
  else
    command_status=${PIPESTATUS[0]}
    status="FAIL"
    OVERALL_STATUS=1
  fi

  read -r passed failed skipped < <(parse_summary_counts "$output_file")

  # A timeout / termination is reported by nextest as a failed test, so a
  # non-zero exit with no parsed failures still counts as at least one failure.
  if [[ "$command_status" -ne 0 && "$failed" -eq 0 ]]; then
    failed=1
  fi
  if [[ "$failed" -gt 0 ]]; then
    status="FAIL"
    OVERALL_STATUS=1
  fi

  end_seconds=$(date +%s)
  elapsed_seconds=$((end_seconds - start_seconds))
  rm -f "$output_file"

  SUMMARY_CRATES+=("$crate")
  SUMMARY_STATUSES+=("$status")
  SUMMARY_SECONDS+=("$elapsed_seconds")
  SUMMARY_PASSED+=("$passed")
  SUMMARY_FAILED+=("$failed")
  SUMMARY_SKIPPED+=("$skipped")
}

for crate in "${CRATES[@]}"; do
  run_crate_tests "$crate"
done

TOTAL_END_SECONDS=$(date +%s)
TOTAL_SECONDS=$((TOTAL_END_SECONDS - TOTAL_START_SECONDS))

echo -e "\n${GREEN}=== Test summary ===${NC}"
printf '%-20s %-8s %8s %8s %8s %s\n' "Crate" "Status" "Passed" "Failed" "Skipped" "Time"
printf '%-20s %-8s %8s %8s %8s %s\n' "-----" "------" "------" "------" "-------" "----"

for i in "${!SUMMARY_CRATES[@]}"; do
  printf '%-20s ' "${SUMMARY_CRATES[$i]}"
  if [[ "${SUMMARY_STATUSES[$i]}" == "PASS" ]]; then
    printf "${GREEN}%-8s${NC} " "${SUMMARY_STATUSES[$i]}"
  else
    printf "${RED}%-8s${NC} " "${SUMMARY_STATUSES[$i]}"
  fi
  printf '%8s %8s %8s %ss\n' "${SUMMARY_PASSED[$i]}" "${SUMMARY_FAILED[$i]}" "${SUMMARY_SKIPPED[$i]}" "${SUMMARY_SECONDS[$i]}"
done

echo "Total time: ${TOTAL_SECONDS}s"

if [[ "$OVERALL_STATUS" -eq 0 ]]; then
  echo -e "${GREEN}All crate unit tests passed.${NC}"
else
  echo -e "${RED}One or more crate unit test runs failed.${NC}"
fi

exit "$OVERALL_STATUS"
