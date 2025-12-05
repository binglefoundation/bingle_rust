#!/usr/bin/env bash
set -euo pipefail

# This script starts the rust_comms CLI with the expected arguments, taking
# configuration from environment variables.
# Required env vars:
#   PASSPHRASE  - passphrase to unlock identity
#   PORT        - UDP/TCP port to bind/expose
#   HANDLE      - User handle
# Optional env vars:
#   EXTERNAL_IP - externally reachable IP or DNS name; if blank, auto-detected
#   EXTRA_ARGS  - any extra args to pass to the CLI
#   STUN_FILE   - path to STUN servers file (default /app/stunservers.txt)
#   NODE_FILE   - path to node configuration JSON (default /app/nodely_testnet_node.json)

: "${PASSPHRASE:?Environment variable PASSPHRASE must be set}"
: "${PORT:?Environment variable PORT must be set}"
: "${HANDLE:?Environment variable HANDLE must be set}"

STUN_FILE=${STUN_FILE:-/app/stunservers.txt}
NODE_FILE=${NODE_FILE:-/app/nodely_testnet_node.json}

# Discover external IP if not provided or blank
if [[ -z "${EXTERNAL_IP:-}" ]]; then
  echo "EXTERNAL_IP not provided; attempting autodiscovery..."
  # Preferred: use routing lookup to a public IP to find the egress interface IP
  if command -v ip >/dev/null 2>&1; then
    EXTERNAL_IP=$(ip -o -4 route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="src"){print $(i+1); exit}}') || true
    if [[ -z "${EXTERNAL_IP}" ]]; then
      # Fallback: first global IPv4 address
      EXTERNAL_IP=$(ip -o -4 addr show scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | head -n1) || true
    fi
  fi
  # Fallback to ifconfig if available
  if [[ -z "${EXTERNAL_IP:-}" ]] && command -v ifconfig >/dev/null 2>&1; then
    EXTERNAL_IP=$(ifconfig 2>/dev/null | awk '/inet / && $2 !~ /127\.0\.0\.1/ {print $2; exit} /inet addr:/ {gsub("addr:","",$2); if($2!="127.0.0.1"){print $2; exit}}') || true
  fi
  # Fallback to hostname -I
  if [[ -z "${EXTERNAL_IP:-}" ]] && command -v hostname >/dev/null 2>&1; then
    EXTERNAL_IP=$(hostname -I 2>/dev/null | awk '{print $1}') || true
  fi
  if [[ -z "${EXTERNAL_IP:-}" ]]; then
    echo "Failed to autodetect EXTERNAL_IP. Please set EXTERNAL_IP explicitly." >&2
    exit 2
  fi
  echo "Autodetected EXTERNAL_IP=${EXTERNAL_IP}"
fi

if [[ ! -s "$STUN_FILE" ]]; then
  echo "STUN servers file not found or empty: $STUN_FILE" >&2
  exit 2
fi

if [[ ! -s "$NODE_FILE" ]]; then
  echo "Node configuration file not found or empty: $NODE_FILE" >&2
  exit 2
fi

CMD=("/app/bingle_cli" \
  "run" \
  "--handle" "$HANDLE" \
  "--passphrase" "$PASSPHRASE" \
  "--relay" \
  "--static-ip" "${EXTERNAL_IP}:${PORT}" \
  "--stun-servers-file" "$STUN_FILE" \
  "--node-file" "$NODE_FILE"
)

if [[ -n "${EXTRA_ARGS:-}" ]]; then
  # shellcheck disable=SC2206
  EXTRA_ARR=( ${EXTRA_ARGS} )
  CMD+=("${EXTRA_ARR[@]}")
fi

echo "Starting: ${CMD[*]}"
exec "${CMD[@]}"
