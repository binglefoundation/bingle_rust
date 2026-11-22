#!/usr/bin/env bash
# scripts/docker_gateway_start.sh
#
# Purpose: entrypoint of the `gateway` image (epic #63, issue #473), which forwards Bingle messages
# to email. The task runs the image twice, once per role, sharing the volume at $GATEWAY_DATA:
#
#   docker_gateway_start.sh logclient   bingle_cli logclient, writing each message into the Maildir
#   docker_gateway_start.sh mailer      scripts/mailer.sh, sending the Maildir through the SES API
#
# Environment:
#   GATEWAY_DATA     the shared volume (default /data): holds state.json and maildir/
#   FROM_ADDRESS     the verified SES address the emails come from (both roles)
#   logclient:
#     NODE_FILE      Algorand node file (default /app/nodely_deployed_mainnet_node.json)
#     STUN_FILE      STUN servers file (default /app/stunservers.txt)
#     SPAM_LIMIT     per-sender daily limit; empty for none
#     PASSPHRASE     the account's recovery phrase, only for the first start (with HANDLE), when
#                    the state file has no account yet; never logged
#     HANDLE         the account's handle, with PASSPHRASE
#   mailer:
#     TO_EMAIL       where the emails go
#     SES_REGION     SES region (default AWS_REGION, which ECS sets)
#
# BINGLE_CLI and MAILER override the program paths, for tests.

set -euo pipefail

die() {
  echo "docker_gateway_start: error: $*" >&2
  exit 2
}

DATA="${GATEWAY_DATA:-/data}"
BINGLE_CLI="${BINGLE_CLI:-/app/bingle_cli}"
MAILER="${MAILER:-/app/mailer.sh}"

[[ $# -eq 1 ]] || die "usage: $0 <logclient|mailer>"
[[ -n "${FROM_ADDRESS:-}" ]] || die "FROM_ADDRESS is not set"
[[ -d "$DATA" ]] || die "the data volume $DATA is not mounted"

case "$1" in
  logclient)
    args=(logclient
      --state_file "$DATA/state.json"
      --maildir "$DATA/maildir"
      --from "$FROM_ADDRESS"
      --node-file "${NODE_FILE:-/app/nodely_deployed_mainnet_node.json}"
      --stun-servers-file "${STUN_FILE:-/app/stunservers.txt}")
    if [[ -n "${SPAM_LIMIT:-}" ]]; then
      args+=(--spam-limit "$SPAM_LIMIT")
    fi
    if [[ -n "${PASSPHRASE:-}" ]]; then
      [[ -n "${HANDLE:-}" ]] || die "PASSPHRASE needs HANDLE"
      echo "docker_gateway_start: importing the account for ${HANDLE} (first start)" >&2
      args+=(--passphrase "$PASSPHRASE" --handle "$HANDLE")
    elif [[ ! -f "$DATA/state.json" ]]; then
      die "$DATA/state.json does not exist yet: the first start needs PASSPHRASE and HANDLE"
    fi
    exec "$BINGLE_CLI" "${args[@]}"
    ;;
  mailer)
    [[ -n "${TO_EMAIL:-}" ]] || die "TO_EMAIL is not set"
    export MAILER_SEND=ses-api
    export SES_RELAY_FROM="$FROM_ADDRESS"
    export SES_REGION="${SES_REGION:-${AWS_REGION:-}}"
    exec "$MAILER" "$TO_EMAIL" "$DATA/maildir"
    ;;
  *)
    die "unknown role '$1'; expected logclient or mailer"
    ;;
esac
