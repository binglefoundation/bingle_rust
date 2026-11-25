#!/usr/bin/env bash
# Run the shell script tests: every scripts/tests/test_*.sh, each in its own bash. They run offline,
# with no AWS, Docker or network (issue #528 added the first, for aws/lib/changeset.sh).
#
#   scripts/tests/run_shell_tests.sh
#
# A test script exits non-zero when any of its cases fails; this runner exits non-zero when any
# test script does.
set -uo pipefail

TESTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
failed=0
for t in "$TESTS_DIR"/test_*.sh; do
  echo "== ${t#"$TESTS_DIR"/}"
  bash "$t" || failed=1
done
if [[ $failed -ne 0 ]]; then
  echo "shell tests: FAILED"
  exit 1
fi
echo "shell tests: all passed"
