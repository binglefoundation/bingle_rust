#!/usr/bin/env bash
# Guard against build-machine absolute paths leaking into shipped native
# libraries (they expose the builder's username and filesystem layout).
#
# The needles are derived at run time from the environment and from generic
# home-directory roots — deliberately NOT hard-coded — so this script itself
# contains no personal identifiers to commit, yet still catches a leak on any
# developer machine or CI runner.
#
# Usage (standalone):  bash scan_native_leaks.sh <file-or-dir> [more...]
# Usage (sourced):     source scan_native_leaks.sh; scan_native_leaks <path...>
set -euo pipefail

scan_native_leaks() {
  # Machine-specific needles, read from the live environment. These are the
  # exact prefixes the remap is meant to strip; if remapping worked they will
  # not appear in the output artifacts.
  local -a needles=()
  [[ -n "${HOME:-}"        ]] && needles+=("$HOME")
  [[ -n "${ROOT_DIR:-}"    ]] && needles+=("$ROOT_DIR")
  [[ -n "${CARGO_HOME:-}"  ]] && needles+=("$CARGO_HOME")
  [[ -n "${RUSTUP_HOME:-}" ]] && needles+=("$RUSTUP_HOME")
  # Generic, identity-free catches: any absolute home root, or an un-remapped
  # cargo/rustup source dir, means a build path slipped through.
  needles+=("/Users/" "/home/" ".cargo/registry" ".rustup/toolchains")

  local found=0 f
  for target in "$@"; do
    while IFS= read -r -d '' f; do
      # `strings` surfaces text inside the binary so grep can match it.
      # No `grep -q`: it exits on the first match, which under `pipefail` sends
      # SIGPIPE upstream and makes the pipeline report failure (a false pass).
      # Letting grep drain all input keeps the exit status honest.
      if strings -n 6 "$f" | grep -Ff <(printf '%s\n' "${needles[@]}") >/dev/null; then
        echo "LEAK: build path embedded in $f" >&2
        # String runs in these libs can be thousands of chars with no newline,
        # so pull out just the path-shaped tokens for a readable sample.
        # `|| true` shields the display from `head` closing the pipe early.
        strings -n 6 "$f" \
          | grep -oE '(/Users/|/home/)[[:graph:]]{0,90}' \
          | sort -u | head -10 | sed 's/^/    /' >&2 || true
        found=1
      fi
    done < <(find "$target" -type f \( -name '*.so' -o -name '*.a' -o -name '*.dylib' \) -print0)
  done

  if [[ "$found" -ne 0 ]]; then
    echo "Native library leak scan FAILED — see paths above." >&2
    return 1
  fi
  echo "Native library leak scan passed: no build paths embedded."
}

# Run directly when executed, act as a library when sourced.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  scan_native_leaks "$@"
fi
