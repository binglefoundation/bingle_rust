#!/usr/bin/env bash

# LEAVE_CONTAINERS=1 keeps docker run containers around after exit (no --rm)
LEAVE_CONTAINERS=${LEAVE_CONTAINERS:-0}

DOCKER_RUN_RM=()
if [[ "$LEAVE_CONTAINERS" != "1" ]]; then
  DOCKER_RUN_RM=(--rm)
fi

# Set TESTNET_ACCOUNTS_DIR to the testnet dir

# Ensure cleanup of background containers on exit
cleanup() {
  local exit_code=$?
  # Use a longer timeout for docker stop to ensure containers have time to finish memory reporting
  docker stop --time 30 bingle_relay_a bingle_relay_b bingle_relay_extra bingle_stun_a bingle_stun_b bingle_pingable >/dev/null 2>&1 || true
}
trap cleanup EXIT

# Wait for a file to appear with timeout (seconds)
wait_for_file() {
  local file="$1"; shift
  local timeout="${1:-120}"
  local elapsed=0
  echo "Waiting for sentinel: $file (timeout ${timeout}s)"
  while [ $elapsed -lt $timeout ]; do
    if [ -f "$file" ]; then
      echo "Sentinel present: $file"
      return 0
    fi
    sleep 1
    elapsed=$((elapsed+1))
  done
  echo "ERROR: Timeout waiting for sentinel $file" >&2
  return 1
}

# Relay20 on 127.0.0.1:20020
RELAY_A_HANDLE=relay20
RELAY_A_ADDRESS=J3GHIF4QBJT7PEQHJ7YNJXP64RY7Q27GRB6HEFJ7O5E6JULGNSPVP546N4
RELAY_A_PASSPHRASE="parent diamond bring another suggest rice diamond gravity bench violin hover fat relax annual repeat keen use moon senior display laundry asthma trend absorb grab"
RELAY_A_PORT=20020

# Relay21 on 127.0.0.1:20021
RELAY_B_HANDLE=relay21
RELAY_B_ADDRESS=ZEBF7TPP3ZKVPBUSXDRZE2XIRLBRBYFQF6PFEXXLTFMOP6ETX3HFKG7D6Y
RELAY_B_PASSPHRASE="design coast gift sting park tooth comic load off feed super close civil divide orbit garden mutual boat wine analyst gospel stem pipe about ritual"
RELAY_B_PORT=20021

# Extra Relay on dynamic IP
RELAY_EXTRA_HANDLE=relayextra
RELAY_EXTRA_ADDRESS=3RLYTSRX54G5WOPPPV4FYWRV2QXKIC5WRPM54YKXGVLTAFGUEIG2QN4DMQ
RELAY_EXTRA_PASSPHRASE="horror stuff huge crunch green marriage parent soon hamster tonight miracle company fee cup hard media shiver emotion hybrid shiver main cube lemon about obvious";
RELAY_EXTRA_PORT=20022

TESTNET_USER=testuser10
TESTNET_ADDRESS=YA2UAJPUJZBY4KR2B4FBM57NSA7252PJQTVKJEGB2MOISRUECW4JGE4USM
TESTNET_PASSPHRASE="glide crawl soda hole assault tide fault century seed tip daughter student rice swap imitate setup like card reject claim truck squeeze same able remind"

# Sync with ping_registered_node.rs when changing
PINGABLE_USER=pinguser21
PINGABLE_ADDRESS=EK2KRWCCCI4DRMSQIDYAING2NURDMDBVWDK6VCCDGQNBQ5DMGFPKRTAFGY
PINGABLE_PASSPHRASE="scare much guide patch report explain collect feel climb mansion cluster child muscle split jewel crush wisdom length merry diary quote axis foil abstract escape"
PINGABLE_PORT=30001

# NAT mode for tests. Accept Direct|Full|Restricted|All (default All)
NAT_MODE=${NAT_MODE:-All}
MEASURE_MEMORY=${MEASURE_MEMORY:-0}

# Subnet prefix for the dedicated bingle_testnet docker network. The pingable
# containers are pinned to static IPs in <prefix>.0.100+, so the network subnet and
# those IPs both derive from this single prefix. Default avoids 172.17/172.18, which
# docker's default bridge and algokit localnet (algokit_sandbox_default) commonly use
# and which caused "Pool overlaps with other one on this address space". Override with
# TESTNET_SUBNET_PREFIX if this one clashes too (must be a two-octet /16 prefix).
TESTNET_SUBNET_PREFIX="${TESTNET_SUBNET_PREFIX:-172.28}"
TESTNET_SUBNET="${TESTNET_SUBNET_PREFIX}.0.0/16"

