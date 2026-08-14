#!/usr/bin/env bash
set -euo pipefail
#
# Run the bingle_jsi Detox iOS end-to-end (e2e) suite from a clean checkout with one command
# (issue #109/#116). Builds the simulator xcframework, installs JS + pods, builds the app with
# Detox, opens Metro in its own Terminal window (like `react-native run-ios`, left running), and
# runs the e2e.
#
# Usage (from anywhere in the repo):
#   bash bingle_jsi/example/scripts/run_e2e_ios.sh              # full: build everything, then test
#   bash bingle_jsi/example/scripts/run_e2e_ios.sh --test-only  # fast: skip setup, just run the e2e
#
# One-time prerequisites: Xcode + command-line tools, CocoaPods, Node, Rust with iOS targets, and
# applesimutils (a newer Homebrew needs the tap trusted):
#   brew tap wix/brew && brew trust wix/brew && brew install applesimutils

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXAMPLE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
JSI_DIR="$(cd "$EXAMPLE_DIR/.." && pwd)"

TEST_ONLY=false
[[ "${1:-}" == "--test-only" ]] && TEST_ONLY=true

if ! command -v applesimutils >/dev/null 2>&1; then
  echo "Error: applesimutils not found. Install it with:" >&2
  echo "  brew tap wix/brew && brew trust wix/brew && brew install applesimutils" >&2
  exit 1
fi

cd "$EXAMPLE_DIR"

if ! $TEST_ONLY; then
  echo "==> Building the Apple-silicon simulator xcframework (arm64 slice only)"
  BINGLE_IOS_SIM_ONLY=1 bash "$JSI_DIR/scripts/build_ios.sh"

  echo "==> Installing JS dependencies"
  npm install --legacy-peer-deps

  echo "==> pod install (old architecture: bingle_jsi is a classic bridge)"
  ( cd ios && RCT_NEW_ARCH_ENABLED=0 pod install )

  echo "==> Building the app in the Detox debug configuration"
  npm run e2e:build:ios
fi

# The Debug app loads JS from Metro. Match `react-native run-ios`: run Metro in its own foreground
# Terminal window and leave it running (so you can watch bundling/logs). Reuse an instance already
# on 8081. Falls back to a background process with no GUI (e.g. CI).
wait_for_metro() {
  echo "    waiting for Metro on :8081 ..."
  for _ in $(seq 1 90); do
    lsof -iTCP:8081 -sTCP:LISTEN -n >/dev/null 2>&1 && return 0
    sleep 1
  done
  echo "Error: Metro did not come up on :8081" >&2
  exit 1
}

if lsof -iTCP:8081 -sTCP:LISTEN -n >/dev/null 2>&1; then
  echo "==> Reusing the Metro instance already listening on 8081"
elif command -v osascript >/dev/null 2>&1; then
  echo "==> Opening Metro in a new Terminal window"
  osascript >/dev/null <<OSA
tell application "Terminal"
  do script "cd \"$EXAMPLE_DIR\" && npx react-native start"
  activate
end tell
OSA
  wait_for_metro
else
  echo "==> Starting Metro in the background (no Terminal available)"
  npx react-native start >/tmp/bingle_metro.log 2>&1 &
  # shellcheck disable=SC2064
  trap "kill $! 2>/dev/null || true" EXIT
  wait_for_metro
fi

echo "==> Running the Detox e2e suite"
npm run e2e:test:ios
