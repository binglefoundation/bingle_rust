#!/usr/bin/env bash
# docker_run_test.sh
# Purpose: Run a prebuilt Rust test binary that contains the testnet integration test
#          testnet_user_reaches_endpoint_available, write results to a host file,
#          and exit with the test's exit code.
#
# Expected environment variables (can be overridden at runtime):
#   TEST_BIN_PATH (optional): Path to the test binary inside the container (default /app/test_bin)
#   OUT_FILE      (optional): Output file path for combined stdout+stderr (default /out/test_results.txt)
#   TEST_FILTER   (optional): libtest filter string (default: testnet_user_reaches_endpoint_available)
#   TESTNET_USER, TESTNET_PASSPHRASE: Credentials required by the integration test
#   NODE_FILE     (optional): Path to node file; default /app/nodely_staging_testnet_node.json
#   NAT_MODE      (optional): Direct|Full|Restricted|All (default: All if not set by outer script)
#   EXPECT_FINAL_STATE (optional): Overrides expected final engine state for single-run mode
#
# Notes:
# - The container image is built to include /app/nodely_staging_testnet_node.json and /app/stunservers.txt.
# - Ensure the host mounts a directory at /out to collect OUT_FILE.
#
set -euo pipefail

TEST_BIN=${TEST_BIN_PATH:-/app/test_bin}
OUT_FILE=${OUT_FILE:-/out/test_results.txt}
FILTER=${TEST_FILTER:-testnet_user_reaches_endpoint_available}
NODE_FILE=${NODE_FILE:-/app/nodely_staging_testnet_node.json}
NAT_MODE=${NAT_MODE:-All}

# Configure NAT/iptables if requested via NAT_MODE
configure_nat() {
  local mode="$1"
  echo "[runner][NAT] NAT_MODE=${mode}"
  if [[ "${mode}" == "Direct" ]]; then
    echo "[runner][NAT] Direct mode: no iptables changes"
    return 0
  fi

  if ! command -v iptables >/dev/null 2>&1; then
    echo "[runner][NAT][WARN] iptables not found; cannot configure NAT/firewall. Proceeding without." >&2
    return 0
  fi

  # Try to detect the primary interface (fallback to eth0)
  local IFACE
  IFACE=$(ip route show default 2>/dev/null | awk '/default/ {for(i=1;i<=NF;i++) if($i=="dev") {print $(i+1); exit}}')
  IFACE=${IFACE:-eth0}
  echo "[runner][NAT] Using interface ${IFACE}"

  # Flush existing rules to have a clean slate for tests
  iptables -F || true
  iptables -t nat -F || true
  iptables -t raw -F || true
  iptables -t mangle -F || true

  # Always allow loopback and established traffic
  iptables -P INPUT DROP || true
  iptables -P FORWARD DROP || true
  iptables -P OUTPUT ACCEPT || true
  iptables -A INPUT -i lo -j ACCEPT
  iptables -A INPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT

  case "${mode}" in
    Full)
      echo "[runner][NAT] Configuring Full NAT (full cone approximation)"
      # Enable basic SNAT (MASQUERADE) for all egress on IFACE
      iptables -t nat -A POSTROUTING -o "$IFACE" -j MASQUERADE
      # Accept inbound UDP on high ports (approximate full-cone behavior for tests)
      iptables -A INPUT -i "$IFACE" -p udp --dport 1024:65535 -j ACCEPT
      # Accept inbound TCP on high ports if needed by tests
      iptables -A INPUT -i "$IFACE" -p tcp --dport 1024:65535 -j ACCEPT
      ;;
    Restricted)
      echo "[runner][NAT] Configuring Restricted (restricted cone approximation)"
      # SNAT for egress
      iptables -t nat -A POSTROUTING -o "$IFACE" -j MASQUERADE
      # Track remote IPs we send to (UDP and TCP) using the 'recent' module
      iptables -A OUTPUT -o "$IFACE" -p udp -m state --state NEW -m recent --name bingle_peers --set
      iptables -A OUTPUT -o "$IFACE" -p tcp -m state --state NEW -m recent --name bingle_peers --set
      # Allow inbound UDP only from IPs we've recently contacted
      iptables -A INPUT -i "$IFACE" -p udp -m recent --name bingle_peers --rcheck -j ACCEPT
      # Allow inbound TCP only from IPs we've recently contacted (if tests use TCP)
      iptables -A INPUT -i "$IFACE" -p tcp -m recent --name bingle_peers --rcheck -j ACCEPT
      ;;
    *)
      echo "[runner][NAT][WARN] Unknown NAT_MODE='${mode}'; using Direct (no changes)" >&2
      ;;
  esac
}

# Derive EXPECT_FINAL_STATE from a given mode
expected_for_mode() {
  case "$1" in
    Restricted) echo "NATRestricted" ;;
    Direct|Full) echo "EndpointAvailable" ;;
    *) echo "EndpointAvailable" ;;
  esac
}

# Global peak tracking for the current process
PEAK_BYTES=0

# Function to report peak memory usage using 'free'
report_peak_memory() {
  local type="${1:-FINAL}"
  if ! command -v free >/dev/null 2>&1; then
    echo "[runner][WARN] 'free' command not found; skipping memory measurement." | tee -a "$OUT_FILE"
    return 0
  fi

  local current_bytes
  current_bytes=$(free -b | awk '/^Mem:/ {print $3}')
  
  if [[ -z "$current_bytes" ]]; then
    echo "[runner][WARN] Could not extract memory usage from 'free'." | tee -a "$OUT_FILE"
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
    local msg="[runner] Peak memory usage (container life): ${mb} MB (${PEAK_BYTES} bytes) [from free] [${type}]"
    echo "$msg"
    if [[ -n "${OUT_FILE:-}" ]]; then
      # Ensure parent directory exists for OUT_FILE
      mkdir -p "$(dirname "$OUT_FILE")"
      echo "$msg" >> "$OUT_FILE"
    fi
  fi
}

