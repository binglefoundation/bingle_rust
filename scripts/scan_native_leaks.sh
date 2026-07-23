#!/usr/bin/env bash
# Guard against build-machine absolute paths leaking into shipped artifacts
# (they expose the builder's username and filesystem layout). Two layers:
#
#   1. Native libraries (.so/.a/.dylib) — the build path a compiler embeds.
#   2. Distributable packages — the cargo `.crate` tarballs published to
#      crates.io and the npm `.tgz` published for react-native-bingle-jsi. Each
#      is extracted and every file inside is scanned, so a leak in a bundled
#      native library or a generated text file is caught before release.
#
# The needles are derived at run time from the environment and from generic
# home-directory roots — deliberately NOT hard-coded — so this script itself
# contains no personal identifiers to commit, yet still catches a leak on any
# developer machine or CI runner.
#
# Usage (standalone):
#   bash scan_native_leaks.sh <file-or-dir> [more...]   scan native libs under paths
#   bash scan_native_leaks.sh --cargo                   build & scan cargo packages
#   bash scan_native_leaks.sh --npm                     build & scan the npm package
#   bash scan_native_leaks.sh --packages                scan both cargo and npm packages
# Usage (sourced):
#   source scan_native_leaks.sh; scan_native_leaks <path...>
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Print the machine-specific and generic needles, one per line. The
# machine-specific ones are read from the live environment: they are the exact
# prefixes the remap is meant to strip, so if remapping worked they will not
# appear in the output artifacts. The generic ones are identity-free catches —
# any absolute home root, or an un-remapped cargo/rustup source dir, means a
# build path slipped through.
_leak_needles() {
  local -a needles=()
  [[ -n "${HOME:-}"        ]] && needles+=("$HOME")
  [[ -n "${ROOT_DIR:-}"    ]] && needles+=("$ROOT_DIR")
  [[ -n "${CARGO_HOME:-}"  ]] && needles+=("$CARGO_HOME")
  [[ -n "${RUSTUP_HOME:-}" ]] && needles+=("$RUSTUP_HOME")
  needles+=("/Users/" "/home/" ".cargo/registry" ".rustup/toolchains")
  printf '%s\n' "${needles[@]}"
}

# Scan a single file for any needle. Returns 1 and prints a report on a hit.
# $1 = file to scan, $2 = file holding the newline-separated needles.
_scan_one_file() {
  local f="$1" needle_file="$2"
  # `strings` surfaces text inside the binary (and plain text alike) so grep can
  # match it. No `grep -q`: it exits on the first match, which under `pipefail`
  # sends SIGPIPE upstream and makes the pipeline report failure (a false pass).
  # Letting grep drain all input keeps the exit status honest.
  if strings -n 6 "$f" | grep -Ff "$needle_file" >/dev/null; then
    echo "LEAK: build path embedded in $f" >&2
    # String runs in these libs can be thousands of chars with no newline,
    # so pull out just the path-shaped tokens for a readable sample.
    # `|| true` shields the display from `head` closing the pipe early.
    strings -n 6 "$f" \
      | grep -oE '(/Users/|/home/)[[:graph:]]{0,90}' \
      | sort -u | head -10 | sed 's/^/    /' >&2 || true
    return 1
  fi
  return 0
}

# Scan every file under a tree that matches an optional find predicate.
# $1 = root path; remaining args = extra find predicate (empty = every file).
_scan_tree() {
  local root="$1"; shift
  local needle_file found=0 f
  needle_file="$(mktemp)"
  _leak_needles > "$needle_file"
  while IFS= read -r -d '' f; do
    _scan_one_file "$f" "$needle_file" || found=1
  done < <(find "$root" -type f "$@" -print0)
  rm -f "$needle_file"
  return "$found"
}

# Public: scan native libraries (.so/.a/.dylib) under the given files or dirs.
scan_native_leaks() {
  local target found=0
  for target in "$@"; do
    _scan_tree "$target" \( -name '*.so' -o -name '*.a' -o -name '*.dylib' \) || found=1
  done

  if [[ "$found" -ne 0 ]]; then
    echo "Native library leak scan FAILED — see paths above." >&2
    return 1
  fi
  echo "Native library leak scan passed: no build paths embedded."
}

# Extract a cargo `.crate` / npm `.tgz` / `.tar.gz` and scan every file inside.
_scan_archive() {
  local archive="$1" tmp found=0
  tmp="$(mktemp -d)"
  tar -xzf "$archive" -C "$tmp"
  echo "  scanning $(basename "$archive") ..."
  _scan_tree "$tmp" || found=1
  rm -rf "$tmp"
  return "$found"
}

# Public: build the publishable cargo packages and scan their `.crate` tarballs.
scan_cargo_packages() {
  local pkg_target="$REPO_ROOT/tmp/leak-scan/cargo" found=0 crate_file
  # Only crates that are actually published to crates.io. `--no-verify` skips
  # the compile step (we only care about the packaged bytes) and `--allow-dirty`
  # lets the check run against an in-progress working tree.
  rm -rf "$pkg_target"
  mkdir -p "$pkg_target"
  echo "Building cargo packages (bingle_core, bingle_local) ..."
  cargo package --manifest-path "$REPO_ROOT/Cargo.toml" \
    -p bingle_core -p bingle_local \
    --no-verify --allow-dirty --target-dir "$pkg_target" >/dev/null

  shopt -s nullglob
  for crate_file in "$pkg_target"/package/*.crate; do
    _scan_archive "$crate_file" || found=1
  done
  shopt -u nullglob

  if [[ "$found" -ne 0 ]]; then
    echo "Cargo package leak scan FAILED — see paths above." >&2
    return 1
  fi
  echo "Cargo package leak scan passed: no build paths embedded."
}

# Public: pack the react-native-bingle-jsi npm module and scan its tarball.
scan_npm_package() {
  local jsi_dir="$REPO_ROOT/bingle_jsi" out="$REPO_ROOT/tmp/leak-scan/npm" found=0 tgz
  rm -rf "$out"
  mkdir -p "$out"
  echo "Packing npm module (react-native-bingle-jsi) ..."
  ( cd "$jsi_dir" && npm pack --pack-destination "$out" >/dev/null )

  shopt -s nullglob
  for tgz in "$out"/*.tgz; do
    _scan_archive "$tgz" || found=1
  done
  shopt -u nullglob

  if [[ "$found" -ne 0 ]]; then
    echo "npm package leak scan FAILED — see paths above." >&2
    return 1
  fi
  echo "npm package leak scan passed: no build paths embedded."
}

# Run directly when executed, act as a library when sourced.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  case "${1:-}" in
    --cargo)      scan_cargo_packages ;;
    --npm)        scan_npm_package ;;
    --packages)   scan_cargo_packages && scan_npm_package ;;
    -h|--help|"") sed -n '16,22p' "${BASH_SOURCE[0]}" ;;
    *)            scan_native_leaks "$@" ;;
  esac
fi
