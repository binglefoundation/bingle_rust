#!/usr/bin/env bash
set -euo pipefail
#
# Run the bingle_jsi Detox Android end-to-end (e2e) suite on an emulator (issue #130). Android
# counterpart of run_e2e_ios.sh: builds the native lib + example app, builds the Detox app +
# androidTest APKs, runs Metro (foreground Terminal window; background under CI), and runs the suite.
# Detox boots the AVD named in .detoxrc.js and adb-reverses the app's ports (Metro 8081 + algod 4001
# / indexer 8980) so the emulator reaches host services.
#
# Usage (from anywhere in the repo):
#   bash bingle_jsi/example/scripts/run_e2e_android.sh              # full: build everything, then test
#   bash bingle_jsi/example/scripts/run_e2e_android.sh --test-only  # fast: skip setup, just run the e2e
#
# Backend (BINGLE_E2E_BACKEND), same selector as iOS:
#   testnet (default): the smoke suite needs nothing; the messaging/failure suites need a funded,
#     already-registered sender, supplied via the environment, e.g.:
#       BINGLE_E2E_PASSPHRASE="word word … word" BINGLE_E2E_HANDLE=my-handle \
#         bash bingle_jsi/example/scripts/run_e2e_android.sh
#   localnet: the smoke suite runs against a locally-running `algokit localnet` (its ports are
#     adb-reversed to the emulator). The self-provisioned messaging/failure suites on localnet are
#     wired in issue #131.
#
# Prerequisites: Android SDK + NDK, a JDK 17, an AVD matching .detoxrc.js (`emulator -list-avds`),
# Node, Rust with Android targets. Set JAVA_HOME to a JDK 17 (RN 0.84 requires 17) or the script
# auto-detects one via /usr/libexec/java_home -v 17 on macOS.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXAMPLE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
JSI_DIR="$(cd "$EXAMPLE_DIR/.." && pwd)"
ROOT_DIR="$(cd "$JSI_DIR/.." && pwd)"

TEST_ONLY=false
[[ "${1:-}" == "--test-only" ]] && TEST_ONLY=true

# RN 0.84 needs a JDK 17. Honor an existing JAVA_HOME; otherwise try to locate one (macOS).
if [[ -z "${JAVA_HOME:-}" ]] && command -v /usr/libexec/java_home >/dev/null 2>&1; then
  JAVA_HOME="$(/usr/libexec/java_home -v 17 2>/dev/null || true)"
  export JAVA_HOME
fi
: "${ANDROID_HOME:=$HOME/Library/Android/sdk}"
export ANDROID_HOME
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/27.1.12297006}"

if ! command -v adb >/dev/null 2>&1 && [[ -x "$ANDROID_HOME/platform-tools/adb" ]]; then
  export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
fi

# Backend selector: stage the node-file + STUN list the network e2e init()s with, to a path the
# emulator can read. (The messaging/failure suites skip cleanly when creds are unset; smoke ignores.)
export BINGLE_E2E_BACKEND="${BINGLE_E2E_BACKEND:-testnet}"
export BINGLE_E2E_NODE_FILE=/tmp/bingle_e2e_node.json
export BINGLE_E2E_STUN_FILE=/tmp/bingle_e2e_stun.txt
case "$BINGLE_E2E_BACKEND" in
  testnet)
    cp "$ROOT_DIR/nodely_staging_testnet_node.json" "$BINGLE_E2E_NODE_FILE"
    cp "$ROOT_DIR/stunservers.txt" "$BINGLE_E2E_STUN_FILE"
    export BINGLE_E2E_ECHO_TO="${BINGLE_E2E_ECHO_TO:-echo-testnet-1}"
    rm -f /tmp/bingle_e2e_messaging_state.json /tmp/bingle_e2e_failure_state.json
    echo "==> Backend: testnet (echo -> $BINGLE_E2E_ECHO_TO)"
    ;;
  localnet)
    echo "Error: BINGLE_E2E_BACKEND=localnet on Android is wired in issue #131 (needs 10.0.2.2" >&2
    echo "       emulator<->host networking); use testnet for the #130 smoke run." >&2
    exit 1
    ;;
  *)
    echo "Error: unknown BINGLE_E2E_BACKEND='$BINGLE_E2E_BACKEND' (expected testnet|localnet)" >&2
    exit 1
    ;;
esac

cd "$EXAMPLE_DIR"

if ! $TEST_ONLY; then
  echo "==> Building the Android native library (jniLibs + Kotlin bindings)"
  bash "$JSI_DIR/scripts/build_android.sh"

  echo "==> Installing JS dependencies"
  npm install --legacy-peer-deps

  echo "==> Building the app + androidTest APKs (Detox debug configuration)"
  npm run e2e:build:android
fi

# The Debug app loads JS from Metro; Detox adb-reverses port 8081 (reversePorts in .detoxrc.js) so
# the emulator reaches it. Match run_e2e_ios.sh: run Metro in its own foreground Terminal window and
# leave it running (so you can watch bundling/logs). Reuse an instance already on 8081. Under CI (or
# with no Terminal available) fall back to a headless background process.
METRO_BG_PID=""
cleanup() { [[ -n "$METRO_BG_PID" ]] && kill "$METRO_BG_PID" 2>/dev/null || true; }
trap cleanup EXIT

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
elif [[ -z "${CI:-}" ]] && command -v osascript >/dev/null 2>&1; then
  echo "==> Opening Metro in a new Terminal window"
  osascript >/dev/null <<OSA
tell application "Terminal"
  do script "cd \"$EXAMPLE_DIR\" && npx react-native start"
  activate
end tell
OSA
  wait_for_metro
else
  echo "==> Starting Metro in the background (CI / no Terminal)"
  npx react-native start >/tmp/bingle_metro.log 2>&1 &
  METRO_BG_PID=$!
  wait_for_metro
fi

# Pre-warm the Android JS bundle: the first cold Metro bundle can take longer than Detox's 60s
# RN-context wait, which makes the app time out before the JS context is ready. Building it once now
# (Metro caches it) means the app's fetch is fast and lands within Detox's window.
echo "==> Pre-warming the Android JS bundle"
curl -s -o /dev/null "http://localhost:8081/index.bundle?platform=android&dev=true&minify=false" || true

echo "==> Running the Detox Android e2e suite"
npm run e2e:test:android
