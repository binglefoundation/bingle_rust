#!/usr/bin/env bash
# run_webserver_testnet.sh
# Purpose: Start relays, a pingable target, and a bingle_webserver in Docker for testnet verification.

set -euo pipefail

CREATOR_PASSPHRASE="version rural bring cushion ball case borrow present avoid else pupil alcohol marine attitude extra favorite mass move midnight symbol sibling latin language able borrow"

# Ensure cleanup of background containers on exit
cleanup() {
  local exit_code=$?
  # echo "Stopping containers..."
  # docker stop bingle_relay_a bingle_relay_b bingle_stun_a bingle_stun_b bingle_pingable >/dev/null 2>&1 || true
  # docker stop bingle_webserver
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

# Configuration from run_testnet_tests.sh
RELAY_A_HANDLE=relay20
RELAY_A_ADDRESS=J3GHIF4QBJT7PEQHJ7YNJXP64RY7Q27GRB6HEFJ7O5E6JULGNSPVP546N4
RELAY_A_PASSPHRASE="parent diamond bring another suggest rice diamond gravity bench violin hover fat relax annual repeat keen use moon senior display laundry asthma trend absorb grab"
RELAY_A_PORT=20020

RELAY_B_HANDLE=relay21
RELAY_B_ADDRESS=ZEBF7TPP3ZKVPBUSXDRZE2XIRLBRBYFQF6PFEXXLTFMOP6ETX3HFKG7D6Y
RELAY_B_PASSPHRASE="design coast gift sting park tooth comic load off feed super close civil divide orbit garden mutual boat wine analyst gospel stem pipe about ritual"
RELAY_B_PORT=20021

TESTNET_USER=testuser10
TESTNET_ADDRESS=YA2UAJPUJZBY4KR2B4FBM57NSA7252PJQTVKJEGB2MOISRUECW4JGE4USM
TESTNET_PASSPHRASE="glide crawl soda hole assault tide fault century seed tip daughter student rice swap imitate setup like card reject claim truck squeeze same able remind"

PINGABLE_USER=pinguser20
PINGABLE_ADDRESS=QASXBML72DKIJEJ5GLMEBBX33KCKW3TSJW7ETFOTLEREQCDMW5BXCLXSQU
PINGABLE_PASSPHRASE="group avocado audit dentist baby index pipe attack enough stairs fame position column media copper athlete resource noodle forward wage middle into fitness ability dragon"
PINGABLE_PORT=30001

# 1) Admin setup: Ensure users are opted in and enabled on testnet
echo "Updating testnet user states via bingle_admin..."
bingle_admin root $RELAY_A_ADDRESS --enable --node-file nodely_testnet_node.json --passphrase "$CREATOR_PASSPHRASE"
bingle_admin root $RELAY_B_ADDRESS --enable --node-file nodely_testnet_node.json --passphrase "$CREATOR_PASSPHRASE"
bingle_admin updateuser --handle $TESTNET_USER --passphrase "$CREATOR_PASSPHRASE" --node-file nodely_testnet_node.json --userpassphrase "$TESTNET_PASSPHRASE"
bingle_admin updateuser --handle $PINGABLE_USER --passphrase "$CREATOR_PASSPHRASE" --node-file nodely_testnet_node.json --userpassphrase "$PINGABLE_PASSPHRASE"

# 2) Setup Docker network
if ! docker network inspect bingle_testnet >/dev/null 2>&1; then
  docker network create --subnet=172.18.0.0/16 bingle_testnet >/dev/null
fi

# 3) Start STUN servers
docker rm -f bingle_stun_a bingle_stun_b >/dev/null 2>&1 || true
docker run --rm -d --name bingle_stun_a --network bingle_testnet instrumentisto/coturn turnserver -n --no-tls --no-dtls --listening-port 3478 --fingerprint --lt-cred-mech=0 --max-bps=0 --min-port=49152 --max-port=49200
docker run --rm -d --name bingle_stun_b --network bingle_testnet instrumentisto/coturn turnserver -n --no-tls --no-dtls --listening-port 3478 --fingerprint --lt-cred-mech=0 --max-bps=0 --min-port=49152 --max-port=49200

STUN_A_IP=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' bingle_stun_a)
STUN_B_IP=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' bingle_stun_b)

mkdir -p tmp tmp/sentinels
cat > tmp/stunservers.txt <<EOF
${STUN_A_IP}:3478
${STUN_B_IP}:3478
EOF

SENT_DIR="$PWD/tmp/sentinels"
rm -f "$SENT_DIR"/*.sentinel

# 4) Build images
echo "Building CLI and Webserver images..."
scripts/build_cli_image.sh
scripts/build_webserver_image.sh

# 5) Start Relays
echo "Starting relays..."
docker run --platform linux/arm64 --rm -d --name bingle_relay_a --network bingle_testnet \
 -e RELAY=1 -e PASSPHRASE="$RELAY_A_PASSPHRASE" -e PORT=$RELAY_A_PORT -e HANDLE=$RELAY_A_HANDLE \
 -e SENTINEL_FILE="/sentinels/relay_a.sentinel" -v "$SENT_DIR":/sentinels "bingle:local"

docker run --platform linux/arm64 --rm -d --name bingle_relay_b --network bingle_testnet \
 -e RELAY=1 -e PASSPHRASE="$RELAY_B_PASSPHRASE" -e PORT=$RELAY_B_PORT -e HANDLE=$RELAY_B_HANDLE \
 -e SENTINEL_FILE="/sentinels/relay_b.sentinel" -v "$SENT_DIR":/sentinels "bingle:local"

wait_for_file "$SENT_DIR/relay_a.sentinel" 180
wait_for_file "$SENT_DIR/relay_b.sentinel" 180

# 5) Start Pingable Target
echo "Starting pingable target..."
docker run --platform linux/arm64 --rm -d --name bingle_pingable --network bingle_testnet \
 --ip "172.18.0.100" --cap-add NET_ADMIN -e PASSPHRASE="$PINGABLE_PASSPHRASE" -e PORT=$PINGABLE_PORT \
 -e HANDLE=$PINGABLE_USER -e NAT_MODE="Direct" -e SENTINEL_FILE="/sentinels/pingable.sentinel" \
 -v "$SENT_DIR":/sentinels -v "$PWD/tmp/stunservers.txt":/app/stunservers.txt:ro "bingle:local"

wait_for_file "$SENT_DIR/pingable.sentinel" 180

# 7) Start Webserver
echo "Starting bingle_webserver as $TESTNET_USER..."
docker run --platform linux/arm64 -d --name bingle_webserver --network bingle_testnet \
 -p 12121:12121 -e HANDLE=$TESTNET_USER -e PASSPHRASE="$TESTNET_PASSPHRASE" \
 -e RUST_BACKTRACE=1 \
 -v "$PWD/tmp/stunservers.txt":/app/stunservers.txt:ro "bingle-webserver:local"

# Wait a few seconds for webserver to initialize
sleep 10

# 8) Verification
echo "Verifying webserver..."
EXPECTED_BUILD=$(cat .build_number)
ACTUAL_VERSION_JSON=$(curl -s "http://localhost:12121/version" || true)
ACTUAL_BUILD=$(echo "$ACTUAL_VERSION_JSON" | jq -r '.buildNumber' 2>/dev/null || echo "unknown")

if [ "$ACTUAL_BUILD" == "$EXPECTED_BUILD" ]; then
  echo "Verification PASSED: Webserver version confirmed. Build number: $ACTUAL_BUILD"
else
  echo "Verification FAILED: Build number mismatch or could not reach webserver."
  echo "Expected build: $EXPECTED_BUILD"
  echo "Actual build:   $ACTUAL_BUILD"
  echo "Full version info: $ACTUAL_VERSION_JSON"
  # docker logs bingle_webserver
  exit 1
fi

echo "All components started and verified."
echo "Webserver is running at http://localhost:12121"
echo "Press Ctrl-C to stop."

# Keep script running to maintain containers until Ctrl-C
while true; do sleep 1; done