# Common docker run flags
COMMON_ARGS=()
if [[ "$MEASURE_MEMORY" == "1" ]]; then
  COMMON_ARGS+=("-e" "MEASURE_MEMORY=1")
fi

# Determine initial NAT mode for the pingable container
if [ "$NAT_MODE" = "All" ] || [ "$NAT_MODE" = "all" ]; then
  PING_INIT_MODE="Direct"
else
  PING_INIT_MODE="$NAT_MODE"
fi

# Ensure the persistent test accounts are opted into the current app and have their
# handles registered. Required after an app redeploy: the usersettings grants below and
# the e2e tests both assume these accounts already hold local state on the app the node
# file points at (a fresh app leaves them opted into the previous app only). Idempotent —
# accounts already registered are skipped. EXTRA_RELAY is honoured (relayextra included).
scripts/bootstrap_testnet_accounts.sh --node-file nodely_staging_testnet_node.json
if [ $? -ne 0 ]; then
  echo "ERROR: testnet account bootstrap failed" >&2
  exit 1
fi

# Enable relay + static-IP permission on-chain for the root relay accounts.
# Latest bingle_admin: the old `root <ID> --enable --passphrase <mnemonic>` form is now
# `usersettings <ID> --enable-relay --enable-static --accounts <DIR>` (APP_ADMIN signs).
# Both permissions are required: relays register a static endpoint (allow_static) and now
# refuse to start unless allow_relay is set (see check_allow_relay in bingle_core).
if [[ ! -f "$TESTNET_ACCOUNTS_DIR/APP_ADMIN.json" ]]; then
  echo "ERROR: accounts directory '$TESTNET_ACCOUNTS_DIR' is missing APP_ADMIN.json" >&2
  echo "       set TESTNET_ACCOUNTS_DIR to the staging testnet account set" >&2
  exit 1
fi

for RELAY_ADDR in "$RELAY_A_ADDRESS" "$RELAY_B_ADDRESS"; do
  bingle_admin usersettings "$RELAY_ADDR" --enable-relay --enable-static \
    --accounts "$ACCOUNTS_DIR" \
    --node-file nodely_staging_testnet_node.json
  if [ $? -ne 0 ]; then
    echo "ERROR: bingle_admin usersettings failed for relay $RELAY_ADDR" >&2
    exit 1
  fi
done

# Ensure a dedicated test network exists with the fixed subnet used for --ip assignment.
if docker network inspect bingle_testnet >/dev/null 2>&1; then
  : # already present, reuse it
elif docker network create --subnet="$TESTNET_SUBNET" bingle_testnet >/dev/null 2>&1; then
  echo "Created docker network bingle_testnet ($TESTNET_SUBNET)"
else
  # Create failed — almost always because $TESTNET_SUBNET overlaps an existing network
  # (docker's default bridge, algokit localnet, a compose network, etc.). Identify the
  # squatter so the fix is obvious instead of surfacing the raw daemon message.
  owner=""
  for id in $(docker network ls -q); do
    if docker network inspect "$id" \
         --format '{{range .IPAM.Config}}{{.Subnet}}{{end}}' 2>/dev/null \
         | grep -q "^${TESTNET_SUBNET}$"; then
      owner=$(docker network inspect "$id" --format '{{.Name}}' 2>/dev/null)
      break
    fi
  done
  if [ -n "$owner" ]; then
    echo "ERROR: subnet $TESTNET_SUBNET is already used by docker network '$owner'." >&2
    echo "       Free it (e.g. 'docker network rm $owner') or set TESTNET_SUBNET_PREFIX" >&2
    echo "       to an unused two-octet /16 prefix, then re-run." >&2
  else
    echo "ERROR: failed to create docker network bingle_testnet ($TESTNET_SUBNET)" >&2
  fi
  exit 1
fi

# Start two local STUN servers in Docker (coturn in STUN-only mode)
docker rm -f bingle_stun_a bingle_stun_b >/dev/null 2>&1 || true

