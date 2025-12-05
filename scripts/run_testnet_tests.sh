#!/usr/bin/env bash

CREATOR_PASSPHRASE="version rural bring cushion ball case borrow present avoid else pupil alcohol marine attitude extra favorite mass move midnight symbol sibling latin language able borrow"

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

TESTNET_USER=testuser10
TESTNET_ADDRESS=SRIDWL763LIECMBL5N4WRJE6TGBBJL6SKJ6OZOMEQSGAOKW5JEBVUUH3QU
TESTNET_PASSPHRASE="sand fantasy youth fix suggest immense stem awful piano pyramid garment wear butter setup cake finger hawk game language demise company surprise rule about during"

# Update these users

bingle_admin root $RELAY_A_ADDRESS --enable \
 --node-file nodely_testnet_node.json \
 --passphrase "$CREATOR_PASSPHRASE" \
 --debug

bingle_admin root $RELAY_B_ADDRESS --enable \
 --node-file nodely_testnet_node.json \
 --passphrase "$CREATOR_PASSPHRASE" \
 --debug

bingle_admin updateuser --handle $TESTNET_USER \
 --passphrase "$CREATOR_PASSPHRASE" \
 --node-file nodely_testnet_node.json \
 --debug --userpassphrase "$TESTNET_PASSPHRASE"

# Start relay containers
docker run --platform linux/arm64 --rm -d \
 --name bingle_relay_a \
 -e PASSPHRASE="$RELAY_A_PASSPHRASE" \
 -e PORT=$RELAY_A_PORT \
 -e HANDLE=$RELAY_A_HANDLE \
 -p $RELAY_A_PORT:$RELAY_A_PORT/udp \
 "bingle:local"
sleep 20

docker run --platform linux/arm64 --rm -d \
 --name bingle_relay_b \
 -e PASSPHRASE="$RELAY_B_PASSPHRASE" \
 -e PORT=$RELAY_B_PORT \
 -e HANDLE=$RELAY_B_HANDLE \
 -p $RELAY_B_PORT:$RELAY_B_PORT/udp \
 "bingle:local"
sleep 20

# Build or refresh the tests image (uses Dockerfile tests stage and prebuilt test binary)
export BINGLE_RUN_TESTNET=1
export TESTNET_USER=$TESTNET_USER
export TESTNET_PASSPHRASE="$TESTNET_PASSPHRASE"

# Ensure the latest tests image is built from the current workspace
scripts/build_tests_image.sh --tag bingle-tests:local

# Prepare output directory to collect results from the container
mkdir -p tmp/test_out

# Run the prebuilt test inside the dedicated tests image (streams output and waits for completion)
docker run --platform linux/arm64 --rm \
  --name bingle_test_runner \
  --network host \
  -e TESTNET_USER="$TESTNET_USER" \
  -e TESTNET_PASSPHRASE="$TESTNET_PASSPHRASE" \
  -v "$PWD/tmp/test_out":/out \
  "bingle-tests:local"
