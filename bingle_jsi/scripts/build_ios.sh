#!/usr/bin/env bash
set -euo pipefail
# Build the bingle_jsi Rust crate for iOS (device + simulator) and generate
# Swift bindings via uniffi-bindgen, then package into an XCFramework.
#
# Prerequisites:
#   - Xcode command-line tools
#   - rustup with iOS targets (installed automatically if missing)
#   - uniffi-bindgen is built from the bingle_jsi crate (no separate install needed)
#
# Usage:
#   bash bingle_jsi/scripts/build_ios.sh
#
# Output:
#   bingle_jsi/ios/BingleJsi.xcframework   -- fat static library
#   bingle_jsi/ios/generated/               -- Swift bindings + C header + modulemap

CRATE_NAME="bingle_jsi"
ROOT_DIR="$(cd "$(dirname "$0")"/../.. && pwd)"
JSI_DIR="$ROOT_DIR/bingle_jsi"
IOS_DIR="$JSI_DIR/ios"
BUILD_DIR="$ROOT_DIR/target"
GENERATED_DIR="$IOS_DIR/generated"

# ── helpers ───────────────────────────────────────────────────────────

have_cmd() { command -v "$1" >/dev/null 2>&1; }

require_cmd() {
  if ! have_cmd "$1"; then
    echo "Error: '$1' not found. Please install it and try again." >&2
    exit 1
  fi
}

ensure_target() {
  local target="$1"
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
  echo "Installing Rust target: $target"
  "${add_cmd[@]}" "$target"
}

require_cmd rustup
require_cmd cargo
require_cmd xcodebuild

ACTIVE_TOOLCHAIN="$(rustup show active-toolchain 2>/dev/null | awk '{print $1}')"
if [[ -z "${ACTIVE_TOOLCHAIN:-}" ]]; then
  echo "Error: Could not determine active rustup toolchain." >&2
  exit 1
fi

CARGO_CMD=(rustup run "$ACTIVE_TOOLCHAIN" cargo)
RUSTC_CMD=(rustup run "$ACTIVE_TOOLCHAIN" rustc)

export IPHONEOS_DEPLOYMENT_TARGET=13.0

# ── install targets ───────────────────────────────────────────────────

ensure_target aarch64-apple-ios
ensure_target aarch64-apple-ios-sim

# x86_64 simulator is optional (for Intel Macs / Rosetta)
HAS_X64_SIM=false
if "${RUSTC_CMD[@]}" --print target-list | grep -q "^x86_64-apple-ios$"; then
  if rustup target list --installed --toolchain "$ACTIVE_TOOLCHAIN" | grep -qx "x86_64-apple-ios" \
     || rustup target add --toolchain "$ACTIVE_TOOLCHAIN" x86_64-apple-ios 2>/dev/null; then
    HAS_X64_SIM=true
  fi
fi

# ── build static libraries ───────────────────────────────────────────

echo "Building $CRATE_NAME for aarch64-apple-ios..."
"${CARGO_CMD[@]}" build -p "$CRATE_NAME" --lib --release --target aarch64-apple-ios

echo "Building $CRATE_NAME for aarch64-apple-ios-sim..."
"${CARGO_CMD[@]}" build -p "$CRATE_NAME" --lib --release --target aarch64-apple-ios-sim

if $HAS_X64_SIM; then
  echo "Building $CRATE_NAME for x86_64-apple-ios..."
  "${CARGO_CMD[@]}" build -p "$CRATE_NAME" --lib --release --target x86_64-apple-ios || true
fi

LIB_DEVICE="$BUILD_DIR/aarch64-apple-ios/release/libBingleJsi.a"
cp "$BUILD_DIR/aarch64-apple-ios/release/lib${CRATE_NAME}.a" "$LIB_DEVICE"

LIB_SIM_ARM64="$BUILD_DIR/aarch64-apple-ios-sim/release/libBingleJsi.a"
cp "$BUILD_DIR/aarch64-apple-ios-sim/release/lib${CRATE_NAME}.a" "$LIB_SIM_ARM64"

LIB_SIM_X64="$BUILD_DIR/x86_64-apple-ios/release/libBingleJsi.a"
if $HAS_X64_SIM && [[ -f "$BUILD_DIR/x86_64-apple-ios/release/lib${CRATE_NAME}.a" ]]; then
  cp "$BUILD_DIR/x86_64-apple-ios/release/lib${CRATE_NAME}.a" "$LIB_SIM_X64"
fi

for lib in "$LIB_DEVICE" "$LIB_SIM_ARM64"; do
  if [[ ! -f "$lib" ]]; then
    echo "Missing library: $lib" >&2
    exit 1
  fi
done

# ── generate Swift bindings via uniffi-bindgen ────────────────────────

echo "Generating Swift bindings..."
rm -rf "$GENERATED_DIR"
mkdir -p "$GENERATED_DIR"

# uniffi-bindgen is provided as a binary target in the bingle_jsi crate
# (src/bin/uniffi-bindgen.rs) via the uniffi "cli" feature.
"${CARGO_CMD[@]}" run -p bingle_jsi --bin uniffi-bindgen -- generate \
  --library "$LIB_DEVICE" \
  --language swift \
  --out-dir "$GENERATED_DIR"

echo "Swift bindings generated in $GENERATED_DIR"

# ── create C header directory for XCFramework ─────────────────────────

HEADERS_DIR="$BUILD_DIR/ios-bingle-jsi-headers"
rm -rf "$HEADERS_DIR"
mkdir -p "$HEADERS_DIR"

# Copy the generated C header(s) and create a module map
if ls "$GENERATED_DIR"/*.h 1>/dev/null 2>&1; then
  cp "$GENERATED_DIR"/*.h "$HEADERS_DIR/"
fi

# Create modulemap for the XCFramework
HEADER_FILES=$(cd "$HEADERS_DIR" && ls *.h 2>/dev/null || true)
cat > "$HEADERS_DIR/module.modulemap" <<EOF
module bingle_jsiFFI [system] {
$(for h in $HEADER_FILES; do echo "  header \"$h\""; done)
  export *
}
EOF

# ── create universal simulator lib (lipo) ─────────────────────────────

if $HAS_X64_SIM && [[ -f "$LIB_SIM_X64" ]]; then
  SIM_UNIV_LIB="$BUILD_DIR/ios-sim-universal/libBingleJsi.a"
  mkdir -p "$(dirname "$SIM_UNIV_LIB")"
  lipo -create -output "$SIM_UNIV_LIB" "$LIB_SIM_ARM64" "$LIB_SIM_X64"
  SIM_LIB="$SIM_UNIV_LIB"
else
  SIM_LIB="$LIB_SIM_ARM64"
fi

# ── create XCFramework ────────────────────────────────────────────────

XCFRAMEWORK_PATH="$IOS_DIR/BingleJsi.xcframework"
rm -rf "$XCFRAMEWORK_PATH"

xcodebuild -create-xcframework \
  -library "$LIB_DEVICE" -headers "$HEADERS_DIR" \
  -library "$SIM_LIB" -headers "$HEADERS_DIR" \
  -output "$XCFRAMEWORK_PATH"

echo ""
echo "=== iOS build complete ==="
echo "XCFramework: $XCFRAMEWORK_PATH"
echo "Swift bindings: $GENERATED_DIR/"
echo ""
echo "To use in a React Native project:"
echo "  1. Add this module as an npm dependency (see package.json)"
echo "  2. Run 'cd ios && pod install'"
echo "  3. The Swift bindings and XCFramework are linked automatically via the podspec"
