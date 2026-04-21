#!/usr/bin/env bash
set -euo pipefail
#
# Build and run the BingleJsiExample app on the iOS Simulator.
#
# This script assumes setup_example.sh has already been run.
# It optionally rebuilds the native libraries if --rebuild is passed.
#
# Usage:
#   bash bingle_jsi/example/scripts/run_ios.sh [--rebuild]

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXAMPLE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
JSI_DIR="$(cd "$EXAMPLE_DIR/.." && pwd)"

REBUILD=false
for arg in "$@"; do
  case "$arg" in
    --rebuild) REBUILD=true ;;
    *) echo "Unknown argument: $arg"; exit 1 ;;
  esac
done

# Optionally rebuild native libraries
if $REBUILD; then
  echo "Rebuilding bingle_jsi iOS native libraries..."
  bash "$JSI_DIR/scripts/build_ios.sh"
  echo ""
  echo "Re-installing pods..."
  cd "$EXAMPLE_DIR/ios"
  pod install
  cd "$EXAMPLE_DIR"
  echo ""
fi

# Verify setup
if [[ ! -d "$EXAMPLE_DIR/node_modules" ]]; then
  echo "Error: node_modules not found. Run setup_example.sh first." >&2
  exit 1
fi

if [[ ! -d "$EXAMPLE_DIR/ios/Pods" ]]; then
  echo "Error: ios/Pods not found. Run setup_example.sh first." >&2
  exit 1
fi

# Clean stale DerivedData build database locks that cause
# "unable to attach DB: database is locked" errors.
DERIVED_DATA_DIR="$HOME/Library/Developer/Xcode/DerivedData"
for dd in "$DERIVED_DATA_DIR"/BingleJsiExample-*/Build/Intermediates.noindex/XCBuildData/build.db; do
  if [[ -f "$dd" ]]; then
    echo "Removing stale build database: $dd"
    rm -f "$dd"
  fi
done

# Run the app
echo "Starting BingleJsiExample on iOS Simulator..."
cd "$EXAMPLE_DIR"
npx react-native run-ios
