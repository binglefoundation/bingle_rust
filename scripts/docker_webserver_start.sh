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

if [[ -n "${EXTRA_ARGS:-}" ]]; then
  # shellcheck disable=SC2206
  EXTRA_ARR=( ${EXTRA_ARGS} )
  CMD+=("${EXTRA_ARR[@]}")
fi

echo "Starting webserver: ${CMD[*]}"
exec "${CMD[@]}"
