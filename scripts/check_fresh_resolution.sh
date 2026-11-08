#!/usr/bin/env bash
# Check the workspace compiles under a from-scratch dependency resolution (issue #399; taken from
# the retired deploy_code.sh).
#
#   scripts/check_fresh_resolution.sh
#
# `cargo check --locked` uses the committed Cargo.lock, so it cannot catch what a downstream
# `cargo install bingle_cli` hits: that ignores the lockfile and resolves every dependency to its
# newest semver-compatible version. A newer release of a transitive dependency can break that
# resolve while the locked build stays green (precis-profiles 0.1.14 pulling precis-core 0.2.0,
# which breaks stun-rs 0.1.11). This updates the lock to the latest compatible versions, compiles,
# and restores the committed lock whatever happens. If it fails, pin the offending crate (see the
# precis-* pins in bingle_core/Cargo.toml).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

backup=$(mktemp)
cp Cargo.lock "$backup"
trap 'cp "$backup" Cargo.lock; rm -f "$backup"' EXIT

cargo update --quiet
cargo check --workspace
echo "the workspace compiles under a from-scratch dependency resolution"
