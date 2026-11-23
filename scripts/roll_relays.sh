#!/usr/bin/env bash
# Roll a network's relays onto one commit, and record what each network runs (issue #399).
#
#   scripts/roll_relays.sh --dry-run          # preview: check everything, change nothing
#   scripts/roll_relays.sh                    # roll every relay in the inventory
#   scripts/roll_relays.sh --check-relays RELAY_CLIENT
#         after the roll, also run `bingle_cli checkrelays` as that account (a file name in
#         BINGLE_ACCOUNTS_DIR, without .json) and require it to pass
#   scripts/roll_relays.sh --check-inventory <file>
#         parse and print an inventory file, then exit (no AWS or git needed)
#
# Run it after `source ~/.config/bingle/bingle_env.sh testnet|mainnet`, which signs in to AWS and
# sets BINGLE_NETWORK and BINGLE_RELAYS_FILE. The relays come from that inventory file (the format
# is aws/relays.example.env); this repository names no relay of its own.
#
# What it rolls:
#   testnet  the commit checked out here, which must be on origin/staging (normally its head)
#   mainnet  the commit checked out here, which must be origin/deployed and carry its vX.Y.Z tag
# The tree must be clean, because the image is built from this checkout.
#
# The relay image is built once, as bingle-relay:<tag>; every relay runs the same image, since its
# handle and passphrase come from its stack. Each relay is then rolled in inventory order with
# aws/deploy_relay.sh --redeploy-only --no-build (image only; the passphrase stays in the stack's
# Secrets Manager secret), then health-gated: the relay must log a start of this commit's build
# within START_TIMEOUT seconds, and no RELAY STARTUP ABORTED. The first relay that fails stops the
# roll, so the rest keep their old image. A relay already on this image is restarted on it, so
# re-running is safe.
#
# On MainNet the inventory can also name the email gateway (GATEWAY lines, #489; there is no TestNet
# gateway, so a TestNet inventory may not have one). After every relay has rolled, each gateway is
# deployed onto the same commit: its image is built once, as bingle-gateway:<tag>, then deployed with
# aws/deploy_gateway.sh --redeploy-only --no-build, which keeps its recipient, sender and other
# settings, and health-gated the same way: logclient must log a start of this commit's build. A
# gateway that fails fails the roll, but the relays already rolled keep the new image.
#
# The roll is recorded as a GitHub deployment in the `testnet` or `mainnet` environment, with its
# outcome. That record says which commit each network runs, and the release reads it (#399). The
# record holds only the commit, version, image tag and relay and gateway counts: bingle_rust is
# public, so no AWS account, stack name or relay handle goes into it.
#
# Env: START_TIMEOUT (300s), GH_REPO (binglefoundation/bingle_rust), BINGLE_NO_RECORD=1 (skip the
# GitHub deployment record, e.g. when gh has no deployments permission).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

