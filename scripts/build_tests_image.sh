#!/usr/bin/env bash
# build_tests_image.sh
# Purpose: Build the Docker 'tests' stage image that runs the prebuilt integration test binary.
#
# Usage:
#   scripts/build_tests_image.sh [--tag <tag>] [--test <test_name>] [--test-bin <path>] [--no-build] [--target <triple>] [--no-zig] [--platform <docker-platform>]
#
# Behavior:
# - If --test-bin is not provided, this script will:
#   1) Build the integration test binary without running it for the specified --target (default aarch64-unknown-linux-musl).
#      By default we use cargo-zigbuild for musl cross-compilation. Pass --no-zig to use plain cargo.
#   2) Auto-detect the resulting binary path under target/<triple>/debug/deps/${TEST_NAME}-*
# - Builds the Docker image using the tests stage:
#     docker buildx build --platform <platform> --target tests -t <tag> --build-arg TEST_BIN_PATH=<path> -f Dockerfile.tests .
# - By default, tag is bingle-tests:local and platform is linux/arm64.
# - Set NO_COLOR=1 to reduce cargo color output
# - Use --no-build to skip the cargo build step and only (re)build the Docker image (requires --test-bin or previous build)
#
set -euo pipefail

TAG="bingle-tests:local"
TEST_BIN_PATH=""
DO_CARGO_BUILD=1
TARGET_TRIPLE="aarch64-unknown-linux-musl"
USE_ZIG=1
DOCKER_PLATFORM="linux/arm64"
TEST_NAME="endpoint_available"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      TAG="$2"; shift 2;;
    --test)
      TEST_NAME="$2"; shift 2;;
    --test-bin)
      TEST_BIN_PATH="$2"; shift 2;;
    --no-build)
      DO_CARGO_BUILD=0; shift;;
    --target)
      TARGET_TRIPLE="$2"; shift 2;;
    --no-zig)
      USE_ZIG=0; shift;;
    --platform)
      DOCKER_PLATFORM="$2"; shift 2;;
    -h|--help)
      echo "Usage: $0 [--tag <tag>] [--test <test_name>] [--test-bin <path>] [--no-build] [--target <triple>] [--no-zig] [--platform <docker-platform>]"; exit 0;;
    *)
      echo "Unknown argument: $1" >&2; exit 2;;
  esac
done

# 1) Build the test binary (unless skipped)
if [[ $DO_CARGO_BUILD -eq 1 ]]; then
  echo "[build-tests-image] Building test binary for target '$TARGET_TRIPLE'"
  if [[ $USE_ZIG -eq 1 ]]; then
    if ! command -v cargo-zigbuild >/dev/null 2>&1; then
      echo "[build-tests-image] cargo-zigbuild not found; installing (cargo install cargo-zigbuild)"
      cargo install cargo-zigbuild
    fi
    # Ensure the rust target is added
    rustup target add "$TARGET_TRIPLE" || true
    echo "[build-tests-image] Using: cargo zigbuild --test $TEST_NAME --target $TARGET_TRIPLE"
    NO_COLOR=1 cargo zigbuild --test "$TEST_NAME" --target "$TARGET_TRIPLE"
  else
    # Plain cargo build path (requires toolchain/linker set up for cross)
    rustup target add "$TARGET_TRIPLE" || true
    echo "[build-tests-image] Using: cargo test --target $TARGET_TRIPLE --test $TEST_NAME --no-run"
    NO_COLOR=1 cargo test --target "$TARGET_TRIPLE" --test "$TEST_NAME" --no-run
  fi
fi

# 2) Resolve TEST_BIN_PATH if not provided
if [[ -z "$TEST_BIN_PATH" ]]; then
  # Try target-specific locations first, then generic
  CANDIDATES=(
    "target/${TARGET_TRIPLE}/debug/deps/${TEST_NAME}-*"
    target/*/debug/deps/${TEST_NAME}-*
    target/debug/deps/${TEST_NAME}-*
  )
  for pat in "${CANDIDATES[@]}"; do
    # Use globbing safely even if pattern doesn't match
    for f in $pat; do
      if [[ -f "${f:-}" && -x "${f:-}" ]]; then
        TEST_BIN_PATH="$f"
        break 2
      fi
    done
  done
fi

if [[ -z "$TEST_BIN_PATH" ]]; then
  echo "[build-tests-image][ERROR] Could not locate the compiled test binary. Provide --test-bin <path> or ensure the test is built." >&2
  exit 3
fi

# Normalize the path (remove leading ./ if present)
TEST_BIN_PATH="${TEST_BIN_PATH#./}"

if [[ ! -f "$TEST_BIN_PATH" ]]; then
  echo "[build-tests-image][ERROR] Test binary not found at: $TEST_BIN_PATH" >&2
  exit 3
fi

if [[ ! -x "$TEST_BIN_PATH" ]]; then
  echo "[build-tests-image][WARN] Test binary at $TEST_BIN_PATH is not marked executable; continuing."
fi

# Show ldd info if available (debugging dynamic vs static)
if command -v file >/dev/null 2>&1; then
  echo "[build-tests-image] Binary type: $(file -h "$TEST_BIN_PATH" | sed -n '1p')"
fi

echo "[build-tests-image] Using TEST_BIN_PATH=$TEST_BIN_PATH"
echo "[build-tests-image] Building Docker tests image: $TAG for platform $DOCKER_PLATFORM"

# 3) Build the Docker image using the 'tests' stage
# Note: buildx enables cross-platform base image resolution
DOCKER_BUILDKIT=1 docker buildx build --platform "$DOCKER_PLATFORM" \
  --target tests \
  -t "$TAG" \
  --build-arg TEST_BIN_PATH="$TEST_BIN_PATH" \
  -f Dockerfile.tests \
  .

echo "[build-tests-image] Done. Image: $TAG"