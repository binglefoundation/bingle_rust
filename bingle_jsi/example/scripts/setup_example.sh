#!/usr/bin/env bash
set -euo pipefail
#
# Setup the BingleJsiExample React Native app for iOS.
#
# This script:
#   1. Builds the bingle_jsi native iOS libraries (XCFramework + Swift bindings)
#   2. Runs `npm install` in the example directory
#   3. Generates the Xcode project via react-native community CLI
#   4. Runs `pod install` to link native dependencies
#
# Prerequisites:
#   - Node.js and npm
#   - Xcode command-line tools
#   - Rust toolchain with iOS targets (handled by build_ios.sh)
#   - CocoaPods (`gem install cocoapods` or `brew install cocoapods`)
#   - swiftformat (`brew install swiftformat`)
#
# Usage:
#   bash bingle_jsi/example/scripts/setup_example.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXAMPLE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
JSI_DIR="$(cd "$EXAMPLE_DIR/.." && pwd)"
ROOT_DIR="$(cd "$JSI_DIR/.." && pwd)"

echo "=== BingleJsiExample Setup ==="
echo "Project root: $ROOT_DIR"
echo "JSI module:   $JSI_DIR"
echo "Example app:  $EXAMPLE_DIR"
echo ""

# ── Step 1: Build native iOS libraries ────────────────────────────────

echo "Step 1: Building bingle_jsi iOS native libraries..."
bash "$JSI_DIR/scripts/build_ios.sh"
echo ""

# ── Step 2: npm install ──────────────────────────────────────────────

echo "Step 2: Installing npm dependencies..."
cd "$EXAMPLE_DIR"
npm install --legacy-peer-deps
echo ""

# ── Step 3: Generate Xcode project if missing ────────────────────────

XCODEPROJ="$EXAMPLE_DIR/ios/BingleJsiExample.xcodeproj"
if [[ ! -d "$XCODEPROJ" ]]; then
  echo "Step 3: Generating Xcode project..."
  # Use react-native's init to create ios/ project scaffolding
  # We use a temp project and copy the ios/ folder
  TMPDIR_INIT="$(mktemp -d)"
  cd "$TMPDIR_INIT"

  npx --yes @react-native-community/cli@12.3.6 init BingleJsiExample --version 0.73.4 --skip-install --skip-git-init 2>&1 || true

  if [[ -d "$TMPDIR_INIT/BingleJsiExample/ios" ]]; then
    # Copy generated ios project files (keep our Podfile)
    cp -R "$TMPDIR_INIT/BingleJsiExample/ios/"* "$EXAMPLE_DIR/ios/" 2>/dev/null || true
    # Keep our custom Podfile
    cp "$EXAMPLE_DIR/ios/Podfile" "$EXAMPLE_DIR/ios/Podfile"
  else
    echo "Warning: react-native init did not produce ios/ directory."
    echo "You may need to create the Xcode project manually."
  fi

  rm -rf "$TMPDIR_INIT"
  cd "$EXAMPLE_DIR"
else
  echo "Step 3: Xcode project already exists, skipping generation."
fi
echo ""

# ── Step 4: Pod install ──────────────────────────────────────────────

echo "Step 4: Installing CocoaPods dependencies..."
cd "$EXAMPLE_DIR/ios"
pod install
cd "$EXAMPLE_DIR"
echo ""

echo "=== Setup complete ==="
echo ""
echo "To run the app on iOS Simulator:"
echo "  cd bingle_jsi/example"
echo "  npx react-native run-ios"
echo ""
echo "Or use the run script:"
echo "  bash bingle_jsi/example/scripts/run_ios.sh"
