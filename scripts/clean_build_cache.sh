#!/usr/bin/env bash
# scripts/clean_build_cache.sh
#
# Reclaim disk by deleting regenerable build cache. A full build — especially the
# bingle_jsi native builds, which cross-compile for every iOS and Android
# architecture — leaves tens of gigabytes of caches that a normal rebuild recreates
# from scratch. Run this after a successful deploy (or any time the disk is filling)
# to reclaim that space; the next build simply rebuilds what it needs.
#
# What it removes (all regenerable, nothing git-tracked), and how each is kept safe:
#   - the Cargo workspace target/ dir — via `cargo clean`, which only ever removes
#     Cargo's own output (no bare rm on a repo path)
#   - the native (JSI multi-arch) caches used by build_ios.sh / build_android.sh:
#     $CARGO_TARGET_DIR (default $TMP_ROOT/bingle_native_target) and
#     $BINGLE_NATIVE_CARGO_HOME (default $TMP_ROOT/bingle_native_cargo_home) — each
#     deleted ONLY if it resolves to a path strictly under $TMP_ROOT (default
#     /var/tmp, where the build scripts write). A mis-set env var pointing outside
#     the tmp root is refused, never deleted.
#   - git-ignored JSI build output under bingle_jsi/ — via `git clean -fdX`, which
#     removes only ignored files, never a tracked/committed artifact
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
    -h|--help)  sed -n '2,26p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)          echo "unknown option: $arg" >&2; exit 1 ;;
  esac
done

ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"
cd "$ROOT_DIR"

# The tmp root that bounds every out-of-repo deletion. The native build scripts
# (bingle_jsi/scripts/build_*.sh) write their caches under /var/tmp; keep these in
# sync. Nothing outside this root is ever rm -rf'd.
TMP_ROOT="/var/tmp"

# Discover the native cache locations, honouring the same env overrides the build
# scripts use — but see remove_under_tmp: a location that resolves outside $TMP_ROOT
# is refused, so honouring the env can never turn into deleting an unrelated dir.
NATIVE_TARGET="${CARGO_TARGET_DIR:-$TMP_ROOT/bingle_native_target}"
NATIVE_CARGO_HOME="${BINGLE_NATIVE_CARGO_HOME:-$TMP_ROOT/bingle_native_cargo_home}"

size_of() { du -sh "$1" 2>/dev/null | cut -f1; }

# Resolve a path to its physical (symlink-free) absolute form, or empty if absent /
# unreadable. Always returns 0 so it is safe under `set -e` in a `x="$(resolve_dir ...)"`
# assignment (a missing cache dir is normal, not an error).
resolve_dir() {
  if [[ -d "$1" ]]; then
    (cd "$1" 2>/dev/null && pwd -P) || true
  fi
}

# rm -rf a directory ONLY if it resolves to a location strictly under $TMP_ROOT.
# This is the safety net: a mis-set CARGO_TARGET_DIR / BINGLE_NATIVE_CARGO_HOME (or
# a bad expansion) pointing outside the tmp root is refused rather than deleted.
remove_under_tmp() {
  local p="$1" real root
  real="$(resolve_dir "$p")"
  if [[ -z "$real" ]]; then
    echo "    skip (absent): $p"
    return
  fi
  root="$(resolve_dir "$TMP_ROOT")"
  if [[ -z "$root" || "$real" != "$root"/?* ]]; then
    echo "    REFUSING '$p' (resolved '$real'): not under tmp root '$TMP_ROOT'" >&2
    return
  fi
  local size; size="$(size_of "$real")"
  if [[ $DRY_RUN -eq 1 ]]; then
    echo "    would remove:  $real ($size)"
  else
    echo "    removing:      $real ($size)"
    rm -rf "$real"
  fi
}

if [[ $DRY_RUN -eq 0 && $ASSUME_YES -eq 0 && -t 0 ]]; then
  read -rp "This deletes regenerable build cache (incl. target/, potentially tens of GB). Continue? [y/N] " ans
  [[ "$ans" =~ ^[Yy]$ ]] || { echo "aborted."; exit 0; }
fi

echo "==> reclaiming build cache$([[ $DRY_RUN -eq 1 ]] && echo ' (dry-run)')"

echo "  native (JSI multi-arch) caches (under $TMP_ROOT):"
remove_under_tmp "$NATIVE_TARGET"
remove_under_tmp "$NATIVE_CARGO_HOME"

# git clean -X removes only ignored files, never tracked ones — so this can never
# delete a committed artifact, only regenerable build output.
echo "  git-ignored JSI build output (bingle_jsi/):"
if [[ $DRY_RUN -eq 1 ]]; then
  git clean -ndX bingle_jsi | sed 's/^/    /'
else
  git clean -fdX bingle_jsi | sed 's/^/    /'
fi

if [[ $JSI_ONLY -eq 0 ]]; then
  echo "  Cargo workspace target/ (cargo clean):"
  if ! command -v cargo >/dev/null 2>&1; then
    echo "    cargo not found on PATH; skipping (run 'cargo clean' yourself)" >&2
  elif [[ $DRY_RUN -eq 1 ]]; then
    echo "    would run: cargo clean  (currently $(size_of "$ROOT_DIR/target"))"
  else
    ( cd "$ROOT_DIR" && cargo clean )
  fi
else
  echo "  (--jsi-only: keeping workspace target/)"
fi

echo "==> done$([[ $DRY_RUN -eq 1 ]] && echo ' (dry-run — nothing deleted)')"
df -h "$ROOT_DIR" | awk 'NR==1 || NR==2'
