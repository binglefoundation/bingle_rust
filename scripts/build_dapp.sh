#!/usr/bin/env bash
# scripts/build_dapp.sh
# Purpose: Build the Algorand dapp smart contracts (dapp_projects) reproducibly.
#   1. Verifies a Python 3.12+ toolchain (python == python3).
#   2. Ensures the AlgoKit + Poetry build environment (idempotent bootstrap).
#   3. Compiles the contract(s) to TEAL / ARC56 artifacts.
#   4. Warns when the built app schema differs from the master-branch baseline.
#      A state-schema change means the on-chain app can no longer be updated in
#      place and must be re-deployed (a new app is created).
#
# Safe to run on a freshly cloned repo and safe to re-run after a previous build.
#
# Usage:
#   scripts/build_dapp.sh [--no-schema-check]
#
#   --no-schema-check   Build only; skip the master-branch schema comparison.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

DAPP_DIR="$REPO_ROOT/dapp_projects"
CONTRACT_NAME="bingle_dapp"
ARTIFACT_DIR="$DAPP_DIR/smart_contracts/artifacts/$CONTRACT_NAME"
ARC56="$ARTIFACT_DIR/BingleDapp.arc56.json"
APPROVAL_TEAL="$ARTIFACT_DIR/BingleDapp.approval.teal"
CLEAR_TEAL="$ARTIFACT_DIR/BingleDapp.clear.teal"
# Committed schema baseline (a small "golden" file, tracked in git) that records
# the app's state schema + program fingerprints for the master comparison.
SCHEMA_REF_REL="dapp_projects/smart_contracts/$CONTRACT_NAME/app_schema.json"
SCHEMA_REF="$REPO_ROOT/$SCHEMA_REF_REL"
BASELINE_REF="master"

DO_SCHEMA_CHECK=1
for arg in "$@"; do
  case "$arg" in
    --no-schema-check) DO_SCHEMA_CHECK=0 ;;
    -h|--help) awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "$0"; exit 0 ;;
    *) echo "Error: unknown argument: $arg" >&2; exit 2 ;;
  esac
done

# ------------------------------------------------------------------ #
# 1. Python 3.12+ toolchain (python must resolve to python3)
# ------------------------------------------------------------------ #
# Prefer the well-supported 3.12 / 3.13 interpreters over a bare `python3`, which
# may be a newer release (e.g. 3.14) that some Algorand build deps (coincurve)
# cannot yet compile against.
find_python() {
  local candidate
  for candidate in python3.12 python3.13 python3 python; do
    if command -v "$candidate" >/dev/null 2>&1 \
      && "$candidate" -c 'import sys; sys.exit(0 if (3, 12) <= sys.version_info[:2] < (3, 14) else 1)' >/dev/null 2>&1; then
      command -v "$candidate"
      return 0
    fi
  done
  # Fall back to any Python 3.12+ if nothing in the preferred range is available.
  for candidate in python3 python; do
    if command -v "$candidate" >/dev/null 2>&1 \
      && "$candidate" -c 'import sys; sys.exit(0 if sys.version_info[:2] >= (3, 12) else 1)' >/dev/null 2>&1; then
      command -v "$candidate"
      return 0
    fi
  done
  return 1
}

if ! PYTHON="$(find_python)"; then
  echo "Error: Python 3.12+ is required (python must be python3)." >&2
  echo "       Install it from https://www.python.org/downloads/" >&2
  exit 1
fi
echo "==> Python: $PYTHON ($("$PYTHON" --version 2>&1))"

# ------------------------------------------------------------------ #
# 2. Build environment: Poetry + AlgoKit, then bootstrap deps
# ------------------------------------------------------------------ #
if ! command -v poetry >/dev/null 2>&1; then
  echo "Error: poetry not found. Install: https://python-poetry.org/docs/#installation" >&2
  exit 1
fi
if ! command -v algokit >/dev/null 2>&1; then
  echo "Error: algokit CLI not found. Install: https://github.com/algorandfoundation/algokit-cli#install" >&2
  exit 1
fi

echo "==> Poetry: $(poetry --version 2>&1)   AlgoKit: $(algokit --version 2>&1)"

echo "==> Ensuring Poetry virtualenv uses $PYTHON"
poetry -C "$DAPP_DIR" env use "$PYTHON" >/dev/null

echo "==> Installing dapp dependencies (idempotent)"
poetry -C "$DAPP_DIR" install --no-interaction

