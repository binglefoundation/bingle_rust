#!/usr/bin/env bash
# scripts/clean_build_cache.sh
#
# Reclaim disk by deleting regenerable build cache. A full build — especially the
# bingle_jsi native builds, which cross-compile for every iOS and Android
# architecture — leaves tens of gigabytes of caches that a normal rebuild recreates
# from scratch. Run this after a successful deploy (or any time the disk is filling)
# to reclaim that space; the next build simply rebuilds what it needs.
#
# What it removes (all regenerable, nothing git-tracked):
#   - the Cargo workspace target/ dir (the largest single consumer)
#   - the out-of-repo native (JSI multi-arch) caches used by build_ios.sh /
#     build_android.sh: $CARGO_TARGET_DIR (default /var/tmp/bingle_native_target)
#     and $BINGLE_NATIVE_CARGO_HOME (default /var/tmp/bingle_native_cargo_home)
#   - git-ignored JSI build output under bingle_jsi/ (xcframework, jniLibs, generated
#     bindings, and the React Native example's gradle/xcode/node_modules dirs)
#
# Usage:
#   scripts/clean_build_cache.sh            # reclaim everything, report freed space
#   scripts/clean_build_cache.sh --dry-run  # list what would be removed, delete nothing
#   scripts/clean_build_cache.sh --jsi-only # only the native/JSI caches; keep target/
#   scripts/clean_build_cache.sh --yes      # skip the confirmation prompt (for automation)
set -euo pipefail

DRY_RUN=0
JSI_ONLY=0
ASSUME_YES=0
for arg in "$@"; do
  case "$arg" in
    --dry-run)  DRY_RUN=1 ;;
    --jsi-only) JSI_ONLY=1 ;;
    -y|--yes)   ASSUME_YES=1 ;;
    -h|--help)  sed -n '2,22p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)          echo "unknown option: $arg" >&2; exit 1 ;;
  esac
done

ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"
cd "$ROOT_DIR"

# The native (JSI multi-arch) caches live outside the repo by default, in a
# username-free location; honour the same env overrides the build scripts use so we
# clean whatever they actually wrote.
NATIVE_TARGET="${CARGO_TARGET_DIR:-/var/tmp/bingle_native_target}"
NATIVE_CARGO_HOME="${BINGLE_NATIVE_CARGO_HOME:-/var/tmp/bingle_native_cargo_home}"

size_of() { du -sh "$1" 2>/dev/null | cut -f1; }

remove_path() {
  local p="$1"
  if [[ ! -e "$p" ]]; then
    echo "    skip (absent): $p"
    return
  fi
  local size; size="$(size_of "$p")"
  if [[ $DRY_RUN -eq 1 ]]; then
    echo "    would remove:  $p ($size)"
  else
    echo "    removing:      $p ($size)"
    rm -rf "$p"
  fi
}

# Guard the destructive run: on an interactive terminal, confirm before deleting
# (the dry-run listing is the safety valve otherwise). --yes skips this, which is
# how deploy_code.sh calls it post-release without blocking.
if [[ $DRY_RUN -eq 0 && $ASSUME_YES -eq 0 && -t 0 ]]; then
  read -rp "This deletes regenerable build cache (incl. target/, potentially tens of GB). Continue? [y/N] " ans
  [[ "$ans" =~ ^[Yy]$ ]] || { echo "aborted."; exit 0; }
fi

echo "==> reclaiming build cache$([[ $DRY_RUN -eq 1 ]] && echo ' (dry-run)')"

echo "  native (JSI multi-arch) caches:"
remove_path "$NATIVE_TARGET"
remove_path "$NATIVE_CARGO_HOME"

# git clean -X removes only ignored files, never tracked ones — so this can never
# delete a committed artifact, only regenerable build output.
echo "  git-ignored JSI build output (bingle_jsi/):"
if [[ $DRY_RUN -eq 1 ]]; then
  git clean -ndX bingle_jsi | sed 's/^/    /'
else
  git clean -fdX bingle_jsi | sed 's/^/    /'
fi

if [[ $JSI_ONLY -eq 0 ]]; then
  echo "  Cargo workspace target/:"
  remove_path "$ROOT_DIR/target"
else
  echo "  (--jsi-only: keeping workspace target/)"
fi

echo "==> done$([[ $DRY_RUN -eq 1 ]] && echo ' (dry-run — nothing deleted)')"
df -h "$ROOT_DIR" | awk 'NR==1 || NR==2'
