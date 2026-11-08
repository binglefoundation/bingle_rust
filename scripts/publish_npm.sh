#!/usr/bin/env bash
# Publish react-native-bingle-jsi to npm at the version the tree carries (issue #399).
#
#   DRY_RUN=1 scripts/publish_npm.sh   # pack the module and list what would ship; publish nothing
#   scripts/publish_npm.sh             # publish, unless npm already has this version
#
# The native libraries must already be built into bingle_jsi (bingle_jsi/scripts/build_ios.sh and
# build_android.sh); the script refuses to publish without them. deploy.yml runs it with npm Trusted
# Publishing: npm exchanges the job's GitHub OpenID Connect (OIDC) identity for a one-time
# credential, so no npm token is stored anywhere, and attaches provenance. That needs npm 11.5.1 or
# later and `id-token: write` on the job.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PKG=react-native-bingle-jsi
VERSION=$(scripts/bump_version.sh --check)

# what the package ships besides its sources; all four are build outputs (gitignored).
for built in bingle_jsi/ios/BingleJsi.xcframework bingle_jsi/ios/generated bingle_jsi/android/generated bingle_jsi/android/src/main/jniLibs; do
  [[ -d "$built" && -n "$(ls -A "$built")" ]] || { echo "error: $built is missing or empty; build the native libraries first" >&2; exit 1; }
done

if [[ "${DRY_RUN:-0}" == 1 ]]; then
  (cd bingle_jsi && npm publish --dry-run --access public)
  echo "packed $PKG $VERSION (dry run, nothing published)"
  exit 0
fi

if [[ -n "$(npm view "$PKG@$VERSION" version 2>/dev/null || true)" ]]; then
  echo "$PKG $VERSION is already on npm; skipping"
  exit 0
fi
# --provenance is implied by Trusted Publishing; stated so a hand publish from CI fails loudly
# rather than shipping without it.
(cd bingle_jsi && npm publish --access public --provenance)
echo "published $PKG $VERSION"