# ------------------------------------------------------------------ #
# 3. Build the contracts
# ------------------------------------------------------------------ #
echo "==> Building smart contracts"
# Run from the project dir so the `smart_contracts` package is importable.
( cd "$DAPP_DIR" && poetry run python -m smart_contracts build )

if [[ ! -f "$ARC56" ]]; then
  echo "Error: build did not produce expected artifact: $ARC56" >&2
  exit 1
fi
echo "==> Build OK. Artifacts in $ARTIFACT_DIR"

# ------------------------------------------------------------------ #
# 4. Refresh the schema baseline and compare against master
# ------------------------------------------------------------------ #
# Regenerate the committed schema fingerprint from the fresh build. This is the
# source of truth for "what schema/program is expected to be deployed".
"$PYTHON" - "$ARC56" "$APPROVAL_TEAL" "$CLEAR_TEAL" > "$SCHEMA_REF" <<'PY'
import hashlib, json, sys

arc56_path, approval_path, clear_path = sys.argv[1:4]
spec = json.load(open(arc56_path))

def sha256(path):
    with open(path, "rb") as fh:
        return hashlib.sha256(fh.read()).hexdigest()

doc = {
    "contract": spec.get("name"),
    "schema": spec["state"]["schema"],
    "approval_sha256": sha256(approval_path),
    "clear_sha256": sha256(clear_path),
}
print(json.dumps(doc, indent=2, sort_keys=True))
PY

if [[ "$DO_SCHEMA_CHECK" -eq 0 ]]; then
  echo "==> Schema baseline written to $SCHEMA_REF_REL (comparison skipped: --no-schema-check)"
  exit 0
fi

# Resolve a baseline ref: prefer local master, fall back to origin/master.
BASE=""
if git rev-parse --verify --quiet "refs/heads/$BASELINE_REF" >/dev/null; then
  BASE="$BASELINE_REF"
elif git rev-parse --verify --quiet "refs/remotes/origin/$BASELINE_REF" >/dev/null; then
  BASE="origin/$BASELINE_REF"
fi

if [[ -z "$BASE" ]]; then
  echo "WARNING: no '$BASELINE_REF' branch found; cannot compare app schema." >&2
  echo "         Treat this as a fresh app: an app DEPLOY is required." >&2
  exit 0
fi

if ! git cat-file -e "$BASE:$SCHEMA_REF_REL" 2>/dev/null; then
  echo "WARNING: no schema baseline committed on $BASE ($SCHEMA_REF_REL)." >&2
  echo "         Treat this as a fresh app: an app DEPLOY is required." >&2
  echo "         Commit $SCHEMA_REF_REL once the app is deployed to establish the baseline." >&2
  exit 0
fi

BASELINE_TMP="$(mktemp)"
trap 'rm -f "$BASELINE_TMP"' EXIT
git show "$BASE:$SCHEMA_REF_REL" > "$BASELINE_TMP"

RESULT="$("$PYTHON" - "$SCHEMA_REF" "$BASELINE_TMP" <<'PY'
import json, sys
cur = json.load(open(sys.argv[1]))
base = json.load(open(sys.argv[2]))
if cur.get("schema") != base.get("schema"):
    print("SCHEMA_CHANGED")
elif (cur.get("approval_sha256"), cur.get("clear_sha256")) != \
     (base.get("approval_sha256"), base.get("clear_sha256")):
    print("PROGRAM_CHANGED")
else:
    print("MATCH")
PY
)"

case "$RESULT" in
  MATCH)
    echo "==> App schema matches $BASE. No deploy needed."
    ;;
  PROGRAM_CHANGED)
    echo "WARNING: approval/clear program differs from $BASE, but the state schema is unchanged." >&2
    echo "         An app UPDATE (deploy) is needed to roll out the new program." >&2
    echo "         Commit the updated $SCHEMA_REF_REL after deploying." >&2
    ;;
  SCHEMA_CHANGED)
    echo "WARNING: app state schema differs from $BASE." >&2
    echo "         The schema is fixed at app creation, so a NEW app must be DEPLOYED." >&2
    echo "         Commit the updated $SCHEMA_REF_REL after deploying. Diff vs $BASE:" >&2
    git --no-pager diff --no-index -- "$BASELINE_TMP" "$SCHEMA_REF" || true
    ;;
  *)
    echo "Error: unexpected schema comparison result: $RESULT" >&2
    exit 1
    ;;
esac
