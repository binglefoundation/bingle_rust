#!/usr/bin/env bash
# scripts/run_unit_tests.sh
# Unit test runner for the Bingle Rust project.
# Runs every crate's unit tests and prints a summary at the end.

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

SUMMARY_CRATES=()
SUMMARY_STATUSES=()
SUMMARY_SECONDS=()
SUMMARY_PASSED=()
SUMMARY_FAILED=()
SUMMARY_IGNORED=()
OVERALL_STATUS=0
TOTAL_START_SECONDS=$(date +%s)

parse_test_counts() {
  local output_file="$1"

  awk '
    /test result:/ {
      for (i = 1; i <= NF; i++) {
        if ($i == "passed;") {
          passed += $(i - 1)
        } else if ($i == "failed;") {
          failed += $(i - 1)
        } else if ($i == "ignored;") {
          ignored += $(i - 1)
        }
      }
    }
    END {
      printf "%d %d %d\n", passed, failed, ignored
    }
  ' "$output_file"
}

run_crate_tests() {
  local crate="$1"
  local start_seconds
  local end_seconds
  local elapsed_seconds
  local status
  local output_file
  local passed
  local failed
  local ignored

  echo -e "\n${YELLOW}=== Running unit tests for ${crate} ===${NC}"
  echo "Command: cargo test -p ${crate} --test unit"

  mkdir -p tmp
  output_file=$(mktemp "tmp/run_unit_tests_${crate}.XXXXXX")
  start_seconds=$(date +%s)
  if cargo test -p "$crate" --test unit 2>&1 | tee "$output_file"; then
    status="PASS"
  else
    status="FAIL"
    OVERALL_STATUS=1
  fi
  end_seconds=$(date +%s)
  elapsed_seconds=$((end_seconds - start_seconds))
  read -r passed failed ignored < <(parse_test_counts "$output_file")
  rm -f "$output_file"

  SUMMARY_CRATES+=("$crate")
  SUMMARY_STATUSES+=("$status")
  SUMMARY_SECONDS+=("$elapsed_seconds")
  SUMMARY_PASSED+=("$passed")
  SUMMARY_FAILED+=("$failed")
  SUMMARY_IGNORED+=("$ignored")
}

for crate in "${CRATES[@]}"; do
  run_crate_tests "$crate"
done

TOTAL_END_SECONDS=$(date +%s)
TOTAL_SECONDS=$((TOTAL_END_SECONDS - TOTAL_START_SECONDS))

echo -e "\n${GREEN}=== Test summary ===${NC}"
printf '%-20s %-8s %8s %8s %8s %s\n' "Crate" "Status" "Passed" "Failed" "Ignored" "Time"
printf '%-20s %-8s %8s %8s %8s %s\n' "-----" "------" "------" "------" "-------" "----"

for i in "${!SUMMARY_CRATES[@]}"; do
  if [[ "${SUMMARY_STATUSES[$i]}" == "PASS" ]]; then
    printf '%-20s ' "${SUMMARY_CRATES[$i]}"
    printf "${GREEN}%-8s${NC} " "${SUMMARY_STATUSES[$i]}"
    printf '%8s %8s %8s %ss\n' "${SUMMARY_PASSED[$i]}" "${SUMMARY_FAILED[$i]}" "${SUMMARY_IGNORED[$i]}" "${SUMMARY_SECONDS[$i]}"
  else
    printf '%-20s ' "${SUMMARY_CRATES[$i]}"
    printf "${RED}%-8s${NC} " "${SUMMARY_STATUSES[$i]}"
    printf '%8s %8s %8s %ss\n' "${SUMMARY_PASSED[$i]}" "${SUMMARY_FAILED[$i]}" "${SUMMARY_IGNORED[$i]}" "${SUMMARY_SECONDS[$i]}"
  fi
done

echo "Total time: ${TOTAL_SECONDS}s"

if [[ "$OVERALL_STATUS" -eq 0 ]]; then
  echo -e "${GREEN}All crate unit tests passed.${NC}"
else
  echo -e "${RED}One or more crate unit test runs failed.${NC}"
fi

exit "$OVERALL_STATUS"
