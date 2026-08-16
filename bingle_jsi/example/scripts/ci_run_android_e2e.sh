#!/usr/bin/env bash
set -euo pipefail
#
# Run the Detox Android e2e inside the CI emulator (issue #132). Invoked by
# reactivecircus/android-emulator-runner's `script:` with the emulator already booted and adb on
# PATH. Distinct from run_e2e_android.sh (local) because CI does not manage the emulator lifecycle
# or the native/app build (those are earlier workflow steps); this just hardens the emulator, stages
# the testnet backend, runs Metro headless, and runs Detox.

HERE="$(cd "$(dirname "$0")" && pwd)"
EXAMPLE_DIR="$(cd "$HERE/.." && pwd)"
JSI_DIR="$(cd "$EXAMPLE_DIR/.." && pwd)"
ROOT_DIR="$(cd "$JSI_DIR/.." && pwd)"

# The emulator is already booted by the runner action. Harden it so a SystemUI ANR or the lock
# screen cannot steal window focus from Espresso (a failure mode seen locally under load).
adb wait-for-device
adb shell settings put global hide_error_dialogs 1 || true
adb shell settings put secure lockscreen.disabled 1 || true
adb shell input keyevent 82 || true

# testnet backend: stage the node-file + STUN list the tests read on the runner host. The messaging/
# failure suites additionally need BINGLE_E2E_PASSPHRASE/HANDLE (from CI secrets); without them they
# skip cleanly and only the smoke suite runs.
export BINGLE_E2E_BACKEND=testnet
export BINGLE_E2E_NODE_FILE=/tmp/bingle_e2e_node.json
export BINGLE_E2E_STUN_FILE=/tmp/bingle_e2e_stun.txt
export BINGLE_E2E_ECHO_TO="${BINGLE_E2E_ECHO_TO:-echo-testnet-1}"
cp "$ROOT_DIR/nodely_staging_testnet_node.json" "$BINGLE_E2E_NODE_FILE"
cp "$ROOT_DIR/stunservers.txt" "$BINGLE_E2E_STUN_FILE"

cd "$EXAMPLE_DIR"

# Metro headless + pre-warm the Android bundle (a cold first bundle can exceed Detox's RN-context
# wait).
npx react-native start >/tmp/bingle_metro.log 2>&1 &
echo "    waiting for Metro on :8081 ..."
for _ in $(seq 1 120); do
  curl -sf http://localhost:8081/status >/dev/null 2>&1 && break
  sleep 1
done
curl -s -o /dev/null "http://localhost:8081/index.bundle?platform=android&dev=true&minify=false" || true

# Detox reuses the already-running emulator (BINGLE_E2E_AVD matches the runner's avd-name).
npx detox test --configuration android.emu.debug --headless --loglevel info