docker run --rm -d \
  --name bingle_stun_a \
  --network bingle_testnet \
  instrumentisto/coturn \
  turnserver -n --no-tls --no-dtls --listening-port 3478 --fingerprint --lt-cred-mech=0 --max-bps=0 --min-port=49152 --max-port=49200

if [ $? -ne 0 ]; then
    echo "ERROR: Failed to start bingle_stun_a container" >&2
    exit 1
fi

docker run --rm -d \
  --name bingle_stun_b \
  --network bingle_testnet \
  instrumentisto/coturn \
  turnserver -n --no-tls --no-dtls --listening-port 3478 --fingerprint --lt-cred-mech=0 --max-bps=0 --min-port=49152 --max-port=49200

if [ $? -ne 0 ]; then
    echo "ERROR: Failed to start bingle_stun_b container" >&2
    exit 1
fi

# Fetch the internal Docker IPs of the STUN servers
STUN_A_IP=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' bingle_stun_a)
STUN_B_IP=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' bingle_stun_b)
if [[ -z "$STUN_A_IP" || -z "$STUN_B_IP" ]]; then
  echo "Failed to discover STUN container IPs" >&2
  exit 2
fi

# Prepare a stunservers.txt that references the containers by their internal Docker IPs
# Prepare output directory to collect results from the containers
mkdir -p tmp tmp/sentinels tmp/test_out

# File paths for collecting results from each docker run
MAIN_OUT="$PWD/tmp/test_out/test_results_main.out"
PING_OUT="$PWD/tmp/test_out/test_results_ping.out"
RELAY_A_OUT="$PWD/tmp/test_out/relay_a.out"
RELAY_B_OUT="$PWD/tmp/test_out/relay_b.out"
RELAY_EXTRA_OUT="$PWD/tmp/test_out/relay_extra.out"
PINGABLE_OUT="$PWD/tmp/test_out/pingable.out"

# Ensure previous outputs are cleared
: > "$MAIN_OUT"
: > "$PING_OUT"
: > "$RELAY_A_OUT"
: > "$RELAY_B_OUT"
: > "$RELAY_EXTRA_OUT"
: > "$PINGABLE_OUT"

cat > tmp/stunservers.txt <<EOF
# Auto-generated by run_testnet_tests.sh
${STUN_A_IP}:3478
${STUN_B_IP}:3478
EOF

echo "Using STUN servers: ${STUN_A_IP}:3478, ${STUN_B_IP}:3478"

# Sentinel directory and file names
SENT_DIR="$PWD/tmp/sentinels"
RELAY_A_SENT="relay_${RELAY_A_HANDLE}_${RELAY_A_PORT}.sentinel"
RELAY_B_SENT="relay_${RELAY_B_HANDLE}_${RELAY_B_PORT}.sentinel"
RELAY_EXTRA_SENT="relay_${RELAY_EXTRA_HANDLE}_${RELAY_EXTRA_PORT}.sentinel"

# Start relay containers on the same docker network
# They will autodetect EXTERNAL_IP inside the container and register static endpoints
# reachable from other containers on this network.
# Remove any old sentinel files
rm -f "$SENT_DIR/$RELAY_A_SENT" "$SENT_DIR/$RELAY_B_SENT" "$SENT_DIR/$RELAY_EXTRA_SENT"

# Enable debug logging for relays to help diagnose connectivity issues
RELAY_EXTRA_ARGS="--log-debug"

docker run --platform linux/arm64 -m 512m --memory-swap 512m "${COMMON_ARGS[@]}" "${DOCKER_RUN_RM[@]}" -d \
 --name bingle_relay_a \
 --network bingle_testnet \
 -e RELAY=1 \
 -e EXTRA_ARGS="$RELAY_EXTRA_ARGS" \
 -e PASSPHRASE="$RELAY_A_PASSPHRASE" \
 -e PORT=$RELAY_A_PORT \
 -e HANDLE=$RELAY_A_HANDLE \
 -e SENTINEL_FILE="/sentinels/$RELAY_A_SENT" \
 -e OUT_FILE="/out/relay_a.out" \
 -v "$PWD/tmp/sentinels":/sentinels \
 -v "$PWD/tmp/test_out":/out \
 "bingle:local"

if [ $? -ne 0 ]; then
    echo "ERROR: Failed to start bingle_relay_a container" >&2
    exit 1
fi

