#!/usr/bin/env sh
# Single-node Sidewinder network on LocalNet for bingle_core end-to-end testing.
#
# The node runs in `on-chain-identity` mode (#246): it authorizes its clients from the Bingle DApp's
# on-chain allow bits and serves the client API over identity-bound mutual Transport Layer Security
# (mTLS). There is no bearer token and no plaintext client API. A client finds the node on-chain from
# the Bingle app id (the node publishes its endpoint at boot) and connects by app id only.
#
# Bringing a node up is two steps, because the on-chain enrolment needs a Bingle tool and this script
# drives only `sw-node`:
#
#   1. `accounts` generates (or reuses) and funds a **node** account and two client accounts — a
#      **sender** and a **receiver** — and prints their addresses and mnemonics.
#   2. The caller deploys (or picks) a Bingle app and enrols the accounts on it: the node account is
#      opted in and given `allow_sw_node` by the app admin, and each client registers a handle (which
#      grants `allow_sw_client`). The bingle_test `localnet::sidewinder` helpers do this.
#   3. `up` (with BINGLE_APP_ID set) writes the node config, starts the node and waits until its
#      client API is listening. The node publishes its endpoint once, at boot, and only an account
#      holding `allow_sw_node` may publish, so step 2 must come first.
#
# Self-contained: it drives the **installed `sw-node`** binary and carries its own application config,
# so it needs no repository checkout or build — copy this one file anywhere and run it.
#
# Prereqs:
#   - sw-node on PATH:        cargo install sidewinder-node --version "$SW_NODE_VERSION" --locked
#   - algokit LocalNet up:    algokit localnet start
#   - curl                    (for the LocalNet reachability check)
#
# Usage:
#   e2e_sidewinder_localnet.sh accounts   # generate/reuse + fund the accounts, print their details
#   BINGLE_APP_ID=<id> e2e_sidewinder_localnet.sh up   # start the node (accounts enrolled on <id>)
#   e2e_sidewinder_localnet.sh down       # stop the node and remove the work dir
set -eu

WORK=${WORK:-./tmp/bingle-e2e}
API_PORT=${API_PORT:-1080}
PEER_PORT=${PEER_PORT:-9101}
FUND_ALGOS=${FUND_ALGOS:-100}
# the installed binary by default; override with a path (e.g. a repo build) if you prefer.
SW_NODE=${SW_NODE:-sw-node}
# the sw-node version this script's node config is written for. 0.1.1 is the first release that admits
# transactions from on-chain-enrolled clients (0.1.0 still checks the allowlist's `callers:`). The
# config below loads on 0.1.x only: the first release after sidewinder#562 rejects `api.auth_token`, so
# drop that line when moving the pin past 0.1.x.
# Set SW_NODE_VERSION= (empty) to skip the check, e.g. for a repo build.
SW_NODE_VERSION=${SW_NODE_VERSION-0.1.1}
# the Bingle DApp app id the node authorizes clients against and publishes its endpoint to (`up` only).
BINGLE_APP_ID=${BINGLE_APP_ID:-}
# the host clients reach the node's client API at, published on-chain for discovery. Loopback by
# default; an Android emulator e2e sets the emulator's host alias (10.0.2.2).
ADVERTISE_HOST=${ADVERTISE_HOST:-127.0.0.1}
# how often (seconds) the node refreshes its membership cache from the chain.
MEMBERSHIP_POLL_SECS=${MEMBERSHIP_POLL_SECS:-2}
# LocalNet algod, used only for the pre-flight reachability check (funding goes via sw-node → kmd).
ALGOD_URL=${ALGOD_URL:-http://localhost:4001}
ALGOD_TOKEN=${ALGOD_TOKEN:-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa}
# optional: an application config file to use instead of the built-in Mailbox registry.
APP_CONFIG=${APP_CONFIG:-}

die() { echo "e2e_sidewinder_localnet: $1" >&2; exit 1; }

# write the built-in Mailbox application config (post / pop / drain over the FIFO primitive) — carried
# here so the harness needs no repo files.
write_app_config() {
  cat > "$1" <<'YAML'
transaction_types:
  - id: 1
    name: FIFO
    dispatch:
      standard: { class: FIFO, method: append, signature: "post(byte[],byte[])byte[]" }
    key_source: "arg:0"
    access_predicate: any-enrolled
    result: "byte[]"
    assurance: { f: 0 }
  - id: 2
    name: FIFO
    dispatch:
      standard: { class: FIFO, method: remove_head, signature: "pop()byte[]" }
    key_source: sender
    access_predicate: owner
    result: "byte[]"
    assurance: { f: 0 }
  - id: 3
    name: FIFO
    dispatch:
      standard: { class: FIFO, method: remove_all, signature: "drain()byte[]" }
    key_source: sender
    access_predicate: owner
    result: "byte[]"
    assurance: { f: 0 }
YAML
}

stop_node() { [ -f "$WORK/node.pid" ] && kill "$(cat "$WORK/node.pid")" 2>/dev/null || true; }

ACTION=${1:-up}
case "$ACTION" in
  accounts|up) ;;
  down)
    stop_node
    rm -rf "$WORK"
    echo "e2e down: stopped the node and removed $WORK" >&2
    exit 0
    ;;
  *) die "unknown action '$ACTION' (accounts | up | down)" ;;
