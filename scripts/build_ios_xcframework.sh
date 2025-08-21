#!/usr/bin/env bash
set -euo pipefail

# Build Rust static libraries for iOS (device and simulators) and package into an XCFramework.
# Prereqs: Xcode command line tools, rustup, and iOS Rust targets installed.
# Usage:
#   bash scripts/build_ios_xcframework.sh
# Output:
#   ios/RustCommsFFI.xcframework

CRATE_NAME="rust_comms"
ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"
INCLUDE_DIR_RUST="$ROOT_DIR/include"
INCLUDE_DIR_TEST="$ROOT_DIR/include_bingle_test"
IOS_DIR="$ROOT_DIR/ios"
BUILD_DIR="$ROOT_DIR/target"

# Ensure required tools and targets are available
have_cmd() { command -v "$1" >/dev/null 2>&1; }
require_cmd() {
  if ! have_cmd "$1"; then
    echo "Error: '$1' not found. Please install it and try again." >&2
    if [ "$1" = "rustup" ]; then
      echo "Install Rust via rustup: https://rustup.rs" >&2
    fi
    exit 1
  fi
}

ensure_target() {
  local target="$1"
  # Use the active rustup toolchain if available
  local tc="${ACTIVE_TOOLCHAIN:-}"
  local list_cmd=(rustup target list --installed)
  local add_cmd=(rustup target add)
  if [[ -n "$tc" ]]; then
    list_cmd=(rustup target list --installed --toolchain "$tc")
    add_cmd=(rustup target add --toolchain "$tc")
  fi
  if "${list_cmd[@]}" | grep -qx "$target"; then
    return 0
  fi
  echo "Installing Rust target for toolchain '${tc:-default}': $target"
  if ! "${add_cmd[@]}" "$target"; then
    echo "Error: Failed to install Rust target '$target' for toolchain '${tc:-default}'." >&2
    echo "Please run: rustup target add ${tc:+--toolchain $tc }$target" >&2
    exit 1
  fi
  # Verify installation succeeded
  if ! "${list_cmd[@]}" | grep -qx "$target"; then
    echo "Error: Rust target '$target' is still not installed after attempting installation (toolchain '${tc:-default}')." >&2
    exit 1
  fi
}

require_cmd rustup
require_cmd cargo
require_cmd rustc

# Determine active rustup toolchain and prefer rustup-managed cargo/rustc
ACTIVE_TOOLCHAIN="$(rustup show active-toolchain 2>/dev/null | awk '{print $1}')"
if [[ -z "${ACTIVE_TOOLCHAIN:-}" ]]; then
  echo "Error: Could not determine active rustup toolchain. Is rustup installed and initialized?" >&2
  exit 1
fi

RUSTUP_CARGO_PATH="$(rustup which cargo 2>/dev/null || true)"
SYSTEM_CARGO_PATH="$(command -v cargo || true)"
if [[ -n "$SYSTEM_CARGO_PATH" && -n "$RUSTUP_CARGO_PATH" && "$SYSTEM_CARGO_PATH" != "$RUSTUP_CARGO_PATH" ]]; then
  echo "Warning: Your cargo ($SYSTEM_CARGO_PATH) is not the rustup-managed cargo ($RUSTUP_CARGO_PATH)." >&2
  echo "         Proceeding with: rustup run $ACTIVE_TOOLCHAIN cargo" >&2
fi

# Helper commands pinned to the active toolchain
CARGO_CMD=(rustup run "$ACTIVE_TOOLCHAIN" cargo)
RUSTC_CMD=(rustup run "$ACTIVE_TOOLCHAIN" rustc)

# Mandatory iOS targets (install for the active toolchain)
ensure_target aarch64-apple-ios
ensure_target aarch64-apple-ios-sim

# x86_64 simulator is optional on Apple Silicon machines; only attempt if supported by this toolchain
if "${RUSTC_CMD[@]}" --print target-list | grep -q "^x86_64-apple-ios$"; then
  # Try to install but don't fail the whole build if it can't be installed
  if ! rustup target list --installed --toolchain "$ACTIVE_TOOLCHAIN" | grep -qx "x86_64-apple-ios"; then
    rustup target add --toolchain "$ACTIVE_TOOLCHAIN" x86_64-apple-ios || true
  fi
fi

# Build release static libraries for each target via rustup toolchain
"${CARGO_CMD[@]}" build --release --target aarch64-apple-ios
"${CARGO_CMD[@]}" build --release --target aarch64-apple-ios-sim
if "${RUSTC_CMD[@]}" --print target-list | grep -q "^x86_64-apple-ios$"; then
  "${CARGO_CMD[@]}" build --release --target x86_64-apple-ios || true
fi

LIB_DEVICE="$BUILD_DIR/aarch64-apple-ios/release/lib${CRATE_NAME}.a"
LIB_SIM_ARM64="$BUILD_DIR/aarch64-apple-ios-sim/release/lib${CRATE_NAME}.a"
LIB_SIM_X64="$BUILD_DIR/x86_64-apple-ios/release/lib${CRATE_NAME}.a"

if [[ ! -f "$LIB_DEVICE" ]]; then
  echo "Missing device library: $LIB_DEVICE" >&2
  exit 1
fi
if [[ ! -f "$LIB_SIM_ARM64" ]]; then
  echo "Missing simulator ARM64 library: $LIB_SIM_ARM64" >&2
  exit 1
fi

