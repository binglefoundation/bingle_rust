#!/usr/bin/env bash
# Run the BingleJsiBridgeTests Swift XCTest suite on an iOS simulator.
#
# These tests exercise BingleJsiBridge.swift against a mock BingleJsiApiProtocol
# implementation. No network, no passphrase, no real Bingle engine required.
#
# Prerequisites:
#   - macOS with Xcode 15+ installed
#   - iOS 18.6 simulator runtime available (xcrun simctl list runtimes)
#   - CocoaPods pods already installed in bingle_jsi/example/ios/
#     (run `pod install` there if Pods/ is missing or Podfile.lock has changed)
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
DESTINATION="platform=iOS Simulator,name=iPhone 16,OS=18.6"
LOG_FILE="$PROJECT_ROOT/tmp/ios_bridge_tests.log"

mkdir -p "$PROJECT_ROOT/tmp"

echo "=== BingleJsiBridge Swift XCTests ==="
echo "Workspace  : $WORKSPACE"
echo "Scheme     : $SCHEME"
echo "Destination: $DESTINATION"
echo "Log        : $LOG_FILE"
echo ""

# Ensure the iPhone 16 iOS 18.6 simulator is booted before running tests.
# xcodebuild can boot it automatically, but pre-booting avoids install timeouts.
DEVICE_ID=$(xcrun simctl list devices available | awk '/-- iOS 18/{found=1} found && /iPhone 16 \(/{print; exit}' | sed 's/.*(\([A-F0-9-]*\)).*/\1/')
if [ -n "$DEVICE_ID" ]; then
    if xcrun simctl list devices | python3 -c "import sys; exit(0 if any('$DEVICE_ID' in l and 'Booted' in l for l in sys.stdin))" 2>/dev/null; then
        echo "Simulator already booted ($DEVICE_ID)"
    else
        echo "Booting simulator $DEVICE_ID..."
        xcrun simctl boot "$DEVICE_ID" 2>/dev/null || true
        sleep 3
    fi
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