# Run the test once for a given mode; returns exit code
run_one_mode() {
  local mode="$1"
  local expect
  expect=$(expected_for_mode "$mode")
  export NAT_MODE="$mode"
  export EXPECT_FINAL_STATE="$expect"

  echo "[runner]==================================================" | tee -a "$OUT_FILE"
  echo "[runner] Running mode: $mode (EXPECT_FINAL_STATE=$expect)" | tee -a "$OUT_FILE"

  configure_nat "$mode"

  # Execute the test; pass the filter as a positional arg followed by --nocapture to show logs
  set +e
  echo "[runner][$mode] START TEST: $FILTER" | tee -a "$OUT_FILE"
  stdbuf -oL -eL "$TEST_BIN" "$FILTER" --nocapture 2>&1 | tee -a "$OUT_FILE"
  local rc=${PIPESTATUS[0]}
  set -e
  echo "[runner][$mode] END TEST: $FILTER (rc=$rc)" | tee -a "$OUT_FILE"

  if [[ $rc -eq 0 ]]; then
    echo "[runner][$mode] Test PASSED" | tee -a "$OUT_FILE"
  else
    echo "[runner][$mode] Test FAILED with exit code $rc" | tee -a "$OUT_FILE"
  fi
  return $rc
}

# Basic validation
if [[ ! -x "$TEST_BIN" ]]; then
  echo "[runner][ERROR] Test binary not found or not executable at: $TEST_BIN" >&2
  exit 2
fi
if [[ ! -f "$NODE_FILE" ]]; then
  echo "[runner][WARN] Node file not found at $NODE_FILE; the test may set its own path." >&2
fi
mkdir -p "$(dirname "$OUT_FILE")"

# Required env for running the test against testnet
if [[ -z "${TESTNET_USER:-}" || -z "${TESTNET_PASSPHRASE:-}" ]]; then
  echo "[runner][ERROR] TESTNET_USER and TESTNET_PASSPHRASE must be provided to run the test." | tee "$OUT_FILE" >&2
  exit 3
fi

# Periodic memory reporting in case of hard kill
REPORTER_PID=""
if [[ "${MEASURE_MEMORY:-0}" == "1" ]]; then
  (
    while true; do
      sleep 10
      report_peak_memory "PERIODIC"
    done
  ) &
  REPORTER_PID=$!
  
  # Ensure we report memory even if the script is interrupted
  trap 'echo "[runner] Caught signal, stopping..."; if [[ -n "$REPORTER_PID" ]]; then kill "$REPORTER_PID" 2>/dev/null || true; fi; report_peak_memory "FINAL"; exit 1' SIGTERM SIGINT
fi

# Force-enable the testnet integration test execution
export BINGLE_RUN_TESTNET=1
export RUST_BACKTRACE=1

# Print basic context (truncate OUT_FILE)
{
  echo "[runner] Starting test binary: $TEST_BIN"
  echo "[runner] Filter: $FILTER"
  echo "[runner] Writing results to: $OUT_FILE"
  echo "[runner] Date: $(date -Iseconds)"
  echo "[runner] NAT_MODE requested: ${NAT_MODE}"
} | tee "$OUT_FILE"

# Multi-mode default when NAT_MODE is All (or case-insensitive all)
if [[ "$NAT_MODE" == "All" || "$NAT_MODE" == "all" ]]; then
  modes=(Direct Full Restricted)
  declare -A results
  overall_rc=0
  for m in "${modes[@]}"; do
    if run_one_mode "$m"; then
      results[$m]=0
    else
      results[$m]=1
      overall_rc=1
    fi
  done

  echo "[runner]==================================================" | tee -a "$OUT_FILE"
  echo "[runner] Summary:" | tee -a "$OUT_FILE"
  for m in "${modes[@]}"; do
    if [[ ${results[$m]} -eq 0 ]]; then
      echo "[runner][summary] $m: PASSED" | tee -a "$OUT_FILE"
    else
      echo "[runner][summary] $m: FAILED" | tee -a "$OUT_FILE"
    fi
  done
  if [[ -n "$REPORTER_PID" ]]; then
    kill "$REPORTER_PID" 2>/dev/null || true
  fi
  report_peak_memory "FINAL"
  exit $overall_rc
fi

# Single-mode execution when NAT_MODE is one of Direct|Full|Restricted
# If EXPECT_FINAL_STATE not set, derive it from NAT_MODE
if [[ -z "${EXPECT_FINAL_STATE:-}" ]]; then
  export EXPECT_FINAL_STATE="$(expected_for_mode "$NAT_MODE")"
fi

# Run once in the requested mode
if run_one_mode "$NAT_MODE"; then
  if [[ -n "$REPORTER_PID" ]]; then
    kill "$REPORTER_PID" 2>/dev/null || true
  fi
  report_peak_memory "FINAL"
  exit 0
else
  if [[ -n "$REPORTER_PID" ]]; then
    kill "$REPORTER_PID" 2>/dev/null || true
  fi
  report_peak_memory "FINAL"
  exit 1
fi
