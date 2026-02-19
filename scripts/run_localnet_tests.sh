#!/usr/bin/env bash
# scripts/run_localnet_tests.sh
# Ensures algokit localnet is running and executes ignored localnet integration tests.

set -e

# 1. Check if algokit is installed
if ! command -v algokit &> /dev/null; then
    echo "ERROR: algokit CLI not found. Please install it to run localnet tests."
    echo "See: https://github.com/algorand/algokit-cli#installation"
    exit 1
fi

# 2. Check localnet status and start if needed
echo "Checking Algorand localnet status..."
if ! algokit localnet status &> /dev/null; then
    echo "Localnet is not running. Starting it now..."
    algokit localnet start
else
    echo "Localnet is already running."
fi

# 3. Set environment variable to enable localnet tests
# This bypasses the probe and forces tests to run (they will fail if network is actually down)
export RUST_COMMS_RUN_LOCALNET=true

# 4. Run ignored tests that match 'localnet'
# These are grouped in tests/all.rs under various modules.
echo "Running ignored localnet tests..."
cargo test --test all localnet -- --ignored --nocapture
