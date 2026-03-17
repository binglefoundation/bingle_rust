#!/usr/bin/env bash
# scripts/run_all_tests.sh
# Comprehensive test runner for the Bingle Rust project.
# Runs unit and integration tests across all workspace members.

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

echo -e "${GREEN}=== Running all standard tests (workspace and integration targets) ===${NC}"
# --workspace covers all crates in the project.
# Standard cargo test skips #[ignore] tests.
cargo test --workspace

echo -e "\n${YELLOW}=== Running tests that are skipped by default (ignored) ===${NC}"
echo "Note: Tests requiring a live localnet/testnet will skip themselves if not available."
# Run ignored tests across the workspace.
cargo test --workspace -- --ignored
