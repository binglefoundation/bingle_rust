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

: "${PASSPHRASE:?Environment variable PASSPHRASE must be set}"
: "${EXTERNAL_IP:?Environment variable EXTERNAL_IP must be set}"
: "${PORT:?Environment variable PORT must be set}"

STUN_FILE=${STUN_FILE:-/app/stunservers.txt}

if [[ ! -s "$STUN_FILE" ]]; then
  echo "STUN servers file not found or empty: $STUN_FILE" >&2
  exit 2
fi

CMD=("/app/cli" \
  "--passphrase" "$PASSPHRASE" \
  "--relay" \
  "--static-ip" "${EXTERNAL_IP}:${PORT}" \
  "--stun-servers-file" "$STUN_FILE"
)

if [[ -n "${EXTRA_ARGS:-}" ]]; then
  # shellcheck disable=SC2206
  EXTRA_ARR=( ${EXTRA_ARGS} )
  CMD+=("${EXTRA_ARR[@]}")
fi

echo "Starting: ${CMD[*]}"
exec "${CMD[@]}"
