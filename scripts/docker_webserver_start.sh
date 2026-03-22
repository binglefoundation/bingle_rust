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

# Global peak tracking for the current process
PEAK_BYTES=0

# Function to report peak memory usage using 'free'
report_peak_memory() {
  local type="${1:-FINAL}"
  if ! command -v free >/dev/null 2>&1; then
    echo "[docker_webserver_start][WARN] 'free' command not found; skipping memory measurement."
    return 0
  fi

  local current_bytes
  current_bytes=$(free -b | awk '/^Mem:/ {print $3}')
  
  if [[ -z "$current_bytes" ]]; then
    echo "[docker_webserver_start][WARN] Could not extract memory usage from 'free'."
    return 0
  fi

  local is_new_peak=0
  if (( current_bytes > PEAK_BYTES )); then
    PEAK_BYTES=$current_bytes
    is_new_peak=1
  fi

  if [[ "$type" == "FINAL" ]] || [[ "$is_new_peak" == "1" ]]; then
    local mb
    mb=$(gawk "BEGIN {printf \"%.2f\", $PEAK_BYTES / 1024 / 1024}" 2>/dev/null || awk "BEGIN {printf \"%.2f\", $PEAK_BYTES / 1024 / 1024}" 2>/dev/null || echo "unknown")
    local msg="[docker_webserver_start] Peak memory usage: ${mb} MB (${PEAK_BYTES} bytes) [from free] [${type}]"
    echo "$msg"
    if [[ -n "${OUT_FILE:-}" ]]; then
      # Ensure parent directory exists for OUT_FILE
      mkdir -p "$(dirname "$OUT_FILE")"
      echo "$msg" >> "$OUT_FILE"
    fi
  fi
}

if [[ -n "${EXTRA_ARGS:-}" ]]; then
  # shellcheck disable=SC2206
  EXTRA_ARR=( ${EXTRA_ARGS} )
  CMD+=("${EXTRA_ARR[@]}")
fi

echo "Starting webserver: ${CMD[*]}"

if [[ "${MEASURE_MEMORY:-0}" == "1" ]]; then
  # Start a periodic background reporter to ensure we get a peak value 
  # even if the final report in the trap is cut short.
  (
    while true; do
      sleep 10
      report_peak_memory "PERIODIC"
    done
  ) &
  REPORTER_PID=$!

  "${CMD[@]}" &
  CHILD_PID=$!
  trap 'echo "[docker_webserver_start] Caught signal, stopping child $CHILD_PID..."; kill -TERM $CHILD_PID 2>/dev/null; wait $CHILD_PID || true; kill $REPORTER_PID 2>/dev/null || true; report_peak_memory "FINAL"; exit 0' SIGTERM SIGINT
  wait $CHILD_PID || true
  kill "$REPORTER_PID" 2>/dev/null || true
  report_peak_memory "FINAL"
else
  exec "${CMD[@]}"
fi
