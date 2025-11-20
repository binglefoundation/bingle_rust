#!/usr/bin/env bash
# setup_static_user.sh
#
# Purpose: Create and register a new Bingle testnet static user end-to-end.
#
# Requirements (tools on PATH):
#   - algokey (to generate a new account and derive addresses from mnemonics)
#   - algokit (preferred) or goal (with a local node data dir) to fund the new account from CREATOR_PASSPHRASE
#   - bingle_cli (CLI from this repo, already built)
#   - bingle_admin (external admin CLI) providing `setallowstatic`
#
# Environment variables (required):
#   CREATOR_PASSPHRASE  - mnemonic/private key for the DApp creator account on testnet (payer/admin)
#   HANDLE              - handle to register for the new account (globally unique)
#   STATIC_IP           - public IP or DNS for the static endpoint
#   STATIC_PORT         - public port for the static endpoint
#
# Environment variables (optional):
#   NODE_FILE        - path to node file JSON (defaults to ./nodely_testnet_node.json)
#                      May include app_id/asset_id; otherwise APP_ID/ASSET_ID env vars are used by bingle_cli.
#   PRICE_UNITS      - price in Bingle units for handle registration (default: 1)
#   FUND_ALGOS       - amount to fund the new account from creator, in ALGOs (default: 2)
#   ALGOKIT_NETWORK  - algokit network name (e.g., testnet, localnet). If set, used with `algokit task transfer -n`. Default: testnet.
#   GOAL_DATA_DIR    - path to an Algorand node data directory (use only if funding via goal against that node)
#   EXTRA_ARGS       - extra flags appended to bingle_cli calls
#   ADMIN_ARGS       - extra flags appended to bingle_admin calls
#
# Notes:
# - This script targets testnet by default using the bundled nodely_testnet_node.json.
# - It attempts to fund using `algokit` first for testnet and remote use. If unavailable, it will try `goal` only when GOAL_DATA_DIR is provided (local node).
# - The exact flags for bingle_admin and bingle_cli registerstatic may vary by your setup.
#   Adjust ADMIN_ARGS / EXTRA_ARGS accordingly if needed.
set -euo pipefail

# Enable extra tracing when DEBUG=1
if [[ "${DEBUG:-0}" == "1" ]]; then
  set -x
fi

# ------------ Helpers ------------
log() { printf "[setup_static_user] %s\n" "$*"; }
fail() { printf "[setup_static_user][ERROR] %s\n" "$*" >&2; exit 1; }
need_cmd() { command -v "$1" >/dev/null 2>&1 || fail "Missing required command: $1"; }

# Derive an address from a mnemonic using algokey (try a few subcommands for portability)
derive_addr_from_mnemonic() {
  local mn="$1"
  # Preferred: algokey import -m '<mnemonic>' (correct syntax per latest algokey)
  if out=$(algokey import -m "$mn" 2>/dev/null); then
    echo "$out" | awk '
      BEGIN{found=0; cand=""}
      { for (i=1;i<=NF;i++){ if ($i ~ /^[A-Z0-9]{50,60}$/){ cand=$i } } }
      /^(Address:|Public key:)/ { for (i=1;i<=NF;i++){ if ($i ~ /^[A-Z0-9]{50,60}$/){ print $i; found=1 } } }
      END{ if (!found) { if (cand!="") { print cand } else { exit 1 } } }
    ' && return 0 || true
  fi
  # Fallbacks for various algokey versions
  if out=$(algokey inspect -m "$mn" 2>/dev/null); then
    echo "$out" | awk '
      BEGIN{found=0; cand=""}
      { for (i=1;i<=NF;i++){ if ($i ~ /^[A-Z0-9]{50,60}$/){ cand=$i } } }
      /^(Address:|Public key:)/ { for (i=1;i<=NF;i++){ if ($i ~ /^[A-Z0-9]{50,60}$/){ print $i; found=1 } } }
      END{ if (!found) { if (cand!="") { print cand } else { exit 1 } } }
    ' && return 0 || true
  fi
  if out=$(algokey from-mnemonic -m "$mn" 2>/dev/null); then
    echo "$out" | awk '
      BEGIN{found=0; cand=""}
      { for (i=1;i<=NF;i++){ if ($i ~ /^[A-Z0-9]{50,60}$/){ cand=$i } } }
      /^(Address:|Public key:)/ { for (i=1;i<=NF;i++){ if ($i ~ /^[A-Z0-9]{50,60}$/){ print $i; found=1 } } }
      END{ if (!found) { if (cand!="") { print cand } else { exit 1 } } }
    ' && return 0 || true
  fi
  if out=$(algokey account -m "$mn" 2>/dev/null); then
    echo "$out" | awk '
      BEGIN{found=0; cand=""}
      { for (i=1;i<=NF;i++){ if ($i ~ /^[A-Z0-9]{50,60}$/){ cand=$i } } }
      /^(Address:|Public key:)/ { for (i=1;i<=NF;i++){ if ($i ~ /^[A-Z0-9]{50,60}$/){ print $i; found=1 } } }
      END{ if (!found) { if (cand!="") { print cand } else { exit 1 } } }
    ' && return 0 || true
  fi
  return 1
}

