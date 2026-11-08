#!/usr/bin/env bash
# Publish the crates to crates.io at the version the tree carries (issue #399).
#
#   DRY_RUN=1 scripts/publish_crates.sh   # package and verify every crate; publish nothing
#   scripts/publish_crates.sh             # publish, in dependency order, skipping any already on crates.io
#
# deploy.yml runs both, the real publish with a short-lived token from crates.io Trusted Publishing
# in CARGO_REGISTRY_TOKEN. Each crate already published at this version is skipped, so a re-run after
# a partial failure finishes the release without publishing anything twice. By hand (RELEASING.md,
# "Manual fallback"), the token comes from CARGO_REGISTRY_TOKEN or `cargo login`.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# in dependency order: bingle_cli depends on the other two, so it publishes last.
CRATES=(bingle_core bingle_local bingle_cli)
VERSION=$(scripts/bump_version.sh --check)

crate_version_exists() { # <crate> <version>
  curl -sfL -A "bingle-release (github.com/binglefoundation/bingle_rust)" -o /dev/null "https://crates.io/api/v1/crates/$1/$2"
}

if [[ "${DRY_RUN:-0}" == 1 ]]; then
  # Packaging every crate together lets cargo verify each dependent against its siblings' packaged
  # copies in a temporary local registry, so a file missing from a package fails here, not on crates.io.
  args=()
  for crate in "${CRATES[@]}"; do args+=(-p "$crate"); done
  cargo publish --dry-run --locked "${args[@]}"
  echo "packaged and verified ${CRATES[*]} at $VERSION (dry run, nothing published)"
  exit 0
fi

for crate in "${CRATES[@]}"; do
  if crate_version_exists "$crate" "$VERSION"; then
    echo "$crate $VERSION is already on crates.io; skipping"
    continue
  fi
  cargo publish --locked -p "$crate"
  # the next crate depends on this one, so wait until the index serves it.
  for _ in $(seq 1 60); do
    crate_version_exists "$crate" "$VERSION" && break
    sleep 5
  done
  crate_version_exists "$crate" "$VERSION" || { echo "error: $crate $VERSION is not visible on crates.io after publishing; re-run to resume" >&2; exit 1; }
  echo "published $crate $VERSION"
done
