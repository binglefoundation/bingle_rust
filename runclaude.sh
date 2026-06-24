#!/usr/bin/env bash
set -euo pipefail

dir=$(basename "$(cd "$(dirname "$0")" && pwd)")
exec claude --dangerously-allow-all -w "$dir"