# Prepare XCFramework output dir
mkdir -p "$IOS_DIR"
XCFRAMEWORK_PATH="$IOS_DIR/RustCommsFFI.xcframework"
rm -rf "$XCFRAMEWORK_PATH"

# If we have both sim archs, lipo them; otherwise just use arm64-sim
SIM_UNIV_LIB="$BUILD_DIR/ios-sim-universal/lib${CRATE_NAME}.a"
if [[ -f "$LIB_SIM_X64" ]]; then
  mkdir -p "$(dirname "$SIM_UNIV_LIB")"
  lipo -create -output "$SIM_UNIV_LIB" "$LIB_SIM_ARM64" "$LIB_SIM_X64"
  SIM_LIB_TO_USE="$SIM_UNIV_LIB"
else
  SIM_LIB_TO_USE="$LIB_SIM_ARM64"
fi

# Create a temporary headers dir for rust_comms without module.modulemap to avoid collisions
TMP_RUST_HEADERS="$BUILD_DIR/ios-tmp-headers/rust"
rm -rf "$TMP_RUST_HEADERS"
mkdir -p "$TMP_RUST_HEADERS"
cp "$INCLUDE_DIR_RUST/rust_comms.h" "$TMP_RUST_HEADERS/"

# Create XCFramework for rust_comms using the temp headers (no module.modulemap)
xcodebuild -create-xcframework \
  -library "$LIB_DEVICE" -headers "$TMP_RUST_HEADERS" \
  -library "$SIM_LIB_TO_USE" -headers "$TMP_RUST_HEADERS" \
  -output "$XCFRAMEWORK_PATH"

echo "Created $XCFRAMEWORK_PATH"

# Now build and package the bingle_test crate as a separate XCFramework
TEST_CRATE_NAME="bingle_test"

# Build bingle_test for the same targets
"${CARGO_CMD[@]}" build -p "$TEST_CRATE_NAME" --release --target aarch64-apple-ios
"${CARGO_CMD[@]}" build -p "$TEST_CRATE_NAME" --release --target aarch64-apple-ios-sim
if "${RUSTC_CMD[@]}" --print target-list | grep -q "^x86_64-apple-ios$"; then
  "${CARGO_CMD[@]}" build -p "$TEST_CRATE_NAME" --release --target x86_64-apple-ios || true
fi

LIB_DEVICE_TEST="$BUILD_DIR/aarch64-apple-ios/release/lib${TEST_CRATE_NAME}.a"
LIB_SIM_ARM64_TEST="$BUILD_DIR/aarch64-apple-ios-sim/release/lib${TEST_CRATE_NAME}.a"
LIB_SIM_X64_TEST="$BUILD_DIR/x86_64-apple-ios/release/lib${TEST_CRATE_NAME}.a"

if [[ ! -f "$LIB_DEVICE_TEST" ]]; then
  echo "Missing device library: $LIB_DEVICE_TEST" >&2
  exit 1
fi
if [[ ! -f "$LIB_SIM_ARM64_TEST" ]]; then
  echo "Missing simulator ARM64 library: $LIB_SIM_ARM64_TEST" >&2
  exit 1
fi

SIM_UNIV_LIB_TEST="$BUILD_DIR/ios-sim-universal/lib${TEST_CRATE_NAME}.a"
if [[ -f "$LIB_SIM_X64_TEST" ]]; then
  mkdir -p "$(dirname "$SIM_UNIV_LIB_TEST")"
  lipo -create -output "$SIM_UNIV_LIB_TEST" "$LIB_SIM_ARM64_TEST" "$LIB_SIM_X64_TEST"
  SIM_LIB_TO_USE_TEST="$SIM_UNIV_LIB_TEST"
else
  SIM_LIB_TO_USE_TEST="$LIB_SIM_ARM64_TEST"
fi

XCFRAMEWORK_PATH_TEST="$IOS_DIR/BingleTestFFI.xcframework"
rm -rf "$XCFRAMEWORK_PATH_TEST"

xcodebuild -create-xcframework \
  -library "$LIB_DEVICE_TEST" -headers "$INCLUDE_DIR_TEST" \
  -library "$SIM_LIB_TO_USE_TEST" -headers "$INCLUDE_DIR_TEST" \
  -output "$XCFRAMEWORK_PATH_TEST"

echo "Created $XCFRAMEWORK_PATH_TEST"

# Create or update a SwiftPM package manifest to include both binary targets
PKG_SWIFT="$IOS_DIR/Package.swift"
cat > "$PKG_SWIFT" <<'SWIFT'
// swift-tools-version:5.7
import PackageDescription

let package = Package(
    name: "RustCommsFFI",
    platforms: [
        .iOS(.v13)
    ],
    products: [
        // You can depend on either product, or both in your app/test target
        .library(name: "RustCommsFFI", targets: ["RustCommsFFI"]),
        .library(name: "BingleTestFFI", targets: ["BingleTestFFI"]) 
    ],
    targets: [
        .binaryTarget(name: "RustCommsFFI", path: "RustCommsFFI.xcframework"),
        .binaryTarget(name: "BingleTestFFI", path: "BingleTestFFI.xcframework"),
        .testTarget(
                    name: "RustCommsFFITests",
                    dependencies: ["RustCommsFFI","BingleTestFFI"],
                    path: "Tests/RustCommsFFITests"
                )
    ]
)
SWIFT

echo "SwiftPM package written at $IOS_DIR. In Xcode: File -> Open, select Package.swift."
