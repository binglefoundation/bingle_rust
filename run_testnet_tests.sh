# Start relay containers on the same docker network
# They will autodetect EXTERNAL_IP inside the container and register static endpoints
# reachable from other containers on this network.
docker run --platform linux/arm64 -d \
 --name bingle_relay_a \
 --network bingle_testnet \
 -e RELAY=1 \
 -e PASSPHRASE="$RELAY_A_PASSPHRASE" \
 -e PORT=$RELAY_A_PORT \
 -e HANDLE=$RELAY_A_HANDLE \
 "bingle:local"

if [ $? -ne 0 ]; then
    echo "ERROR: Failed to start bingle_relay_a container"
    exit 1
fi

docker run --platform linux/arm64 -d \
 --name bingle_relay_b \
