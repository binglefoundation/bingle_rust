#!/usr/bin/env bash
#
# bootstrap_testnet_accounts.sh
#
# Opt the long-lived testnet accounts (relays, test user, ping target) into the
# CURRENT Bingle app + asset and register their handles.
#
# Why this is needed: opt-in and handle registration are per-app local state. When
# the app is redeployed under a new app_id, these persistent accounts are still only
# opted into the OLD app and hold none of the new Bingle asset. `bingle_admin
# usersettings` (allow_relay/allow_static) and the e2e tests both assume the accounts
# are already opted into the app referenced by the node file, so they fail with
# "has not opted in to app ..." until the accounts are re-initialised on the new app.
#
# bingle_admin cannot do this for these accounts: `inituser` only creates NEW random
# accounts and `updateuser` bails unless the account is already registered. The
# supported flow for an existing, known-mnemonic account is the two client commands
# this script runs: `bingle_cli buybingle` (opt into the asset + acquire 1 Bingle,
# paying the current on-chain price in ALGO) followed by `bingle_cli register`
# (opt into the app local state, pay the 1-unit registration fee, set the handle).
#
# Idempotent: accounts already registered (opted into the current app with a Handle
# in local state) are skipped, so re-runs spend no ALGO and cannot hit re-register
# errors. Requires `bingle_cli` on PATH, plus `curl` and `python3` for the read-only
# on-chain state checks. Run from the repo root (default node file path is relative).
#
# Usage:
#   scripts/bootstrap_testnet_accounts.sh [--node-file <file>] [--only "h1 h2 ..."]
#
# Env:
#   NODE_FILE     default node file (overridden by --node-file); defaults below
#   ONLY_HANDLES  restrict to these handles (space-separated); same as --only
#   EXTRA_RELAY   when set, also bootstrap the extra relay (relayextra)

set -uo pipefail

NODE_FILE="${NODE_FILE:-nodely_staging_testnet_node.json}"
ONLY_HANDLES="${ONLY_HANDLES:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --node-file) NODE_FILE="$2"; shift 2;;
    --only) ONLY_HANDLES="$2"; shift 2;;
    -h|--help)
      echo "Usage: $0 [--node-file <file>] [--only \"handle1 handle2\"]"; exit 0;;
    *) echo "Unknown argument: $1" >&2; exit 2;;
  esac
done

if ! command -v bingle_cli >/dev/null 2>&1; then
  echo "ERROR: bingle_cli not found on PATH" >&2; exit 1
fi
if [[ ! -s "$NODE_FILE" ]]; then
  echo "ERROR: node file not found or empty: $NODE_FILE" >&2; exit 1
fi

# Persistent account table, kept in sync with run_testnet_tests.sh. Each entry is a
# parallel index across the three arrays: handle, address, mnemonic passphrase.
HANDLES=(relay20 relay21 testuser10 pinguser21)
ADDRESSES=(
  J3GHIF4QBJT7PEQHJ7YNJXP64RY7Q27GRB6HEFJ7O5E6JULGNSPVP546N4
  ZEBF7TPP3ZKVPBUSXDRZE2XIRLBRBYFQF6PFEXXLTFMOP6ETX3HFKG7D6Y
  YA2UAJPUJZBY4KR2B4FBM57NSA7252PJQTVKJEGB2MOISRUECW4JGE4USM
  EK2KRWCCCI4DRMSQIDYAING2NURDMDBVWDK6VCCDGQNBQ5DMGFPKRTAFGY
)
PASSPHRASES=(
  "parent diamond bring another suggest rice diamond gravity bench violin hover fat relax annual repeat keen use moon senior display laundry asthma trend absorb grab"
  "design coast gift sting park tooth comic load off feed super close civil divide orbit garden mutual boat wine analyst gospel stem pipe about ritual"
  "glide crawl soda hole assault tide fault century seed tip daughter student rice swap imitate setup like card reject claim truck squeeze same able remind"
  "scare much guide patch report explain collect feel climb mansion cluster child muscle split jewel crush wisdom length merry diary quote axis foil abstract escape"
)

# Optionally include the extra relay (dynamic-IP, STUN-only relay used by some runs).
if [[ -n "${EXTRA_RELAY:-}" ]]; then
  HANDLES+=(relayextra)
  ADDRESSES+=(3RLYTSRX54G5WOPPPV4FYWRV2QXKIC5WRPM54YKXGVLTAFGUEIG2QN4DMQ)
  PASSPHRASES+=("horror stuff huge crunch green marriage parent soon hamster tonight miracle company fee cup hard media shiver emotion hybrid shiver main cube lemon about obvious")
