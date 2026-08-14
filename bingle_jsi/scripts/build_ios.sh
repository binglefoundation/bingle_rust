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
#   BINGLE_IOS_SIM_ONLY=1 bash bingle_jsi/scripts/build_ios.sh   # fast: Apple-silicon simulator
#                                                                # slice only (local Detox iteration;
#                                                                # not the shipped/committed artifact)
#
# Output:
#   bingle_jsi/ios/BingleJsi.xcframework   -- fat static library
#   bingle_jsi/ios/generated/               -- Swift bindings + C header + modulemap

CRATE_NAME="bingle_jsi"
ROOT_DIR="$(cd "$(dirname "$0")"/../.. && pwd)"
JSI_DIR="$ROOT_DIR/bingle_jsi"
IOS_DIR="$JSI_DIR/ios"
# Build into a username-free target dir. Vendored OpenSSL bakes its
# OPENSSLDIR/ENGINESDIR/MODULESDIR constants from OUT_DIR at C-compile time;
# --remap-path-prefix (a rustc flag) cannot rewrite those, so if OUT_DIR sat
# under $ROOT_DIR the builder's home path would leak into the shipped .a libs.
# Overridable by setting CARGO_TARGET_DIR in the environment.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/var/tmp/bingle_native_target}"
BUILD_DIR="$CARGO_TARGET_DIR"
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

# ── strip local build paths from the shipped libraries ────────────────
# Dependency panic-location strings and debuginfo embed absolute source
# paths, mostly under $CARGO_HOME (~/.cargo/registry). Left as-is these
# leak the builder's username and filesystem layout into the .a static
# libs packaged in the XCFramework. Remap them to stable placeholders,
# computed from the live environment so no personal path is committed.
REMAP_FLAGS="--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo --remap-path-prefix=${RUSTUP_HOME:-$HOME/.rustup}=/rustup --remap-path-prefix=$ROOT_DIR=/bingle"

# Per-target RUSTFLAGS. Note: setting RUSTFLAGS overrides (does not merge
# with) the per-target rustflags in .cargo/config.toml, so the iOS
# deployment-target link args that normally live there must be repeated
# here — keep them in sync with .cargo/config.toml. $1 is the clang
# `-target` triple for the given Rust target.
ios_rustflags() {
  echo "$REMAP_FLAGS -C link-arg=-target -C link-arg=$1"
}

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

# Fast local-iteration mode: build only the Apple-silicon simulator slice and package a
# simulator-only XCFramework. Skips the device (aarch64-apple-ios) and Intel-simulator
# (x86_64-apple-ios) slices, so a rebuild is ~3x faster. Use for Detox e2e on an Apple-silicon
# Mac (`BINGLE_IOS_SIM_ONLY=1`). Do NOT commit the resulting sim-only XCFramework as the shipped
# artifact — that one must be the full build (device + both simulator archs).
SIM_ONLY=false
if [[ "${BINGLE_IOS_SIM_ONLY:-0}" == "1" ]]; then
  SIM_ONLY=true
  HAS_X64_SIM=false
  echo "BINGLE_IOS_SIM_ONLY=1 -> building simulator slice only (aarch64-apple-ios-sim)."
fi

if ! $SIM_ONLY; then
  echo "Building $CRATE_NAME for aarch64-apple-ios..."
  RUSTFLAGS="$(ios_rustflags arm64-apple-ios13.0)" \
    "${CARGO_CMD[@]}" build -p "$CRATE_NAME" --lib --release --target aarch64-apple-ios
fi

echo "Building $CRATE_NAME for aarch64-apple-ios-sim..."
RUSTFLAGS="$(ios_rustflags arm64-apple-ios13.0-simulator)" \
  "${CARGO_CMD[@]}" build -p "$CRATE_NAME" --lib --release --target aarch64-apple-ios-sim

if $HAS_X64_SIM; then
  echo "Building $CRATE_NAME for x86_64-apple-ios..."
  RUSTFLAGS="$(ios_rustflags x86_64-apple-ios13.0-simulator)" \
    "${CARGO_CMD[@]}" build -p "$CRATE_NAME" --lib --release --target x86_64-apple-ios || true
fi

LIB_DEVICE="$BUILD_DIR/aarch64-apple-ios/release/libBingleJsi.a"
if ! $SIM_ONLY; then
  cp "$BUILD_DIR/aarch64-apple-ios/release/lib${CRATE_NAME}.a" "$LIB_DEVICE"
fi

LIB_SIM_ARM64="$BUILD_DIR/aarch64-apple-ios-sim/release/libBingleJsi.a"
cp "$BUILD_DIR/aarch64-apple-ios-sim/release/lib${CRATE_NAME}.a" "$LIB_SIM_ARM64"

LIB_SIM_X64="$BUILD_DIR/x86_64-apple-ios/release/libBingleJsi.a"
if $HAS_X64_SIM && [[ -f "$BUILD_DIR/x86_64-apple-ios/release/lib${CRATE_NAME}.a" ]]; then
  cp "$BUILD_DIR/x86_64-apple-ios/release/lib${CRATE_NAME}.a" "$LIB_SIM_X64"
fi

# The bindings/headers are architecture-independent, so any built slice works for generation.
BINDGEN_LIB="$LIB_DEVICE"
$SIM_ONLY && BINDGEN_LIB="$LIB_SIM_ARM64"

REQUIRED_LIBS=("$LIB_SIM_ARM64")
$SIM_ONLY || REQUIRED_LIBS=("$LIB_DEVICE" "$LIB_SIM_ARM64")
for lib in "${REQUIRED_LIBS[@]}"; do
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
  --library "$BINDGEN_LIB" \
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

if $SIM_ONLY; then
  # Simulator-only XCFramework for fast local Detox iteration (not for commit).
  xcodebuild -create-xcframework \
    -library "$SIM_LIB" -headers "$HEADERS_DIR" \
    -output "$XCFRAMEWORK_PATH"
else
  xcodebuild -create-xcframework \
    -library "$LIB_DEVICE" -headers "$HEADERS_DIR" \
    -library "$SIM_LIB" -headers "$HEADERS_DIR" \
    -output "$XCFRAMEWORK_PATH"
fi

# ── guard: fail if any build-machine path leaked into the static libs ─
source "$(dirname "$0")/../../scripts/scan_native_leaks.sh"
scan_native_leaks "$XCFRAMEWORK_PATH"

echo ""
echo "=== iOS build complete ==="
echo "XCFramework: $XCFRAMEWORK_PATH"
echo "Swift bindings: $GENERATED_DIR/"
echo ""
echo "To use in a React Native project:"
echo "  1. Add this module as an npm dependency (see package.json)"
echo "  2. Run 'cd ios && pod install'"
echo "  3. The Swift bindings and XCFramework are linked automatically via the podspec"