docker run --platform linux/arm64 -m 512m --memory-swap 512m "${COMMON_ARGS[@]}" "${DOCKER_RUN_RM[@]}" -d \
 --name bingle_relay_b \
 --network bingle_testnet \
 -e RELAY=1 \
 -e EXTRA_ARGS="$RELAY_EXTRA_ARGS" \
 -e PASSPHRASE="$RELAY_B_PASSPHRASE" \
 -e PORT=$RELAY_B_PORT \
 -e HANDLE=$RELAY_B_HANDLE \
 -e SENTINEL_FILE="/sentinels/$RELAY_B_SENT" \
 -e OUT_FILE="/out/relay_b.out" \
 -v "$PWD/tmp/sentinels":/sentinels \
 -v "$PWD/tmp/test_out":/out \
 "bingle:local"

if [ $? -ne 0 ]; then
    echo "ERROR: Failed to start bingle_relay_b container" >&2
    exit 1
fi

# Wait for both relay sentinels before continuing and starting the extra relay
# if required (needs the root relays to be up)
wait_for_file "$SENT_DIR/$RELAY_A_SENT" 180 || exit 1
wait_for_file "$SENT_DIR/$RELAY_B_SENT" 180 || exit 1

if [[ -n "${EXTRA_RELAY:-}" ]]; then
  # The extra relay also runs with --relay, so it needs allow_relay set on-chain. It is
  # STUN_ONLY and registers no static endpoint, so static permission is not required.
  bingle_admin usersettings "$RELAY_EXTRA_ADDRESS" --enable-relay \
    --accounts "$ACCOUNTS_DIR" \
    --node-file nodely_staging_testnet_node.json
  if [ $? -ne 0 ]; then
    echo "ERROR: bingle_admin usersettings failed for extra relay $RELAY_EXTRA_ADDRESS" >&2
    exit 1
  fi

  docker run --platform linux/arm64 "${COMMON_ARGS[@]}" "${DOCKER_RUN_RM[@]}"  -d \
   --name bingle_relay_extra \
   --network bingle_testnet \
   -e RELAY=1 \
   -e STUN_ONLY=1 \
   -e EXTRA_ARGS="$RELAY_EXTRA_ARGS" \
   -e PASSPHRASE="$RELAY_EXTRA_PASSPHRASE" \
   -e PORT=$RELAY_EXTRA_PORT \
   -e HANDLE=$RELAY_EXTRA_HANDLE \
   -e SENTINEL_FILE="/sentinels/$RELAY_EXTRA_SENT" \
   -e OUT_FILE="/out/relay_extra.out" \
   -v "$PWD/tmp/sentinels":/sentinels \
   -v "$PWD/tmp/test_out":/out \
   -v "$PWD/tmp/stunservers.txt":/app/stunservers.txt:ro \
   "bingle:local"

  if [ $? -ne 0 ]; then
      echo "ERROR: Failed to start bingle_relay_extra container" >&2
      exit 1
  fi
fi

if [[ -n "${EXTRA_RELAY:-}" ]]; then
  wait_for_file "$SENT_DIR/$RELAY_EXTRA_SENT" 180 || exit 1
fi

# Start the ping target
echo "Restarting ping target mode ${PING_INIT_MODE}"
# Initialize IP address tracking for bingle_pingable container
PINGABLE_IP_SUFFIX=100
echo "Using IP address ${TESTNET_SUBNET_PREFIX}.0.$PINGABLE_IP_SUFFIX for initial bingle_pingable"
PING_INIT_SENT="pingable_${PING_INIT_MODE}_${PINGABLE_PORT}.sentinel"
echo "Delete sentinel ${SENT_DIR}/${PING_INIT_SENT}"
rm -f "$SENT_DIR/$PING_INIT_SENT"

# Enable verbose logging for the ping target to help diagnose failures
PING_EXTRA_ARGS="--log-debug"

