#!/usr/bin/env bash
set -euo pipefail

# Generate BINGLE_SPEC.md using openapi-markdown (NPM) and pandoc
# - Converts spec/openapi.yaml to Markdown (generated/message_reference.md)
# - spec/spec_main.md uses `!include ../generated/message_reference.md` and the include-files Lua filter injects it
# - Outputs generated/BINGLE_SPEC.md

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

OPENAPI_YAML="spec/openapi.yaml"
GEN_DIR="generated"
MSG_REF_MD="$GEN_DIR/message_reference.md"
OUTPUT_MD="$GEN_DIR/BINGLE_SPEC.md"
LUA_FILTER="scripts/include-files.lua"

# Check prerequisites
if ! command -v pandoc >/dev/null 2>&1; then
  echo "Error: pandoc is not installed or not in PATH." >&2
  echo "Please install pandoc: https://pandoc.org/installing.html" >&2
  exit 1
fi
if ! command -v npx >/dev/null 2>&1; then
  echo "Error: npx (Node.js/npm) is not installed or not in PATH." >&2
  echo "Please install Node.js/npm: https://nodejs.org/" >&2
  exit 1
fi

# Ensure directories exist
mkdir -p "$GEN_DIR"

# Convert OpenAPI -> Markdown using openapi-markdown
if [ ! -f "$OPENAPI_YAML" ]; then
  echo "Error: OpenAPI file not found: $OPENAPI_YAML" >&2
  exit 1
fi

echo "[1/2] Generating Markdown from OpenAPI via openapi-markdown..."
# Using npx to fetch and run the package without adding it as a dependency
# Common CLI usage: openapi-markdown -i <input.yaml> -o <output.md>
# If your environment uses a different CLI, adjust the command accordingly.
if ! npx --yes openapi-markdown -i "$OPENAPI_YAML" -o "$MSG_REF_MD" >/dev/null 2>&1; then
  echo "openapi-markdown failed to run or is not available as expected." >&2
  echo "Attempting to show error output for troubleshooting:" >&2
  npx --yes openapi-markdown -i "$OPENAPI_YAML" -o "$MSG_REF_MD"
fi

echo "Generated: $MSG_REF_MD"

# Copy assets (images) referenced by spec_main.md into generated/
# Copy SVG and PNG from spec/ to generated/ so relative links work in output
if compgen -G "spec/*.svg" > /dev/null; then
  cp -f spec/*.svg "$GEN_DIR"/
fi
if compgen -G "spec/*.png" > /dev/null; then
  cp -f spec/*.png "$GEN_DIR"/
fi

# Build the full spec document with pandoc and Lua filter
echo "[2/2] Building full spec with pandoc..."
PANDOC_INPUT="spec/spec_main.md"

pandoc \
  "$PANDOC_INPUT" \
  --lua-filter="$LUA_FILTER" \
  --from=markdown \
  --to markdown+hard_line_breaks \
  --wrap=auto \
  -s \
  -t gfm \
  -o "$OUTPUT_MD"

echo "Done. Output: $OUTPUT_MD"
