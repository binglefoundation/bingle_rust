#!/usr/bin/env bash
# Run the BingleJsiBridgeTests Swift XCTest suite on an iOS simulator.
#
# These tests exercise BingleJsiBridge.swift against a mock BingleJsiApiProtocol
# implementation. No network, no passphrase, no real Bingle engine required.
#
# Local only: iOS tests do not run in CI.
#
# Prerequisites:
#   - macOS with Xcode installed and an iPhone simulator available (xcrun simctl list devices)
#   - The simulator framework and Swift bindings built for the current Rust code:
#       BINGLE_IOS_SIM_ONLY=1 bash bingle_jsi/scripts/build_ios.sh
#   - CocoaPods pods installed in bingle_jsi/example/ios/
#     (run `pod install` there if Pods/ is missing or Podfile.lock has changed)
#
# The simulator is chosen by UDID: BINGLE_IOS_TEST_DEVICE if set, else an already booted iPhone,
# else the first available iPhone.
#
# Usage:
#   ./bingle_jsi/scripts/run_ios_bridge_tests.sh          # from project root
#   cd bingle_jsi/scripts && ./run_ios_bridge_tests.sh    # from this directory

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
IOS_DIR="$PROJECT_ROOT/bingle_jsi/example/ios"
WORKSPACE="$IOS_DIR/BingleJsiExample.xcworkspace"
SCHEME="BingleJsiBridgeTests"
LOG_FILE="$PROJECT_ROOT/tmp/ios_bridge_tests.log"

# Pick a simulator: an explicit UDID, else a booted iPhone, else the first available iPhone.
DEVICE_ID="${BINGLE_IOS_TEST_DEVICE:-}"
if [ -z "$DEVICE_ID" ]; then
    DEVICE_ID=$(xcrun simctl list devices available | grep -E '^ +iPhone.*\(Booted\)' | head -1 | sed -E 's/.*\(([A-F0-9-]{36})\).*/\1/')
fi
if [ -z "$DEVICE_ID" ]; then
    DEVICE_ID=$(xcrun simctl list devices available | grep -E '^ +iPhone' | head -1 | sed -E 's/.*\(([A-F0-9-]{36})\).*/\1/')
fi
if [ -z "$DEVICE_ID" ]; then
    echo "No iPhone simulator available (see: xcrun simctl list devices available)" >&2
    exit 1
fi
DESTINATION="platform=iOS Simulator,id=$DEVICE_ID"

mkdir -p "$PROJECT_ROOT/tmp"

echo "=== BingleJsiBridge Swift XCTests ==="
echo "Workspace  : $WORKSPACE"
echo "Scheme     : $SCHEME"
echo "Destination: $DESTINATION"
echo "Log        : $LOG_FILE"
echo ""

# Boot the simulator first: xcodebuild can boot it, but pre-booting avoids install timeouts.
if xcrun simctl list devices | grep "$DEVICE_ID" | grep -q Booted; then
    echo "Simulator already booted ($DEVICE_ID)"
else
    echo "Booting simulator $DEVICE_ID..."
    xcrun simctl boot "$DEVICE_ID" 2>/dev/null || true
    sleep 3
fi

echo ""
echo "Running tests (output in $LOG_FILE)..."
echo ""

xcodebuild test \
  -workspace "$WORKSPACE" \
  -scheme "$SCHEME" \
  -destination "$DESTINATION" \
  -sdk iphonesimulator \
  2>&1 | tee "$LOG_FILE"

# Parse the log for a summary
python3 -c "
import sys
with open('$LOG_FILE') as f:
    content = f.read()
# Print test case result lines
for line in content.splitlines():
    for k in ['Test Suite', 'Test Case', 'All tests', 'SUCCEEDED', 'FAILED']:
        if k in line and 'IDETest' not in line:
            print(line[:300])
            break
"

if python3 -c "
import sys
with open('$LOG_FILE') as f:
    content = f.read()
if '** TEST SUCCEEDED **' in content:
    print('')
    print('=== ALL TESTS PASSED ===')
    sys.exit(0)
else:
    print('')
    print('=== TESTS FAILED OR DID NOT RUN ===')
    sys.exit(1)
" 2>&1; then
    exit 0
else
    exit 1
fi
