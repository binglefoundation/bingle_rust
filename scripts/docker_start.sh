#!/usr/bin/env bash
set -euo pipefail

# This script starts the rust_comms CLI with the expected arguments, taking
# configuration from environment variables.
# Required env vars:
#   PASSPHRASE  - passphrase to unlock identity
#   EXTERNAL_IP - externally reachable IP or DNS name
#   PORT        - UDP/TCP port to bind/expose
# Optional env vars:
#   EXTRA_ARGS  - any extra args to pass to the CLI
#   STUN_FILE   - path to STUN servers file (default /app/stunservers.txt)
#   NODE_FILE   - path to node configuration JSON (default /app/nodely_testnet_node.json)

: "${PASSPHRASE:?Environment variable PASSPHRASE must be set}"
: "${EXTERNAL_IP:?Environment variable EXTERNAL_IP must be set}"
: "${PORT:?Environment variable PORT must be set}"

STUN_FILE=${STUN_FILE:-/app/stunservers.txt}
NODE_FILE=${NODE_FILE:-/app/nodely_testnet_node.json}

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