fi

# Resolve app_id, asset_id and the algod REST base URL from the node file.
read -r APP_ID ASSET_ID API_BASE < <(python3 - "$NODE_FILE" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
url = d["client_api_url"]
port = d.get("client_api_port")
if port and int(port) not in (80, 443):
    url = f"{url}:{port}"
print(d["app_id"], d["asset_id"], url)
PY
)
if [[ -z "${APP_ID:-}" || -z "${ASSET_ID:-}" || -z "${API_BASE:-}" ]]; then
  echo "ERROR: could not read app_id/asset_id/client_api_url from $NODE_FILE" >&2
  exit 1
fi

# Current Bingle price (microAlgos) from the app's global state, formatted as ALGO.
PRICE_MICRO=$(curl -fsS "$API_BASE/v2/applications/$APP_ID" 2>/dev/null | python3 -c "
import sys, json, base64
d = json.load(sys.stdin)
gs = d.get('params', {}).get('global-state', [])
for kv in gs:
    if base64.b64decode(kv['key']).decode('latin1') == 'BinglePrice':
        print(kv['value']['uint']); break
")
if [[ -z "${PRICE_MICRO:-}" ]]; then
  echo "ERROR: could not read BinglePrice from app $APP_ID via $API_BASE" >&2
  exit 1
fi
PRICE_ALGO=$(python3 -c "print(f'{$PRICE_MICRO/1_000_000:.6f}')")

echo "Bootstrapping testnet accounts on app_id=$APP_ID asset_id=$ASSET_ID (price ${PRICE_ALGO} ALGO)"

# Returns 0 if the address is opted into APP_ID with a Handle set in local state.
is_registered() {
  local json
  json=$(curl -fsS "$API_BASE/v2/accounts/$1" 2>/dev/null) || return 1
  APP_ID="$APP_ID" python3 -c '
import sys, os, json, base64
app_id = int(os.environ["APP_ID"])
d = json.loads(sys.stdin.read())
for a in d.get("apps-local-state", []):
    if a.get("id") == app_id:
        for kv in a.get("key-value", []) or []:
            if base64.b64decode(kv["key"]).decode("latin1") == "Handle" and kv["value"].get("bytes"):
                sys.exit(0)
sys.exit(1)
' <<<"$json"
}

# Prints the address's holding (units) of ASSET_ID, or 0.
asset_units() {
  local json
  json=$(curl -fsS "$API_BASE/v2/accounts/$1" 2>/dev/null) || { echo 0; return 0; }
  ASSET_ID="$ASSET_ID" python3 -c '
import sys, os, json
aid = int(os.environ["ASSET_ID"])
d = json.loads(sys.stdin.read())
print(next((a.get("amount", 0) for a in d.get("assets", []) if a.get("asset-id") == aid), 0))
' <<<"$json"
}

rc=0
for i in "${!HANDLES[@]}"; do
  h="${HANDLES[$i]}"
  addr="${ADDRESSES[$i]}"
  pass="${PASSPHRASES[$i]}"

  if [[ -n "$ONLY_HANDLES" ]] && ! grep -qw "$h" <<<"$ONLY_HANDLES"; then
    continue
  fi

  if is_registered "$addr"; then
    echo "[$h] already registered on app $APP_ID; skipping"
    continue
  fi

  echo "[$h] bootstrapping ($addr)"
  units=$(asset_units "$addr")
  if [[ "${units:-0}" -lt 1 ]]; then
    echo "[$h] buying 1 Bingle for ${PRICE_ALGO} ALGO"
    if ! bingle_cli buybingle "$PRICE_ALGO" --passphrase "$pass" --node-file "$NODE_FILE"; then
      echo "ERROR: [$h] buybingle failed" >&2; rc=1; continue
    fi
  fi

  echo "[$h] registering handle"
  if ! bingle_cli register --handle "$h" --passphrase "$pass" --price-units 1 --node-file "$NODE_FILE"; then
    echo "ERROR: [$h] register failed" >&2; rc=1; continue
  fi
  echo "[$h] done"
done

if [[ $rc -ne 0 ]]; then
  echo "Bootstrap completed with errors" >&2
else
  echo "Bootstrap complete"
fi
exit $rc