# Validate an Algorand address (heuristic: uppercase base32-ish length ~58)
validate_address() {
  local addr="$1"
  [[ -n "$addr" && ${#addr} -ge 50 && ${#addr} -le 60 && "$addr" =~ ^[A-Z0-9]+$ ]]
}

# Validate an Algorand mnemonic (expect at least 25 words)
validate_mnemonic() {
  local m="$1"
  local wc
  # shellcheck disable=SC2005
  wc=$(echo "$m" | awk '{print NF}')
  [[ -n "$m" && "$wc" -ge 25 ]]
}

# Parse algokey generate output to get address and mnemonic
parse_algokey_generate() {
  # Reads from stdin, prints two lines: ADDRESS\nMNEMONIC
  awk '
    function ltrim(s){ sub(/^[ \t\r\n]+/, "", s); return s }
    BEGIN{addr=""; mnem=""; getting=0; cand=""; IGNORECASE=1}
    {
      # keep a candidate address token if we see a long uppercase/number token
      for (i=1; i<=NF; i++) {
        if ($i ~ /^[A-Z0-9]{50,60}$/) { cand=$i }
      }
    }
    /^(Address:|Public key:)/ {
      for (i=1; i<=NF; i++) {
        if ($i ~ /^[A-Z0-9]{50,60}$/) { addr=$i }
      }
      next
    }
    /^Private key mnemonic[:)]/ {
      line=$0
      sub(/^Private key mnemonic[:)]?[ \t]*/, "", line)
      mnem=line
      if (mnem=="") getting=1
      next
    }
    getting==1 {
      if (mnem!="") mnem=mnem" "
      mnem=mnem $0
      n=split(mnem, a, /[ \t]+/)
      if (n>=25) getting=0
      next
    }
    END{
      if (addr=="" && cand!="") addr=cand
      mnem=ltrim(mnem)
      if (addr=="" || mnem=="") exit 1
      print addr
      print mnem
    }
  '
}

# Fund using goal by building & signing a transaction with creator mnemonic
fund_with_goal() {
  if [[ -z "${GOAL_DATA_DIR:-}" ]]; then
    fail "GOAL_DATA_DIR is not set; cannot use 'goal' without a local node data directory. Install/Use algokit or set GOAL_DATA_DIR to your node's data dir."
  fi
  local creator_mn="$1" new_addr="$2" micro_algos="$3"
  need_cmd goal
  need_cmd algokey
  log "Using goal to fund ${new_addr} with $((micro_algos/1000000)) ALGO"
  # Derive creator address
  local creator_addr
  creator_addr=$(derive_addr_from_mnemonic "$creator_mn") || fail "Unable to derive creator address from mnemonic using algokey"
  log "Creator address: ${creator_addr}"
  # Create unsigned txn
  local tmpdir; tmpdir=$(mktemp -d)
  local unsigned="${tmpdir}/txn.txn" signed="${tmpdir}/stxn.stxn"
  goal -d "$GOAL_DATA_DIR" clerk send -a "$micro_algos" -f "$creator_addr" -t "$new_addr" -o "$unsigned"
  # Sign with mnemonic (goal supports -M for mnemonic in many versions)
  if goal clerk sign -i "$unsigned" -o "$signed" -M "$creator_mn" 2>/dev/null; then
    :
  else
    # Fallback: some goal builds use --mnemonic
    goal clerk sign -i "$unsigned" -o "$signed" --mnemonic "$creator_mn"
  fi
  goal clerk rawsend -f "$signed"
  rm -rf "$tmpdir"
}

# Fund using algokit CLI (task transfer). Some versions require a TTY for mnemonic entry.
# We try plain stdin pipe first; if that fails, fall back to an expect-driven interaction when available.
fund_with_algokit() {
  local creator_mn="$1" new_addr="$2" micro_algos="$3"
  need_cmd algokit
  # Derive sender address from mnemonic
  local sender_addr
  sender_addr=$(derive_addr_from_mnemonic "$creator_mn") || fail "Unable to derive creator address from mnemonic using algokey"
  local network
  network=${ALGOKIT_NETWORK:-testnet}
  log "Using algokit task to fund ${new_addr} with $((micro_algos/1000000)) ALGO from ${sender_addr} on ${network}"
  # Build command
  local cmd=( algokit task transfer \
    -s "$sender_addr" \
    -r "$new_addr" \
    -a "$micro_algos" \
    -n "$network" )

  # Attempt 1: pipe mnemonic to stdin (works if algokit reads from stdin)
  if printf '%s\n' "$creator_mn" | "${cmd[@]}"; then
    return 0
  fi

  # Attempt 2: here-string (some shells/cli detect differently)
  if "${cmd[@]}" <<<"$creator_mn"; then
    return 0
  fi

  # Attempt 3: use expect to interact with TTY-style prompt
  if command -v expect >/dev/null 2>&1; then
    log "algokit appears to require a TTY; retrying with expect automation"
    # shellcheck disable=SC2016
    SENDER_ADDR="$sender_addr" \
    RECEIVER_ADDR="$new_addr" \
    AMOUNT_UALGOS="$micro_algos" \
    ALGOKIT_NET="$network" \
    CREATOR_MN="$creator_mn" \
    expect <<'EOF'
      set timeout 120
      # Capture environment variables passed from shell
      set sender $env(SENDER_ADDR)
      set receiver $env(RECEIVER_ADDR)
      set amount $env(AMOUNT_UALGOS)
      set net $env(ALGOKIT_NET)
      set mnem $env(CREATOR_MN)
      spawn algokit task transfer -s $sender -r $receiver -a $amount -n $net
      expect {
        -re {(?i)Enter the mnemonic phrase.*:} { send -- "$mnem\r" }
        timeout { puts stderr "[expect] Timed out waiting for mnemonic prompt"; exit 2 }
      }
      # Some versions may print additional confirmations; just wait for process to end
      expect {
        eof { exit [lindex [wait] 3] }
        timeout { puts stderr "[expect] Timed out waiting for algokit to finish"; exit 3 }
      }
EOF
    local ec=$?
    if [[ $ec -eq 0 ]]; then
      return 0
    else
      fail "algokit funding via expect failed with exit code $ec. You can install 'expect' (e.g., brew install expect) or fund manually."
    fi
  fi

  fail "algokit funding failed and 'expect' is not installed. Install expect (e.g., 'brew install expect' on macOS) or set GOAL_DATA_DIR to use goal, or fund the account manually."
}

# ------------ Validate inputs ------------
: "${CREATOR_PASSPHRASE:?CREATOR_PASSPHRASE must be set to the creator mnemonic}"
: "${HANDLE:?HANDLE must be set to the new handle to register}"
: "${STATIC_IP:?STATIC_IP must be set}"
: "${STATIC_PORT:?STATIC_PORT must be set}"

NODE_FILE=${NODE_FILE:-"$(dirname "$0")/../nodely_testnet_node.json"}
PRICE_UNITS=${PRICE_UNITS:-1}
FUND_ALGOS=${FUND_ALGOS:-2}
EXTRA_ARGS=${EXTRA_ARGS:-}
ADMIN_ARGS=${ADMIN_ARGS:-}

# Ensure required tools exist
need_cmd algokey
need_cmd bingle_cli
need_cmd bingle_admin

# ------------ Generate a new account ------------
log "Generating new Algorand account with algokey"
NEW_ADDR=""; NEW_MNEMONIC=""
# Capture stdout and stderr separately for robust debugging
_gen_out_file=$(mktemp)
_gen_err_file=$(mktemp)
if algokey generate >"$_gen_out_file" 2>"$_gen_err_file"; then
  out=$(cat "$_gen_out_file")
  mapfile -t lines < <(echo "$out" | parse_algokey_generate || true)
  if [[ ${#lines[@]} -eq 2 ]]; then
    NEW_ADDR="${lines[0]}"
    NEW_MNEMONIC="${lines[1]}"
  fi
else
  out=$(cat "$_gen_out_file" 2>/dev/null || true)
fi
err=$(cat "$_gen_err_file" 2>/dev/null || true)
rm -f "$_gen_out_file" "$_gen_err_file"

# Validate we actually parsed something sensible
if ! validate_address "$NEW_ADDR" || ! validate_mnemonic "$NEW_MNEMONIC"; then
  printf "[setup_static_user][ERROR] Failed to parse algokey generate output.\n" >&2
  printf "[setup_static_user][ERROR] algokey stdout:\n%s\n" "${out:-<empty>}" >&2
  if [[ -n "$err" ]]; then
    printf "[setup_static_user][ERROR] algokey stderr:\n%s\n" "$err" >&2
  fi
  printf "[setup_static_user][ERROR] Parsed address: '%s' (valid=%s), mnemonic words: %s (valid=%s)\n" \
    "${NEW_ADDR}" \
    "$([[ $(validate_address "$NEW_ADDR"; echo $?) -eq 0 ]] && echo yes || echo no)" \
    "$(echo "$NEW_MNEMONIC" | awk '{print NF}')" \
    "$([[ $(validate_mnemonic "$NEW_MNEMONIC"; echo $?) -eq 0 ]] && echo yes || echo no)" \
    >&2
  fail "Unable to obtain address/mnemonic from algokey generate. If algokey format changed, please update the parser."
fi
log "New account: ${NEW_ADDR}"

# ------------ Fund the new account ------------
MICRO_ALGOS=$(( FUND_ALGOS * 1000000 ))
if command -v algokit >/dev/null 2>&1; then
  fund_with_algokit "$CREATOR_PASSPHRASE" "$NEW_ADDR" "$MICRO_ALGOS"
elif command -v goal >/dev/null 2>&1 && [[ -n "${GOAL_DATA_DIR:-}" ]]; then
  fund_with_goal "$CREATOR_PASSPHRASE" "$NEW_ADDR" "$MICRO_ALGOS"
else
  fail "Could not fund the new account automatically. algokit is not available and GOAL_DATA_DIR is not set for goal. Please install algokit (preferred for testnet) or set GOAL_DATA_DIR for a local node, or fund ${NEW_ADDR} with ${FUND_ALGOS} ALGO manually and rerun from registration step."
fi

log "Waiting a few seconds for funding transaction to confirm..."
sleep 5

# ------------ Register handle ------------
log "Registering handle '${HANDLE}' via bingle_cli"
CMD=( bingle_cli register \
  --handle "$HANDLE" \
  --passphrase "$NEW_MNEMONIC" \
  --price-units "$PRICE_UNITS" \
  --node-file "$NODE_FILE"
)
if [[ -n "$EXTRA_ARGS" ]]; then
  # shellcheck disable=SC2206
  EXTRA_ARR=( $EXTRA_ARGS )
  CMD+=( "${EXTRA_ARR[@]}" )
fi
log "Running: ${CMD[*]}"
"${CMD[@]}"

# ------------ Allow static via admin ------------
log "Marking account as allowed static via bingle_admin setallowstatic"
ACMD=( bingle_admin setallowstatic \
  --passphrase "$CREATOR_PASSPHRASE" \
  --handle "$HANDLE" \
  --node-file "$NODE_FILE"
)
if [[ -n "$ADMIN_ARGS" ]]; then
  # shellcheck disable=SC2206
  AEXTRA_ARR=( $ADMIN_ARGS )
  ACMD+=( "${AEXTRA_ARR[@]}" )
fi
log "Running: ${ACMD[*]}"
"${ACMD[@]}"

# ------------ Register static endpoint ------------
ENDPOINT="${STATIC_IP}:${STATIC_PORT}"
log "Registering static endpoint ${ENDPOINT} via bingle_cli registerstatic"
SCMD=( bingle_cli registerstatic \
  --passphrase "$NEW_MNEMONIC" \
  --static-ip "$ENDPOINT" \
  --node-file "$NODE_FILE"
)
if [[ -n "$EXTRA_ARGS" ]]; then
  # shellcheck disable=SC2206
  EXTRA_ARR=( $EXTRA_ARGS )
  SCMD+=( "${EXTRA_ARR[@]}" )
fi
log "Running: ${SCMD[*]}"
"${SCMD[@]}"

# ------------ Summary ------------
cat <<EOF

Setup complete.
New account address: ${NEW_ADDR}
New account mnemonic: ${NEW_MNEMONIC}
Handle: ${HANDLE}
Static endpoint: ${ENDPOINT}
Node file: ${NODE_FILE}

Please store the mnemonic securely.
EOF