docker run --platform linux/arm64 "${COMMON_ARGS[@]}" "${DOCKER_RUN_RM[@]}"  -d \
 --name bingle_pingable \
 --network bingle_testnet \
 --ip "${TESTNET_SUBNET_PREFIX}.0.$PINGABLE_IP_SUFFIX" \
 --cap-add NET_ADMIN \
 -e RUST_BACKTRACE=1 \
 -e EXTRA_ARGS="$PING_EXTRA_ARGS" \
 -e PASSPHRASE="$PINGABLE_PASSPHRASE" \
 -e PORT=$PINGABLE_PORT \
 -e HANDLE=$PINGABLE_USER \
 -e NAT_MODE="$PING_INIT_MODE" \
 -e SENTINEL_FILE="/sentinels/$PING_INIT_SENT" \
 -e OUT_FILE="/out/pingable.out" \
 -v "$PWD/tmp/sentinels":/sentinels \
 -v "$PWD/tmp/test_out":/out \
 -v "$PWD/tmp/stunservers.txt":/app/stunservers.txt:ro \
 "bingle:local"

if [ $? -ne 0 ]; then
    echo "ERROR: Failed to start bingle_pingable container" >&2
    exit 1
fi

# Wait for pingable listening sentinel
echo "Waiting for ${SENT_DIR}/${PING_INIT_SENT}"
wait_for_file "$SENT_DIR/$PING_INIT_SENT" 180 || exit 1
echo "Ping target restarted"

# Ensure the latest application and tests images are built from the current workspace
export BINGLE_RUN_TESTNET=1
export TESTNET_USER=$TESTNET_USER
export TESTNET_PASSPHRASE="$TESTNET_PASSPHRASE"

scripts/build_cli_image.sh --tag bingle:local
if [ $? -ne 0 ]; then
  echo "ERROR: failed to build bingle:local image" >&2
  exit 1
fi
scripts/build_tests_image.sh --tag bingle-tests:local
if [ $? -ne 0 ]; then
  echo "ERROR: failed to build bingle-tests:local image" >&2
  exit 1
fi

# Run the prebuilt test inside the dedicated tests image
# NAT_MODE can be set by the caller to control iptables behavior in the test container: Direct|Full|Restricted|All
# Default to All to exercise all three modes in sequence with a final summary
NAT_MODE=${NAT_MODE:-All}
# RUN_TESTS can be set by the caller to control which test groups run:
#   All  -> run main + ping (default)
#   Init -> run only the main init test
#   Ping -> run only the ping tests
RUN_TESTS=${RUN_TESTS:-All}

MAIN_RC=0
PING_RC=0
PING_MODES=()

if [[ "$RUN_TESTS" == "All" || "$RUN_TESTS" == "all" || "$RUN_TESTS" == "Init" || "$RUN_TESTS" == "init" ]]; then
  docker run --platform linux/arm64 -t "${COMMON_ARGS[@]}" "${DOCKER_RUN_RM[@]}" \
    --name bingle_test_runner \
    --network bingle_testnet \
    --cap-add NET_ADMIN \
    -e NAT_MODE="$NAT_MODE" \
    -e TESTNET_USER="$TESTNET_USER" \
    -e TESTNET_PASSPHRASE="$TESTNET_PASSPHRASE" \
    -e TEST_FILTER="testnet_user_reaches_endpoint_available" \
    -e OUT_FILE="/out/$(basename "$MAIN_OUT")" \
    -v "$PWD/tmp/test_out":/out \
    -v "$PWD/tmp/stunservers.txt":/app/stunservers.txt:ro \
    "bingle-tests:local"
  MAIN_RC=$?

  if [ $MAIN_RC -ne 0 ]; then
      echo "ERROR: Main test container failed with exit code $MAIN_RC" >&2
      # Don't exit here - we want to collect results and run the ping test
  fi
fi