esac

# --- pre-flight: prereqs present, the pinned sw-node version, and LocalNet reachable ---
command -v "$SW_NODE" >/dev/null 2>&1 \
  || die "sw-node not found ('$SW_NODE') — install it: cargo install sidewinder-node --version $SW_NODE_VERSION --locked, or set SW_NODE=/path/to/sw-node"
command -v curl >/dev/null 2>&1 || die "curl not found — needed for the LocalNet reachability check"
if [ -n "$SW_NODE_VERSION" ]; then
  installed=$("$SW_NODE" --version 2>/dev/null | sed -n 's/^sw-node //p')
  [ "$installed" = "$SW_NODE_VERSION" ] \
    || die "sw-node is version '${installed:-unknown}', this script needs $SW_NODE_VERSION — cargo install sidewinder-node --version $SW_NODE_VERSION --locked (or set SW_NODE_VERSION= to skip the check)"
fi
[ -z "$APP_CONFIG" ] || [ -f "$APP_CONFIG" ] || die "application config not found: $APP_CONFIG"
curl -fsS -o /dev/null -H "X-Algo-API-Token: $ALGOD_TOKEN" "$ALGOD_URL/v2/status" 2>/dev/null \
  || die "LocalNet algod unreachable at $ALGOD_URL — start it: algokit localnet start"
if [ "$ACTION" = "up" ]; then
  case "$BINGLE_APP_ID" in
    ''|0|*[!0-9]*) die "up needs BINGLE_APP_ID=<Bingle app id> with the node account enrolled on it (see the header)" ;;
  esac
fi

stop_node                       # stop a node from a previous `up` (keeps the account files for reuse)
mkdir -p "$WORK/etc" "$WORK/data"

# parse a `key: value` line from sw-node's output.
field() { printf '%s\n' "$1" | sed -n "s/^$2: //p"; }

# --- generate (or reuse) and fund the three accounts on LocalNet ---
echo "generating + funding accounts on LocalNet ($FUND_ALGOS ALGO each)..." >&2
node_out=$("$SW_NODE" account --out "$WORK/etc/account.key" --fund-localnet --amount "$FUND_ALGOS") \
  || die "node account generate/fund failed (is LocalNet funded and kmd reachable?)"
sender_out=$("$SW_NODE" account --out "$WORK/sender.key" --fund-localnet --amount "$FUND_ALGOS") \
  || die "sender account generate/fund failed"
receiver_out=$("$SW_NODE" account --out "$WORK/receiver.key" --fund-localnet --amount "$FUND_ALGOS") \
  || die "receiver account generate/fund failed"

NODE_ADDR=$(field "$node_out" address);       NODE_MN=$(field "$node_out" mnemonic)
SENDER_ADDR=$(field "$sender_out" address);   SENDER_MN=$(field "$sender_out" mnemonic)
RECEIVER_ADDR=$(field "$receiver_out" address); RECEIVER_MN=$(field "$receiver_out" mnemonic)

