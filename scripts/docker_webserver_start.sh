#!/usr/bin/env bash
set -euo pipefail

# This script starts the bingle_webserver with the expected arguments, taking
# configuration from environment variables.
# Required env vars:
#   PASSPHRASE  - passphrase to unlock identity
#   HANDLE      - User handle
# Optional env vars:
#   PORT        - Webserver listen port (default 12121)
#   ADDRESS     - Webserver listen address (default 0.0.0.0)
#   EXTRA_ARGS  - any extra args to pass to the CLI
#   STUN_FILE   - path to STUN servers file (default /app/stunservers.txt)
#   NODE_FILE   - path to node configuration JSON (default /app/nodely_testnet_node.json)

: "${PASSPHRASE:?Environment variable PASSPHRASE must be set}"
: "${HANDLE:?Environment variable HANDLE must be set}"

PORT=${PORT:-12121}
ADDRESS=${ADDRESS:-0.0.0.0}
STUN_FILE=${STUN_FILE:-/app/stunservers.txt}
NODE_FILE=${NODE_FILE:-/app/nodely_testnet_node.json}

CMD=("/app/bingle_webserver" \
  "--port" "$PORT" \
  "--address" "$ADDRESS" \
  "--handle" "$HANDLE" \
  "--passphrase" "$PASSPHRASE" \
  "--stun-servers-file" "$STUN_FILE" \
  "--node-file" "$NODE_FILE"
)

# Function to report peak memory usage from cgroups
report_peak_memory() {
  local peak_file=""
  if [[ -f "/sys/fs/cgroup/memory.peak" ]]; then
    peak_file="/sys/fs/cgroup/memory.peak"
  elif [[ -f "/sys/fs/cgroup/memory/memory.max_usage_in_bytes" ]]; then
    peak_file="/sys/fs/cgroup/memory/memory.max_usage_in_bytes"
  fi

  if [[ -n "$peak_file" ]]; then
    local bytes
    bytes=$(cat "$peak_file")
    local mb
    mb=$(awk "BEGIN {printf \"%.2f\", $bytes / 1024 / 1024}" 2>/dev/null || echo "unknown")
    echo "[docker_webserver_start] Peak memory usage: ${mb} MB (${bytes} bytes)"
  fi
}

if [[ -n "${EXTRA_ARGS:-}" ]]; then
  # shellcheck disable=SC2206
  EXTRA_ARR=( ${EXTRA_ARGS} )
  CMD+=("${EXTRA_ARR[@]}")
fi

echo "Starting webserver: ${CMD[*]}"

if [[ "${MEASURE_MEMORY:-0}" == "1" ]]; then
  "${CMD[@]}" &
  CHILD_PID=$!
  trap 'kill -TERM $CHILD_PID 2>/dev/null' SIGTERM SIGINT
  wait $CHILD_PID || true
  report_peak_memory
else
  exec "${CMD[@]}"
fi