if [[ "$RUN_TESTS" == "All" || "$RUN_TESTS" == "all" || "$RUN_TESTS" == "Ping" || "$RUN_TESTS" == "ping" ]]; then
  # Build image for ping_registered_node test once
  scripts/build_tests_image.sh --tag bingle-tests:ping --test ping_registered_node
  if [ $? -ne 0 ]; then
    echo "ERROR: failed to build bingle-tests:ping image; skipping ping tests" >&2
    exit 1
  fi

  # Set explicit filter to the ping test function
  PING_FILTER="testnet_send_ping_to_registered_node"

  # Decide which pingable NAT modes to exercise
  if [ "$NAT_MODE" = "All" ] || [ "$NAT_MODE" = "all" ]; then
    PING_MODES=("Direct" "Full" "Restricted")
  else
    PING_MODES=("$PING_INIT_MODE")
  fi

  PING_OUT_BASE="$PWD/tmp/test_out/test_results_ping"
  PING_ANY_FAIL=0
  FIRST_MODE=1

  for MODE in "${PING_MODES[@]}"; do
    # Restart pingable for subsequent modes
    if [ $FIRST_MODE -eq 0 ]; then
      echo "Restarting bingle_pingable with NAT_MODE=$MODE..."
      docker stop --time 30 bingle_pingable >/dev/null 2>&1 || true
      # Wait for container to be fully stopped and removed
      timeout=10
      elapsed=0
      while docker ps -a --format '{{.Names}}' | grep -q "^bingle_pingable$" && [ $elapsed -lt $timeout ]; do
        sleep 0.5
        elapsed=$((elapsed + 1))
      done
      # Force remove if still exists
      docker rm -f bingle_pingable >/dev/null 2>&1 || true
      # Increment IP address for new container
      PINGABLE_IP_SUFFIX=$((PINGABLE_IP_SUFFIX + 1))
      echo "Using IP address ${TESTNET_SUBNET_PREFIX}.0.$PINGABLE_IP_SUFFIX for bingle_pingable restart"
      PING_SENT="pingable_${MODE}_${PINGABLE_PORT}.sentinel"
      echo "Removing $SENT_DIR/$PING_SENT"
      rm -f "$SENT_DIR/$PING_SENT"
      docker run --platform linux/arm64 "${COMMON_ARGS[@]}" "${DOCKER_RUN_RM[@]}" -d \
        --name bingle_pingable \
        --network bingle_testnet \
        --ip "${TESTNET_SUBNET_PREFIX}.0.$PINGABLE_IP_SUFFIX" \
        --cap-add NET_ADMIN \
        -e RUST_BACKTRACE=1 \
        -e EXTRA_ARGS="$PING_EXTRA_ARGS" \
        -e PASSPHRASE="$PINGABLE_PASSPHRASE" \
        -e PORT=$PINGABLE_PORT \
        -e HANDLE=$PINGABLE_USER \
        -e NAT_MODE="$MODE" \
        -e SENTINEL_FILE="/sentinels/$PING_SENT" \
        -e OUT_FILE="/out/pingable.out" \
        -v "$PWD/tmp/sentinels":/sentinels \
        -v "$PWD/tmp/test_out":/out \
        -v "$PWD/tmp/stunservers.txt":/app/stunservers.txt:ro \
        "bingle:local"
      if [ $? -ne 0 ]; then
        echo "ERROR: Failed to start bingle_pingable container for mode $MODE" >&2
        PING_ANY_FAIL=1
        continue
      fi
      # Wait for per-mode pingable listening sentinel
      echo "Waiting for $SENT_DIR/$PING_SENT"
      if ! wait_for_file "$SENT_DIR/$PING_SENT" 240; then
        echo "ERROR: Pingable did not signal listening for mode $MODE within timeout" >&2
        PING_ANY_FAIL=1
        continue
      fi
    fi

    # Run the prebuilt test inside the dedicated tests image (streams output and waits for completion)
    # NAT_MODE can be set by the caller to control iptables behavior in the test container: Direct|Full|Restricted|All
    # Default to All to exercise all three modes in sequence with a final summary
    # (Nat mode for the caller is always Direct, the nat mode for the target changes)
    OUT_FILE_MODE="/out/$(basename "${PING_OUT_BASE}_${MODE}.out")"
    echo "Starting ping test for mode $MODE (Output: $OUT_FILE_MODE)..."
    docker run --platform linux/arm64 -t "${COMMON_ARGS[@]}" "${DOCKER_RUN_RM[@]}" \
      --name bingle_test_runner_ping \
      --network bingle_testnet \
      --cap-add NET_ADMIN \
      -e NAT_MODE="Direct" \
      -e TEST_FILTER="$PING_FILTER" \
      -e TESTNET_USER="$TESTNET_USER" \
      -e TESTNET_PASSPHRASE="$TESTNET_PASSPHRASE" \
      -e PINGABLE_USER="$PINGABLE_USER" \
      -e PINGABLE_ADDRESS="$PINGABLE_ADDRESS" \
      -e OUT_FILE="$OUT_FILE_MODE" \
      -v "$PWD/tmp/test_out":/out \
      -v "$PWD/tmp/stunservers.txt":/app/stunservers.txt:ro \
      "bingle-tests:ping"
    rc=$?
    if [ $rc -ne 0 ]; then
      echo "ERROR: Ping test container failed in mode $MODE with exit code $rc" >&2
      PING_ANY_FAIL=1
    fi
    # Append a clear per-mode result line to the host file
    host_file="${PING_OUT_BASE}_${MODE}.out"
    if [ $rc -eq 0 ]; then
      echo "[runner][$MODE] Test PASSED" >> "$host_file"
    else
      echo "[runner][$MODE] Test FAILED with exit code $rc" >> "$host_file"
    fi

    FIRST_MODE=0
    # Ensure the runner container is not lingering
    docker stop --time 30 bingle_test_runner_ping >/dev/null 2>&1 || true

  done

  # Combine all per-mode ping outputs into the canonical PING_OUT file
  : > "$PING_OUT"
  for MODE in "${PING_MODES[@]}"; do
    host_file="${PING_OUT_BASE}_${MODE}.out"
    if [ -f "$host_file" ]; then
      echo "" >> "$PING_OUT"
      echo "==== Ping mode $MODE ====" >> "$PING_OUT"
      cat "$host_file" >> "$PING_OUT"
    fi
  done

  # Set overall PING_RC for later summary
  if [ $PING_ANY_FAIL -ne 0 ]; then
    PING_RC=1
  else
      PING_RC=0
    fi
  fi