print_accounts() {
  cat <<EOF
[node]
  address  : $NODE_ADDR
  mnemonic : $NODE_MN

[sender]
  address  : $SENDER_ADDR
  mnemonic : $SENDER_MN

[receiver]
  address  : $RECEIVER_ADDR
  mnemonic : $RECEIVER_MN
EOF
}

if [ "$ACTION" = "accounts" ]; then
  print_accounts
  exit 0
fi

# --- network setup: BLS consensus key, allowlist, parent-chain, node config ---
"$SW_NODE" keygen --out "$WORK/etc/consensus.key" --account "$NODE_ADDR" > "$WORK/keygen.out" 2>&1 \
  || { cat "$WORK/keygen.out" >&2; die "keygen failed (a stale consensus.key? run 'down' first)"; }
BLS=$(sed -n 's/^[[:space:]]*bls_public_key:[[:space:]]*//p' "$WORK/keygen.out")
POP=$(sed -n 's/^[[:space:]]*proof_of_possession:[[:space:]]*//p' "$WORK/keygen.out")

if [ -n "$APP_CONFIG" ]; then cp "$APP_CONFIG" "$WORK/etc/application.yaml"; else write_app_config "$WORK/etc/application.yaml"; fi
cat > "$WORK/etc/parent-chain.json" <<EOF
{ "network": "localnet", "client_api_url": "http://localhost", "client_api_port": 4001, "token": "$ALGOD_TOKEN" }
EOF
# the node set only: in on-chain-identity mode clients are enrolled by their on-chain allow bit.
cat > "$WORK/etc/allowlist.yaml" <<EOF
nodes:
  - account: "$NODE_ADDR"
    bls_public_key: "$BLS"
    proof_of_possession: "$POP"
EOF
# `api.auth_token` is required by sw-node 0.1.x and ignored under mutual TLS; the release after
# sidewinder#562 rejects it, so remove that line when moving SW_NODE_VERSION past 0.1.x.
cat > "$WORK/etc/node.yaml" <<EOF
index: 0
data_dir: $(cd "$WORK/data" && pwd)
application_config: application.yaml
allowlist: allowlist.yaml
parent_chain: parent-chain.json
consensus_key: consensus.key
account_key: account.key
listen: "0.0.0.0:$PEER_PORT"
auth_mode:
  mode: on-chain-identity
  app_id: $BINGLE_APP_ID
  poll_interval_secs: $MEMBERSHIP_POLL_SECS
  dapp: bingle
advertise:
  ipv4: "$ADVERTISE_HOST:$API_PORT"
api:
  listen: "0.0.0.0:$API_PORT"
  auth_token: ""
EOF

# --- start the node (background local process; survives this script, stopped by `down`) ---
nohup "$SW_NODE" run --node-file "$WORK/etc/node.yaml" > "$WORK/node.log" 2>&1 &
echo $! > "$WORK/node.pid"

# wait for the node's single readiness line. The client API is mutual TLS only, so a plain probe of
# /health cannot tell a ready node from a refused handshake.
ready=""
i=0
while [ "$i" -lt 60 ]; do
  if grep -q 'NODE READY' "$WORK/node.log" 2>/dev/null; then ready=1; break; fi
  kill -0 "$(cat "$WORK/node.pid")" 2>/dev/null || break
  sleep 1; i=$((i + 1))
done
[ -n "$ready" ] || { tail -20 "$WORK/node.log" >&2; stop_node; die "node did not become ready — see $WORK/node.log"; }

cat <<EOF

=== bingle_core LocalNet endpoints ===
Bingle app id : $BINGLE_APP_ID
API endpoint  : https://$ADVERTISE_HOST:$API_PORT (mutual TLS; discovered on-chain from the app id)
Application   : Mailbox — post(recipient, msg) / pop() / drain()

EOF
print_accounts
cat <<EOF

node pid $(cat "$WORK/node.pid") — logs: $WORK/node.log
stop with: $0 down
EOF
