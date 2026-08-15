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
# Backend for the network e2e (issue #111), via environment:
#   BINGLE_E2E_BACKEND=testnet|localnet   default testnet
#   BINGLE_E2E_PASSPHRASE=<mnemonic>      funded, already-registered sender account (testnet only)
#   BINGLE_E2E_HANDLE=<handle>            that account's registered handle (testnet only)
#   BINGLE_E2E_ECHO_TO=<handle>           a live echo peer/relay to send to (replies "Echo: ...")
#   BINGLE_E2E_OFFLINE_HANDLE=<handle>    (optional) a handle registered but offline, for the
#                                         RecipientNotAdvertised failure-cause case (#112)
# The messaging/failure tests skip cleanly when these are unset; the smoke test ignores them.
#
# localnet (issue #123) is self-provisioning: a Rust provisioner binary (bingle_test
# localnet_e2e_provisioner) stands up a deployed app + asset, two root relays, STUN servers, an
# in-process echo peer and funded/registered sender + offline-fixture accounts on the running
# `algokit localnet`, then writes the node-file, STUN list and a BINGLE_E2E_* env file this script
# sources — so no credentials need to be supplied by hand. It needs `algokit localnet` running and
# the `algokit`/`goal` CLIs on PATH.
#
# One-time prerequisites: Xcode + command-line tools, CocoaPods, Node, Rust with iOS targets, and
# applesimutils (a newer Homebrew needs the tap trusted):
#   brew tap wix/brew && brew trust wix/brew && brew install applesimutils

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXAMPLE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
JSI_DIR="$(cd "$EXAMPLE_DIR/.." && pwd)"
ROOT_DIR="$(cd "$JSI_DIR/.." && pwd)"

TEST_ONLY=false
[[ "${1:-}" == "--test-only" ]] && TEST_ONLY=true

if ! command -v applesimutils >/dev/null 2>&1; then
  echo "Error: applesimutils not found. Install it with:" >&2
  echo "  brew tap wix/brew && brew trust wix/brew && brew install applesimutils" >&2
  exit 1
fi

# Background processes we own; killed on exit via a single cleanup trap so neither the localnet
# provisioner nor a headless Metro instance is left running after the suite finishes.
PROVISIONER_PID=""
METRO_BG_PID=""
cleanup() {
  [[ -n "$PROVISIONER_PID" ]] && kill "$PROVISIONER_PID" 2>/dev/null || true
  [[ -n "$METRO_BG_PID" ]] && kill "$METRO_BG_PID" 2>/dev/null || true
}
trap cleanup EXIT

# Backend selector (issue #111): stage the node-file + STUN list the network e2e will init() with,
# to a simulator-readable path (the sim can read the Mac's /tmp). Tests that need a real network
# read BINGLE_E2E_* from the environment; the smoke test ignores them.
export BINGLE_E2E_BACKEND="${BINGLE_E2E_BACKEND:-testnet}"
export BINGLE_E2E_NODE_FILE=/tmp/bingle_e2e_node.json
export BINGLE_E2E_STUN_FILE=/tmp/bingle_e2e_stun.txt
export BINGLE_E2E_ENV_FILE=/tmp/bingle_e2e_localnet.env
case "$BINGLE_E2E_BACKEND" in
  testnet)
    cp "$ROOT_DIR/nodely_staging_testnet_node.json" "$BINGLE_E2E_NODE_FILE"
    cp "$ROOT_DIR/stunservers.txt" "$BINGLE_E2E_STUN_FILE"
    # `echo-testnet-1` is the always-live testnet echo peer (counterpart of mainnet `echo-test-1`);
    # default to it so only the funded sender (BINGLE_E2E_HANDLE + BINGLE_E2E_PASSPHRASE) is needed.
    export BINGLE_E2E_ECHO_TO="${BINGLE_E2E_ECHO_TO:-echo-testnet-1}"
    # Start the network tests from clean local state so the funded account (imported from the
    # passphrase) is used, not a keypair left over from a previous run.
    rm -f /tmp/bingle_e2e_messaging_state.json /tmp/bingle_e2e_failure_state.json
    echo "==> Backend: testnet (staged node-file at $BINGLE_E2E_NODE_FILE; echo -> $BINGLE_E2E_ECHO_TO)"
    ;;
  localnet)
    if ! command -v algokit >/dev/null 2>&1; then
      echo "Error: BINGLE_E2E_BACKEND=localnet needs the algokit CLI on PATH." >&2
      exit 1
    fi
    echo "==> Backend: localnet (self-provisioning via localnet_e2e_provisioner)"
    echo "==> Ensuring algokit localnet is running"
    algokit localnet start

    echo "==> Building the localnet provisioner (bingle_test, localnet feature)"
    cargo build --manifest-path "$ROOT_DIR/Cargo.toml" -p bingle_test --features localnet \
      --bin localnet_e2e_provisioner

    # The provisioner writes the env file last, as its readiness signal; remove any stale copy so we
    # don't source a previous run's ids. Also clear the network tests' local state (as testnet does).
    rm -f "$BINGLE_E2E_ENV_FILE" /tmp/bingle_e2e_messaging_state.json /tmp/bingle_e2e_failure_state.json
    echo "==> Starting the localnet provisioner in the background"
    "$ROOT_DIR/target/debug/localnet_e2e_provisioner" >/tmp/bingle_e2e_provisioner.log 2>&1 &
    PROVISIONER_PID=$!
    ;;
  *)
    echo "Error: unknown BINGLE_E2E_BACKEND='$BINGLE_E2E_BACKEND' (expected testnet|localnet)" >&2
    exit 1
    ;;
esac

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

# For localnet, wait for the provisioner to publish credentials (node-file/STUN already staged by
# it) and source them, so the messaging + failure-cause tests have BINGLE_E2E_* set. The build above
# runs concurrently with provisioning, so by now it is usually already up.
if [[ "$BINGLE_E2E_BACKEND" == "localnet" ]]; then
  echo "==> Waiting for the localnet provisioner to become ready (env file $BINGLE_E2E_ENV_FILE)"
  for _ in $(seq 1 600); do
    [[ -f "$BINGLE_E2E_ENV_FILE" ]] && break
    if ! kill -0 "$PROVISIONER_PID" 2>/dev/null; then
      echo "Error: the localnet provisioner exited before becoming ready. Last log lines:" >&2
      tail -n 40 /tmp/bingle_e2e_provisioner.log >&2 || true
      exit 1
    fi
    sleep 1
  done
  if [[ ! -f "$BINGLE_E2E_ENV_FILE" ]]; then
    echo "Error: the localnet provisioner did not become ready within 600s." >&2
    tail -n 40 /tmp/bingle_e2e_provisioner.log >&2 || true
    exit 1
  fi
  # shellcheck disable=SC1090
  source "$BINGLE_E2E_ENV_FILE"
  echo "==> localnet ready: sender='$BINGLE_E2E_HANDLE' echo -> $BINGLE_E2E_ECHO_TO offline='$BINGLE_E2E_OFFLINE_HANDLE'"
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
  METRO_BG_PID=$!
  wait_for_metro
fi

echo "==> Running the Detox e2e suite"
npm run e2e:test:ios
