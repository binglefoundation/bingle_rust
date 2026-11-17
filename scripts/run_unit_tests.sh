#!/usr/bin/env bash
# scripts/run_unit_tests.sh
# Unit test runner for the Bingle Rust project.
# Runs every crate's unit tests and prints a summary at the end.
# Each test must finish within 20 seconds unless UNIT_TEST_TIMEOUT_SECONDS is set.

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
DEFAULT_UNIT_TEST_TIMEOUT_SECONDS=${UNIT_TEST_TIMEOUT_SECONDS:-20}
UNIT_TEST_JOBS=${UNIT_TEST_JOBS:-2}

run_with_timeout() {
  local timeout_seconds="$1"
  shift

  perl -e '
    use strict;
    use warnings;
    use POSIX qw(setsid);

    my $timeout_seconds = shift @ARGV;
    my $pid = fork();
    die "fork failed: $!\n" unless defined $pid;

    if ($pid == 0) {
      setsid() or die "setsid failed: $!\n";
      exec @ARGV or die "exec failed: $!\n";
    }

    local $SIG{ALRM} = sub {
      print STDERR "Timed out after ${timeout_seconds}s\n";
      kill "TERM", -$pid;
      sleep 1;
      kill "KILL", -$pid;
      exit 124;
    };

    alarm $timeout_seconds;
    waitpid($pid, 0);
    alarm 0;

    if ($? & 127) {
      exit 128 + ($? & 127);
    }
    exit $? >> 8;
  ' "$timeout_seconds" "$@"
}

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

unit_test_names_for_executable() {
  local test_executable="$1"

  "$test_executable" --list --format terse | sed -n 's/: test$//p'
}

run_single_unit_test() {
  local test_name="$1"
  local test_output_file
  local result_file
  local command_status
  local test_passed
  local test_failed
  local test_ignored

  test_output_file=$(mktemp "${TEST_RESULT_DIR}/output.XXXXXX")
  result_file="${test_output_file}.result"

  echo "Running ${test_name}"
  if run_with_timeout "$TIMEOUT_SECONDS" "$TEST_EXECUTABLE" "$test_name" --exact --format terse > "$test_output_file" 2>&1; then
    command_status=0
  else
    command_status=$?
  fi

  cat "$test_output_file"
  read -r test_passed test_failed test_ignored < <(parse_test_counts "$test_output_file")

  if [[ "$command_status" -eq 124 ]]; then
    echo "${test_name} timed out after ${TIMEOUT_SECONDS}s."
    test_failed=$((test_failed + 1))
  elif [[ "$command_status" -ne 0 && "$test_failed" -eq 0 ]]; then
    test_failed=$((test_failed + 1))
  fi

  printf '%d %d %d %d\n' "$command_status" "$test_passed" "$test_failed" "$test_ignored" > "$result_file"
  exit "$command_status"
}

export -f run_with_timeout
export -f parse_test_counts
export -f run_single_unit_test

run_crate_tests() {
  local crate="$1"
  local start_seconds
  local end_seconds
  local elapsed_seconds
  local status
  local output_file
  local result_dir
  local test_names_file
  local passed
  local failed
  local ignored
  local test_executable
  local command_status
  local timeout_seconds
  local result_command_status
  local result_passed
  local result_failed
  local result_ignored
  local results_seen

  echo -e "\n${YELLOW}=== Running unit tests for ${crate} ===${NC}"
  echo "Command: cargo test -p ${crate} --test unit"
  echo "Per-test timeout: ${DEFAULT_UNIT_TEST_TIMEOUT_SECONDS}s"
  echo "Parallel test jobs: ${UNIT_TEST_JOBS}"

  mkdir -p tmp
  output_file=$(mktemp "tmp/run_unit_tests_${crate}.XXXXXX")
  result_dir=$(mktemp -d "tmp/run_unit_tests_${crate}_results.XXXXXX")
  start_seconds=$(date +%s)
  passed=0
  failed=0
  ignored=0
  timeout_seconds="$DEFAULT_UNIT_TEST_TIMEOUT_SECONDS"

  if ! cargo test -p "$crate" --test unit --no-run 2>&1 | tee "$output_file"; then
    status="FAIL"
    OVERALL_STATUS=1
    test_executable=""
  else
    test_executable=$(sed -n 's/^  Executable .* (\(.*\))$/\1/p' "$output_file" | tail -n 1)
  fi

  if [[ "${status:-}" == "FAIL" ]]; then
    :
  elif [[ -z "$test_executable" ]]; then
    echo "Could not determine unit test executable for ${crate}." | tee -a "$output_file"
    status="FAIL"
    OVERALL_STATUS=1
  else
    status="PASS"
    test_names_file=$(mktemp "tmp/run_unit_tests_${crate}_names.XXXXXX")

    if ! unit_test_names_for_executable "$test_executable" > "$test_names_file"; then
      echo "Could not list unit tests for ${crate}." | tee -a "$output_file"
      status="FAIL"
      OVERALL_STATUS=1
    else
      export TEST_EXECUTABLE="$test_executable"
      export TIMEOUT_SECONDS="$timeout_seconds"
      export TEST_RESULT_DIR="$result_dir"

      if xargs -n 1 -P "$UNIT_TEST_JOBS" bash -c 'run_single_unit_test "$1"' _ < "$test_names_file" 2>&1 | tee -a "$output_file"; then
        :
      else
        status="FAIL"
        OVERALL_STATUS=1
      fi

      results_seen=0
      for result_file in "$result_dir"/*.result; do
        [[ -e "$result_file" ]] || continue
        results_seen=1
        read -r result_command_status result_passed result_failed result_ignored < "$result_file"
        passed=$((passed + result_passed))
        failed=$((failed + result_failed))
        ignored=$((ignored + result_ignored))

        if [[ "$result_command_status" -ne 0 ]]; then
          status="FAIL"
          OVERALL_STATUS=1
        fi
      done

      if [[ "$results_seen" -eq 0 ]]; then
        echo "No unit test results were produced for ${crate}." | tee -a "$output_file"
        status="FAIL"
        OVERALL_STATUS=1
      fi
    fi

    rm -f "$test_names_file"
  fi

  if [[ "$failed" -gt 0 ]]; then
    status="FAIL"
    OVERALL_STATUS=1
  fi
  end_seconds=$(date +%s)
  elapsed_seconds=$((end_seconds - start_seconds))
  rm -f "$output_file"
  rm -rf "$result_dir"

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