# Show individual results from the main run
if [[ -f "$MAIN_OUT" ]]; then
  echo "-- Main run (per-mode results) --"
  grep -E '^\[runner\]\[(Direct|Full|Restricted)\] Test ' "$MAIN_OUT" || echo "(no per-mode result lines found)"
  echo "-- Main run (summary) --"
  grep -E '^\[runner\]\[summary\]' "$MAIN_OUT" || echo "(no summary lines found)"
else
  echo "[WARN] Main results file not found: $MAIN_OUT"
fi

# Show results from the ping run
if [[ -f "$PING_OUT" ]]; then
  echo "-- Ping run (per-mode results) --"
  grep -E '^\[runner\]\[(Direct|Full|Restricted)\] Test ' "$PING_OUT" || echo "(no per-mode result lines found)"
else
  echo "[WARN] Ping results file not found: $PING_OUT"
fi

# Determine statuses
MAIN_STATUS="UNKNOWN"
PING_STATUS="UNKNOWN"

if [[ -f "$MAIN_OUT" ]]; then
  if grep -Eq '^\[runner\]\[summary\].*FAILED' "$MAIN_OUT"; then
    MAIN_STATUS="FAIL"
  elif grep -Eq '^\[runner\]\[summary\].*PASSED' "$MAIN_OUT"; then
    # If any PASSED summaries found and no FAILED, consider PASS
    MAIN_STATUS="PASS"
  else
    # Fall back to exit code
    if [[ ${MAIN_RC:-1} -eq 0 ]]; then MAIN_STATUS="PASS"; else MAIN_STATUS="FAIL"; fi
  fi
else
  if [[ ${MAIN_RC:-1} -eq 0 ]]; then MAIN_STATUS="PASS"; else MAIN_STATUS="FAIL"; fi
fi

if [[ -f "$PING_OUT" ]]; then
  # If any mode failed, overall ping run is FAIL
  if grep -Eq '^\[runner\]\[(Direct|Full|Restricted)\] Test FAILED' "$PING_OUT"; then
    PING_STATUS="FAIL"
  # Otherwise, if at least one mode passed and none failed, it's a PASS
  elif grep -Eq '^\[runner\]\[(Direct|Full|Restricted)\] Test PASSED' "$PING_OUT"; then
    PING_STATUS="PASS"
  else
    # Fall back to aggregated RC from per-mode runs
    if [[ ${PING_RC:-1} -eq 0 ]]; then PING_STATUS="PASS"; else PING_STATUS="FAIL"; fi
  fi
else
  if [[ ${PING_RC:-1} -eq 0 ]]; then PING_STATUS="PASS"; else PING_STATUS="FAIL"; fi
fi

# If measuring memory, stop background containers now to collect their final reports
if [[ "$MEASURE_MEMORY" == "1" ]]; then
  echo "Stopping background containers to collect memory reports..."
  docker stop --time 30 bingle_relay_a bingle_relay_b bingle_relay_extra bingle_pingable >/dev/null 2>&1 || true
fi