GH_REPO=${GH_REPO:-binglefoundation/bingle_rust}
# The image repository deploy_relay.sh pushes to by default.
IMAGE_REPO=bingle-relay
# The image repository deploy_gateway.sh pushes to by default.
GATEWAY_IMAGE_REPO=bingle-gateway
START_TIMEOUT=${START_TIMEOUT:-300}

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
ok()   { printf '    \033[1;32mok\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarn:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# ── inventory ─────────────────────────────────────────────────────────
# The inventory is data, read line by line and never sourced: RELAY repeats, and its values hold
# spaces. Sets INV_NETWORK, INV_ACCOUNT and the parallel arrays R_HANDLE, R_STACK, R_REGION, R_MODE,
# R_NODE_FILE, one entry per RELAY line in file order, and G_STACK, G_RELAY_STACK, G_REGION, one per
# GATEWAY line.
parse_inventory() { # <file>
  local file=$1 line n=0 rest extra
  local handle stack region mode node_file relay_stack
  [[ -f "$file" ]] || die "relay inventory '$file' not found"
  INV_NETWORK="" INV_ACCOUNT=""
  R_HANDLE=() R_STACK=() R_REGION=() R_MODE=() R_NODE_FILE=()
  G_STACK=() G_RELAY_STACK=() G_REGION=()
  while IFS= read -r line || [[ -n "$line" ]]; do
    n=$((n + 1))
    line=${line%$'\r'}
    case "$line" in
      ''|'#'*) continue ;;
      NETWORK=*)
        [[ -z "$INV_NETWORK" ]] || die "$file:$n: NETWORK given twice"
        INV_NETWORK=${line#NETWORK=} ;;
      ACCOUNT_ID=*)
        [[ -z "$INV_ACCOUNT" ]] || die "$file:$n: ACCOUNT_ID given twice"
        INV_ACCOUNT=${line#ACCOUNT_ID=}
        [[ "$INV_ACCOUNT" =~ ^[0-9]{12}$ ]] || die "$file:$n: ACCOUNT_ID must be a 12-digit AWS account id" ;;
      RELAY=*)
        rest=${line#RELAY=}
        extra=""
        read -r handle stack region mode node_file extra <<< "$rest"
        [[ -n "$node_file" && -z "$extra" ]] \
          || die "$file:$n: a RELAY line has five fields: <handle> <stack name> <region> <ec2|express> <in-container node file>"
        [[ "$mode" == ec2 || "$mode" == express ]] || die "$file:$n: mode must be ec2 or express, not '$mode'"
        [[ "$node_file" == /app/* ]] || die "$file:$n: the node file is the path inside the container (/app/...), not '$node_file'"
        R_HANDLE+=("$handle") R_STACK+=("$stack") R_REGION+=("$region") R_MODE+=("$mode") R_NODE_FILE+=("$node_file") ;;
      GATEWAY=*)
        rest=${line#GATEWAY=}
        extra=""
        read -r stack relay_stack region extra <<< "$rest"
        [[ -n "$region" && -z "$extra" ]] \
          || die "$file:$n: a GATEWAY line has three fields: <gateway stack name> <relay stack name> <region>"
        G_STACK+=("$stack") G_RELAY_STACK+=("$relay_stack") G_REGION+=("$region") ;;
      *) die "$file:$n: unrecognised line (expected NETWORK=, ACCOUNT_ID=, RELAY=, GATEWAY= or a # comment): $line" ;;
    esac
  done < "$file"
  [[ "$INV_NETWORK" == testnet || "$INV_NETWORK" == mainnet ]] || die "$file: NETWORK must be testnet or mainnet (got '${INV_NETWORK}')"
  [[ -n "$INV_ACCOUNT" ]] || die "$file: ACCOUNT_ID is missing"
  [[ ${#R_HANDLE[@]} -gt 0 ]] || die "$file: no RELAY lines"
  [[ ${#G_STACK[@]} -eq 0 || "$INV_NETWORK" == mainnet ]] \
    || die "$file: GATEWAY lines are for MainNet only; there is no $INV_NETWORK gateway"
  local i j found
  for ((i = 0; i < ${#R_STACK[@]}; i++)); do
    for ((j = i + 1; j < ${#R_STACK[@]}; j++)); do
      [[ "${R_STACK[i]}/${R_REGION[i]}" != "${R_STACK[j]}/${R_REGION[j]}" ]] || die "$file: stack ${R_STACK[i]} in ${R_REGION[i]} is listed twice"
      [[ "${R_HANDLE[i]}" != "${R_HANDLE[j]}" ]] \
        || die "$file: handle ${R_HANDLE[i]} is listed for two relays; one account runs one relay"
    done
  done
  # A gateway runs in a relay stack's cluster, so that relay must be in this inventory, in the same
  # region.
  for ((i = 0; i < ${#G_STACK[@]}; i++)); do
    found=0
    for ((j = 0; j < ${#R_STACK[@]}; j++)); do
      [[ "${R_STACK[j]}/${R_REGION[j]}" == "${G_RELAY_STACK[i]}/${G_REGION[i]}" ]] && found=1
    done
    [[ $found -eq 1 ]] \
      || die "$file: gateway ${G_STACK[i]} runs on relay stack ${G_RELAY_STACK[i]} in ${G_REGION[i]}, which is not a RELAY in this inventory"
    for ((j = i + 1; j < ${#G_STACK[@]}; j++)); do
      [[ "${G_STACK[i]}/${G_REGION[i]}" != "${G_STACK[j]}/${G_REGION[j]}" ]] || die "$file: gateway ${G_STACK[i]} in ${G_REGION[i]} is listed twice"
    done
  done
}

print_inventory() {
  local i
  echo "network $INV_NETWORK, AWS account $INV_ACCOUNT, ${#R_HANDLE[@]} relay(s):"
  for ((i = 0; i < ${#R_HANDLE[@]}; i++)); do
    printf '  %d. %-20s stack %-32s %-14s %-7s %s\n' $((i + 1)) "${R_HANDLE[i]}" "${R_STACK[i]}" "${R_REGION[i]}" "${R_MODE[i]}" "${R_NODE_FILE[i]}"
  done
  [[ ${#G_STACK[@]} -gt 0 ]] || return 0
  echo "${#G_STACK[@]} gateway(s), deployed after the relays:"
  for ((i = 0; i < ${#G_STACK[@]}; i++)); do
    printf '  %d. stack %-32s on relay stack %-24s %s\n' $((i + 1)) "${G_STACK[i]}" "${G_RELAY_STACK[i]}" "${G_REGION[i]}"
  done
}

# ── arguments ─────────────────────────────────────────────────────────
DRY_RUN=0
CHECK_ACCOUNT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --check-relays) [[ -n "${2:-}" ]] || die "--check-relays needs an account name"; CHECK_ACCOUNT=$2; shift 2 ;;
    --check-inventory)
      [[ -n "${2:-}" ]] || die "--check-inventory needs a file"
      parse_inventory "$2"; print_inventory; exit 0 ;;
    -h|--help) awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

# ── preflight ─────────────────────────────────────────────────────────
need() { command -v "$1" >/dev/null 2>&1 || die "required tool '$1' not found on PATH"; }
need git; need aws; need python3
[[ $DRY_RUN -eq 1 ]] || need docker
[[ -n "${BINGLE_NETWORK:-}" ]] || die "BINGLE_NETWORK is not set; run: source ~/.config/bingle/bingle_env.sh testnet (or mainnet)"
[[ -n "${BINGLE_RELAYS_FILE:-}" ]] || die "BINGLE_RELAYS_FILE is not set; point it at your relay inventory (format: aws/relays.example.env)"

info "relay inventory $BINGLE_RELAYS_FILE"
parse_inventory "$BINGLE_RELAYS_FILE"
print_inventory
[[ "$INV_NETWORK" == "$BINGLE_NETWORK" ]] \
  || die "the inventory is for $INV_NETWORK, but BINGLE_NETWORK is $BINGLE_NETWORK; run bingle_env $INV_NETWORK"

caller_account=$(aws sts get-caller-identity --query Account --output text 2>/dev/null) \
  || die "not signed in to AWS (AWS_PROFILE=${AWS_PROFILE:-<unset>}); run bingle_aws_login"
[[ "$caller_account" == "$INV_ACCOUNT" ]] \
  || die "signed in to AWS account $caller_account, but the $INV_NETWORK relays are in $INV_ACCOUNT; refusing to roll"
ok "AWS account $caller_account is the $INV_NETWORK account"

# ── the commit ────────────────────────────────────────────────────────
git fetch --quiet --tags origin staging deployed || die "could not fetch origin"
SHA=$(git rev-parse HEAD)
SHORT=$(git rev-parse --short=8 HEAD)
# the build number file is rewritten by some builds; any other change means the image would not be
# the commit being recorded.
if [[ -n "$(git status --porcelain --untracked-files=no -- . ':!.build_number')" ]]; then
  die "the tree has uncommitted changes; the image is built from this checkout, so commit or discard them first"
fi
VERSION=$(python3 - <<'PY'
import re, pathlib
m = re.search(r'\[package\][^\[]*?\nversion\s*=\s*"([^"]*)"', pathlib.Path("bingle_core/Cargo.toml").read_text(), re.S)
print(m.group(1) if m else "")
PY
)
[[ -n "$VERSION" ]] || die "could not read the version from bingle_core/Cargo.toml"

case "$INV_NETWORK" in
  testnet)
    git merge-base --is-ancestor "$SHA" origin/staging \
      || die "HEAD $SHORT is not on origin/staging; TestNet only runs staging commits (git checkout staging && git pull)"
    if [[ "$SHA" != "$(git rev-parse origin/staging)" ]]; then
      warn "HEAD $SHORT is an earlier staging commit, not the head of origin/staging ($(git rev-parse --short=8 origin/staging))"
    fi
    TAG="$VERSION-$SHORT" ;;
  mainnet)
    [[ "$SHA" == "$(git rev-parse origin/deployed)" ]] \
      || die "HEAD $SHORT is not origin/deployed ($(git rev-parse --short=8 origin/deployed)); MainNet only runs the released commit. Check it out, e.g. git worktree add tmp/deployed origin/deployed"
    tag_sha=$(git rev-parse -q --verify "refs/tags/v$VERSION^{commit}" 2>/dev/null || true)
    [[ "$tag_sha" == "$SHA" ]] \
      || die "v$VERSION does not name HEAD $SHORT (it names ${tag_sha:-nothing}); MainNet runs only a tagged release"
    TAG="$VERSION" ;;
esac
ok "commit $SHORT, version $VERSION, image tag $TAG"

# ── the stacks ────────────────────────────────────────────────────────
# Each stack must exist and run the handle and node file the inventory says, so a stale inventory or
# a hand-made stack is caught before anything changes.
stack_param() { # <stack> <region> <key>
  aws cloudformation describe-stacks --stack-name "$1" --region "$2" \
    --query "Stacks[0].Parameters[?ParameterKey=='$3'] | [0].ParameterValue" --output text
}
info "checking the stacks"
for ((i = 0; i < ${#R_HANDLE[@]}; i++)); do
  stack=${R_STACK[i]} region=${R_REGION[i]}
  aws cloudformation describe-stacks --stack-name "$stack" --region "$region" >/dev/null 2>&1 \
    || die "stack $stack is not in $region; deploy it first (admin runbook) or fix the inventory"
  live_handle=$(stack_param "$stack" "$region" Handle)
  live_node=$(stack_param "$stack" "$region" NodeFile)
  [[ "$live_handle" == "${R_HANDLE[i]}" ]] || die "stack $stack runs handle '$live_handle', but the inventory says '${R_HANDLE[i]}'"
  [[ "$live_node" == "${R_NODE_FILE[i]}" ]] || die "stack $stack reads node file '$live_node', but the inventory says '${R_NODE_FILE[i]}'"
  ok "$stack ($region): handle ${R_HANDLE[i]}, node file $live_node"
done
for ((i = 0; i < ${#G_STACK[@]}; i++)); do
  stack=${G_STACK[i]} region=${G_REGION[i]}
  aws cloudformation describe-stacks --stack-name "$stack" --region "$region" >/dev/null 2>&1 \
    || die "gateway stack $stack is not in $region; deploy it first (aws/deploy_gateway.sh, SUPPORT_EMAIL.md) or fix the inventory"
  ok "gateway $stack ($region) on relay stack ${G_RELAY_STACK[i]}"
done

if [[ -n "$CHECK_ACCOUNT" ]]; then
  [[ -n "${BINGLE_ACCOUNTS_DIR:-}" && -f "$BINGLE_ACCOUNTS_DIR/$CHECK_ACCOUNT.json" ]] \
    || die "--check-relays: $CHECK_ACCOUNT.json is not in BINGLE_ACCOUNTS_DIR (${BINGLE_ACCOUNTS_DIR:-<unset>})"
  [[ -n "${BINGLE_NODE_FILE:-}" && -f "$BINGLE_NODE_FILE" ]] || die "--check-relays: BINGLE_NODE_FILE is not set or missing"
fi

if [[ $DRY_RUN -eq 1 ]]; then
  info "dry run: would build image $TAG from $SHORT once, then roll, in order:"
  echo "    scripts/build_cli_image.sh --platform linux/arm64 --tag $IMAGE_REPO:$TAG"
  for ((i = 0; i < ${#R_HANDLE[@]}; i++)); do
    echo "    aws/deploy_relay.sh --redeploy-only --no-build --handle ${R_HANDLE[i]} --stack-name ${R_STACK[i]} --region ${R_REGION[i]} --tag $TAG$([[ ${R_MODE[i]} == express ]] && echo ' --express')"
  done
  if [[ ${#G_STACK[@]} -gt 0 ]]; then
    echo "    then build the gateway image: scripts/build_cli_image.sh --stage gateway --platform linux/arm64 --tag $GATEWAY_IMAGE_REPO:$TAG"
    for ((i = 0; i < ${#G_STACK[@]}; i++)); do
      echo "    aws/deploy_gateway.sh --redeploy-only --no-build --relay-stack ${G_RELAY_STACK[i]} --stack-name ${G_STACK[i]} --region ${G_REGION[i]} --tag $TAG"
    done
  fi
  [[ -n "$CHECK_ACCOUNT" ]] && echo "    then: bingle_cli checkrelays as $CHECK_ACCOUNT"
  [[ "${BINGLE_NO_RECORD:-0}" == 1 ]] || echo "    and record a $INV_NETWORK deployment of $SHORT in $GH_REPO"
  info "dry run: nothing changed"
  exit 0
fi

# ── the image ─────────────────────────────────────────────────────────
# Built once here; each deploy below pushes and deploys it without rebuilding.
info "building image $IMAGE_REPO:$TAG from $SHORT"
bash scripts/build_cli_image.sh --platform linux/arm64 --tag "$IMAGE_REPO:$TAG" \
  || die "could not build image $IMAGE_REPO:$TAG"
ok "built image $IMAGE_REPO:$TAG"
if [[ ${#G_STACK[@]} -gt 0 ]]; then
  info "building image $GATEWAY_IMAGE_REPO:$TAG from $SHORT"
  bash scripts/build_cli_image.sh --stage gateway --platform linux/arm64 --tag "$GATEWAY_IMAGE_REPO:$TAG" \
    || die "could not build image $GATEWAY_IMAGE_REPO:$TAG"
  ok "built image $GATEWAY_IMAGE_REPO:$TAG"
fi

# ── the deployment record ─────────────────────────────────────────────
DEPLOYMENT_ID=""
RUN_STATE=failure
record_status() { # <state> <description>
  [[ -n "$DEPLOYMENT_ID" ]] || return 0
  gh api -X POST "repos/$GH_REPO/deployments/$DEPLOYMENT_ID/statuses" -f state="$1" -f description="$2" >/dev/null \
    || warn "could not record the deployment status '$1'"
}
finish() {
  local ec=$?
  if [[ $ec -ne 0 || "$RUN_STATE" != success ]]; then
    record_status failure "$TAG: failed"
    [[ -n "$DEPLOYMENT_ID" ]] && warn "recorded a failed $INV_NETWORK deployment of $SHORT"
  fi
}
trap finish EXIT

if [[ "${BINGLE_NO_RECORD:-0}" != 1 ]]; then
  need gh
  production=false
  [[ "$INV_NETWORK" == mainnet ]] && production=true
  DEPLOYMENT_ID=$(python3 -c '
import json, sys
sha, env, version, tag, n, g, prod = sys.argv[1:]
print(json.dumps({"ref": sha, "environment": env, "auto_merge": False, "required_contexts": [],
  "transient_environment": False, "production_environment": prod == "true",
  "description": ("relays and gateway " if int(g) else "relays ") + tag,
  "payload": {"version": version, "image_tag": tag, "relays": int(n), "gateways": int(g)}}))
' "$SHA" "$INV_NETWORK" "$VERSION" "$TAG" "${#R_HANDLE[@]}" "${#G_STACK[@]}" "$production" \
    | gh api -X POST "repos/$GH_REPO/deployments" --input - -q .id) \
    || die "could not record a $INV_NETWORK deployment in $GH_REPO (set BINGLE_NO_RECORD=1 to roll without one)"
  record_status in_progress "rolling $TAG"
  ok "recorded $INV_NETWORK deployment $DEPLOYMENT_ID (in progress)"
fi

# ── roll ──────────────────────────────────────────────────────────────
# wait_for_start <stack> <region> <since ms> [log group]: the relay (or gateway) must log a start of
# this commit's build, and must not log RELAY STARTUP ABORTED. Version lines read
# "Bingle CLI starting. Version: X (git <sha>, ...". The log group defaults to the relay's.
wait_for_start() {
  local stack=$1 region=$2 since=$3 log_group=${4:-/bingle/relay/$1} deadline events
  deadline=$(( $(date +%s) + START_TIMEOUT ))
  while :; do
    events=$(aws logs filter-log-events --log-group-name "$log_group" --region "$region" \
      --start-time "$since" --query "events[].message" --output text 2>/dev/null || true)
    if grep -q "RELAY STARTUP ABORTED" <<< "$events"; then
      grep -A 8 "RELAY STARTUP ABORTED" <<< "$events" | head -n 20 >&2
      return 1
    fi
    if grep -q "Bingle CLI starting.*(git $SHA" <<< "$events"; then
      return 0
    fi
    [[ $(date +%s) -lt $deadline ]] || { warn "$stack logged no start of $SHORT within ${START_TIMEOUT}s"; return 1; }
    sleep 10
  done
}

for ((i = 0; i < ${#R_HANDLE[@]}; i++)); do
  handle=${R_HANDLE[i]} stack=${R_STACK[i]} region=${R_REGION[i]}
  info "rolling $handle ($stack, $region) onto $TAG"
  since=$(( $(date +%s) * 1000 ))
  mode_args=()
  [[ "${R_MODE[i]}" == express ]] && mode_args=(--express)
  aws/deploy_relay.sh --redeploy-only --no-build --handle "$handle" --stack-name "$stack" --region "$region" --tag "$TAG" ${mode_args[@]+"${mode_args[@]}"} \
    || die "$stack failed to roll; the relays after it were not touched"
  wait_for_start "$stack" "$region" "$since" \
    || die "$stack did not start cleanly on $SHORT; the relays after it were not touched"
  ok "$handle is running $SHORT"
done

if [[ -n "$CHECK_ACCOUNT" ]]; then
  info "checking relay discovery as $CHECK_ACCOUNT"
  cargo build -q -p bingle_cli --bin bingle_cli
  passphrase=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["passphrase"])' "$BINGLE_ACCOUNTS_DIR/$CHECK_ACCOUNT.json")
  target/debug/bingle_cli checkrelays --passphrase "$passphrase" --node-file "$BINGLE_NODE_FILE" --stun-servers-file stunservers.txt \
    || die "bingle_cli checkrelays failed after the roll"
  ok "relay discovery passed"
fi

for ((i = 0; i < ${#G_STACK[@]}; i++)); do
  stack=${G_STACK[i]} region=${G_REGION[i]}
  info "deploying gateway $stack ($region) onto $TAG"
  since=$(( $(date +%s) * 1000 ))
  aws/deploy_gateway.sh --redeploy-only --no-build --relay-stack "${G_RELAY_STACK[i]}" --stack-name "$stack" --region "$region" --tag "$TAG" \
    || die "gateway $stack failed to deploy; the relays keep $SHORT"
  wait_for_start "$stack" "$region" "$since" "/bingle/gateway/$stack" \
    || die "gateway $stack did not start cleanly on $SHORT; the relays keep $SHORT"
  ok "gateway $stack is running $SHORT"
done

RUN_STATE=success
record_status success "$TAG: success"
info "rolled ${#R_HANDLE[@]} $INV_NETWORK relay(s)$([[ ${#G_STACK[@]} -gt 0 ]] && echo " and ${#G_STACK[@]} gateway(s)") onto $SHORT ($TAG)"
