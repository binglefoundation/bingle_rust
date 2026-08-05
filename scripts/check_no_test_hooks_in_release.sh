#!/usr/bin/env bash
# scripts/check_no_test_hooks_in_release.sh
# CI guard: ensure the test-only unsafe accessor never compiles into production.
#
# `BingleAccessUnsafeForTests::access_unsafe_for_tests` is gated behind the
# bingle_core `test-hooks` feature (see bingle_core/src/engine/mod.rs and the
# self dev-dependency in bingle_core/Cargo.toml). This script fails CI if:
#   1. any production binary fails to build in release (would mean production
#      code still references the now-gated accessor), or
#   2. the `test-hooks` feature has leaked into a release build graph via Cargo
#      feature unification.
#
# Run from the repo root.

set -uo pipefail

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

# Production build targets that must never pull in test-hooks.
BINARIES=(
  "-p bingle_webserver"
  "-p bingle_jsi"
  "-p bingle_cli --bin bingle_cli"
)

fail() {
  echo -e "${RED}FAIL:${NC} $1" >&2
  exit 1
}

echo "== 1/2 release build of production binaries (fails if any references the gated accessor) =="
for target in "${BINARIES[@]}"; do
  echo "  building: cargo build --release ${target}"
  # shellcheck disable=SC2086
  cargo build --release ${target} || fail "release build failed for '${target}'"
done

echo "== 2/2 asserting test-hooks has not leaked into the release feature graph =="
for target in "${BINARIES[@]}"; do
  # shellcheck disable=SC2086
  if cargo tree -e features --release ${target} 2>/dev/null | grep -q 'bingle_core feature "test-hooks"'; then
    fail "'test-hooks' feature leaked into release graph for '${target}'"
  fi
  echo "  clean: no test-hooks in release graph for '${target}'"
done

echo -e "${GREEN}OK:${NC} production release builds are free of the test-only accessor."
