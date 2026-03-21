#!/usr/bin/env bash
# build_webserver_image.sh
# Purpose: Build the Docker 'webserver' stage image that runs the bingle_webserver binary.
#
# Usage:
#   scripts/build_webserver_image.sh [--tag <tag>] [--no-build] [--target <triple>] [--no-zig] [--platform <docker-platform>]

set -euo pipefail

TAG="bingle-webserver:local"
DO_CARGO_BUILD=1
TARGET_TRIPLE="aarch64-unknown-linux-musl"
USE_ZIG=1
DOCKER_PLATFORM="linux/arm64"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      TAG="$2"; shift 2;;
    --no-build)
      DO_CARGO_BUILD=0; shift;;
    --target)
      TARGET_TRIPLE="$2"; shift 2;;
    --no-zig)
      USE_ZIG=0; shift;;
    --platform)
      DOCKER_PLATFORM="$2"; shift 2;;
    -h|--help)
      echo "Usage: $0 [--tag <tag>] [--no-build] [--target <triple>] [--no-zig] [--platform <docker-platform>]"; exit 0;;
    *)
      echo "Unknown argument: $1" >&2; exit 2;;
  esac
done

# 1) Build the webserver binary (unless skipped)
if [[ $DO_CARGO_BUILD -eq 1 ]]; then
  echo "[build-webserver-image] Building bingle_webserver for target '$TARGET_TRIPLE'"
  if [[ $USE_ZIG -eq 1 ]]; then
    if ! command -v cargo-zigbuild >/dev/null 2>&1; then
      echo "[build-webserver-image] cargo-zigbuild not found; installing (cargo install cargo-zigbuild)"
      cargo install cargo-zigbuild
    fi
    rustup target add "$TARGET_TRIPLE" || true
    echo "[build-webserver-image] Using: cargo zigbuild -p bingle_webserver --target $TARGET_TRIPLE"
    NO_COLOR=1 cargo zigbuild -p bingle_webserver --target "$TARGET_TRIPLE"
  else
    rustup target add "$TARGET_TRIPLE" || true
    echo "[build-webserver-image] Using: cargo build -p bingle_webserver --target $TARGET_TRIPLE"
    NO_COLOR=1 cargo build -p bingle_webserver --target "$TARGET_TRIPLE"
  fi
fi

# 2) Resolve WEB_BIN_PATH
WEB_BIN_PATH="target/${TARGET_TRIPLE}/debug/bingle_webserver"

if [[ ! -f "$WEB_BIN_PATH" ]]; then
  echo "[build-webserver-image][ERROR] Webserver binary not found at: $WEB_BIN_PATH" >&2
  exit 3
fi

echo "[build-webserver-image] Building Docker webserver image: $TAG for platform $DOCKER_PLATFORM"

# 3) Build the Docker image using the 'webserver' stage
DOCKER_BUILDKIT=1 docker buildx build --platform "$DOCKER_PLATFORM" \
  --target webserver \
  -t "$TAG" \
  --build-arg WEB_BIN_PATH="$WEB_BIN_PATH" \
  -f Dockerfile \
  .

echo "[build-webserver-image] Done. Image: $TAG"