# Extract and display peak memory information
if [[ "$MEASURE_MEMORY" == "1" ]]; then
  echo "-- Peak Memory Usage --"

  report_file() {
    local label="$1"
    local file="$2"
    if [[ -f "$file" ]]; then
      local line
      line=$(grep "Peak memory usage" "$file" | tail -n 1) || true
      if [[ -n "$line" ]]; then
        # Extract the value part. Example: "50.00 MB (52428800 bytes)"
        local val
        val=$(echo "$line" | sed 's/.*Peak memory usage: //; s/ \[.*//')
        echo "$label $val"
      else
        echo "$label (no report found)"
      fi
    else
      echo "$label (missing)"
    fi
  }

  report_file "Main run (test runner):" "$MAIN_OUT"

  if [[ -n "${PING_MODES[*]:-}" ]]; then
    for MODE in "${PING_MODES[@]}"; do
      report_file "Ping run (test runner) [NAT=$MODE]:" "${PING_OUT_BASE}_${MODE}.out"
    done
  fi

  report_file "Relay A:" "$RELAY_A_OUT"
  report_file "Relay B:" "$RELAY_B_OUT"

  if [[ -n "${EXTRA_RELAY:-}" ]]; then
    report_file "Relay Extra:" "$RELAY_EXTRA_OUT"
  fi

  report_file "Pingable:" "$PINGABLE_OUT"
fi

# Timings
echo "-- Timing Summary --"
if [[ -f "$MAIN_OUT" ]]; then
  echo "Main run timings (NAT_MODE=$NAT_MODE):"
  # Prefix each timing line with the NAT mode used by the main test container
  grep "TIMING:" "$MAIN_OUT" | sed "s/^/  [NAT=$NAT_MODE] /" || echo "  (no timing info found)"
fi

# For ping run, show timings per NAT mode exercised (if any)
echo "Ping run timings:"
if [[ -n "${PING_MODES[*]:-}" ]]; then
  any_timing=0
  for MODE in "${PING_MODES[@]}"; do
    host_file="${PING_OUT_BASE}_${MODE}.out"
    if [[ -f "$host_file" ]]; then
      # Prefix per-mode timing lines with the NAT mode
      if grep -q "TIMING:" "$host_file"; then
        grep "TIMING:" "$host_file" | sed "s/^/  [NAT=$MODE] /"
        any_timing=1
      fi
    fi
  done
  if [[ $any_timing -eq 0 ]]; then
    echo "  (no timing info found)"
  fi
else
  if [[ -f "$PING_OUT" ]]; then
    # Fallback: prefix timings found in combined file without per-mode context
    grep "TIMING:" "$PING_OUT" | sed 's/^/  /' || echo "  (no timing info found)"
  else
    echo "  (no timing info found)"
  fi
fi

echo "-- Overall --"
echo "Main run: $MAIN_STATUS"
echo "Ping run: $PING_STATUS"

if [[ "$RUN_TESTS" == "Init" || "$RUN_TESTS" == "init" ]]; then
  if [[ "$MAIN_STATUS" == "PASS" ]]; then
    echo "OVERALL RESULT: PASS"
    exit 0
  else
    echo "OVERALL RESULT: FAIL"
    if [[ ${MAIN_RC:-0} -ne 0 ]]; then exit ${MAIN_RC:-1}; fi
    exit 1
  fi
elif [[ "$RUN_TESTS" == "Ping" || "$RUN_TESTS" == "ping" ]]; then
  if [[ "$PING_STATUS" == "PASS" ]]; then
    echo "OVERALL RESULT: PASS"
    exit 0
  else
    echo "OVERALL RESULT: FAIL"
    if [[ ${PING_RC:-0} -ne 0 ]]; then exit ${PING_RC:-1}; fi
    exit 1
  fi
else
  if [[ "$MAIN_STATUS" == "PASS" && "$PING_STATUS" == "PASS" ]]; then
    echo "OVERALL RESULT: PASS"
    exit 0
  else
    echo "OVERALL RESULT: FAIL"
  fi

  # Prefer returning the first non-zero original RC if available
  if [[ ${MAIN_RC:-0} -ne 0 ]]; then exit ${MAIN_RC:-1}; fi
  if [[ ${PING_RC:-0} -ne 0 ]]; then exit ${PING_RC:-1}; fi
  exit 1
fi
