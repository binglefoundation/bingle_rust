#!/usr/bin/env bash
set -euo pipefail
#
# Run the Detox Android e2e inside the CI emulator against the in-CI localnet (issues #132/#137).
# Invoked by reactivecircus/android-emulator-runner's `script:` with the emulator already booted and
# adb on PATH. The localnet + provisioner (emulator mode) were brought up in earlier workflow steps
# on the runner host; this hardens the emulator, sources the provisioner's derived creds, runs Metro
# headless, and runs Detox. All addressing is IP-based (10.0.2.2), so no internet/DNS is needed —
# which is why localnet, not testnet, is the CI network backend (the CI emulator has no internet).

HERE="$(cd "$(dirname "$0")" && pwd)"
EXAMPLE_DIR="$(cd "$HERE/.." && pwd)"

# sys.boot_completed fires before the framework services (package/settings) are ready; using them
# too early gives "cmd: Can't find service: settings/package" and the app install fails.
adb wait-for-device
echo "    waiting for boot + framework services ..."
for _ in $(seq 1 180); do
  if [[ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == "1" ]] \
     && adb shell service check package 2>/dev/null | grep -q found \
     && adb shell service check settings 2>/dev/null | grep -q found; then
    break
  fi
  sleep 2
done

# Harden the emulator so a SystemUI ANR or the lock screen cannot steal window focus from Espresso.
adb shell settings put global hide_error_dialogs 1 || true
adb shell settings put secure lockscreen.disabled 1 || true
adb shell input keyevent 82 || true

# The window that has input focus, e.g. "Window{... u0 com.google.android.apps.nexuslauncher/...}".
focused_window() {
  { adb shell dumpsys window 2>/dev/null | tr -d '\r' | grep -m 1 'mCurrentFocus=' | sed 's/^ *mCurrentFocus=//'; } || true
}

# Clear whatever the boot snapshot restored over the launcher (issue #414): a dialog or the lock
# screen left on screen keeps the app's window from getting focus, which fails the first suite (or
# every suite) with "has-window-focus=false" or "No views in hierarchy found". The hardening above
# stops new dialogs but does not close one already showing. Then wait for the launcher to hold focus.
echo "    focus after boot: $(focused_window)"
adb shell wm dismiss-keyguard || true
adb shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS >/dev/null || true
adb shell input keyevent KEYCODE_HOME || true
for _ in $(seq 1 30); do
  focus=$(focused_window)
  [[ "$focus" == *launcher* ]] && break
  sleep 2
done
echo "    focus before tests: $focus"

# Record every change of focus while the suites run, so a failure names the window that held it.
ARTIFACTS_DIR="$EXAMPLE_DIR/artifacts"
mkdir -p "$ARTIFACTS_DIR"
FOCUS_LOG="$ARTIFACTS_DIR/window_focus.log"
(
  last=""
  while true; do
    now=$(focused_window)
    if [[ -n "$now" && "$now" != "$last" ]]; then
      echo "$(date -u +%H:%M:%S) $now"
      last=$now
    fi
    sleep 2
  done
) >"$FOCUS_LOG" 2>/dev/null &
focus_watch=$!
trap 'kill "$focus_watch" 2>/dev/null || true' EXIT

# localnet backend: the provisioner (started in a prior workflow step) wrote the node-file/STUN list
# (advertising 10.0.2.2) and an env file with the derived creds. The app reaches host services via
# the emulator's qemu gateway 10.0.2.2; the smoke suite's default localnet config is adb-reversed.
export BINGLE_E2E_BACKEND=localnet
export BINGLE_E2E_NODE_FILE=/tmp/bingle_e2e_node.json
export BINGLE_E2E_STUN_FILE=/tmp/bingle_e2e_stun.txt
if [[ ! -f /tmp/bingle_e2e_localnet.env ]]; then
  echo "Error: /tmp/bingle_e2e_localnet.env missing — the provisioner step did not complete." >&2
  exit 1
fi
# shellcheck disable=SC1090
source /tmp/bingle_e2e_localnet.env
echo "localnet: sender='$BINGLE_E2E_HANDLE' echo -> $BINGLE_E2E_ECHO_TO offline='${BINGLE_E2E_OFFLINE_HANDLE:-}' store_forward='${BINGLE_E2E_STORE_FORWARD:-off}'"

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

# Detox reuses the already-running emulator (BINGLE_E2E_AVD matches the runner's avd-name). A failing
# test keeps a screenshot and the device log under artifacts/, which the workflow uploads.
status=0
npx detox test --configuration android.emu.debug --headless --loglevel info \
  --artifacts-location "$ARTIFACTS_DIR/" --take-screenshots failing --record-logs failing || status=$?
if [[ $status -ne 0 ]]; then
  echo "window focus during the run (issue #414):"
  cat "$FOCUS_LOG"
fi
exit "$status"
