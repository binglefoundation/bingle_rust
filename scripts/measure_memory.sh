#!/usr/bin/env bash
# scripts/measure_memory.sh
# Purpose: Measure peak memory usage of a Docker container by polling its stats.
#
# Usage:
#   scripts/measure_memory.sh <container_name_or_id> [interval]
#
# Example:
#   scripts/measure_memory.sh bingle_webserver 0.5

CONTAINER=$1
INTERVAL=${2:-1}

if [[ -z "$CONTAINER" ]]; then
  echo "Usage: $0 <container_name_or_id> [interval]"
  exit 1
fi

PEAK_BYTES=0

format_bytes() {
  local b=$1
  if [[ $b -ge 1073741824 ]]; then
    echo "$(awk "BEGIN {printf \"%.2f GB\", $b/1073741824}")"
  elif [[ $b -ge 1048576 ]]; then
    echo "$(awk "BEGIN {printf \"%.2f MB\", $b/1048576}")"
  elif [[ $b -ge 1024 ]]; then
    echo "$(awk "BEGIN {printf \"%.2f KB\", $b/1024}")"
  else
    echo "$b bytes"
  fi
}

echo "Monitoring peak memory for container: $CONTAINER (Interval: ${INTERVAL}s)"
echo "Press Ctrl+C to stop and show final result."

# Trap Ctrl+C
trap 'echo ""; echo "Final Peak Memory for $CONTAINER: $(format_bytes $PEAK_BYTES)"; exit 0' SIGINT SIGTERM

while true; do
  # docker stats --no-stream --format "{{.MemUsage}}"
  # Output is typically "12.34MiB / 15.58GiB" or "1.234MB / 1.5GB"
  STATS=$(docker stats --no-stream --format "{{.MemUsage}}" "$CONTAINER" 2>/dev/null || true)
  
  if [[ -z "$STATS" ]]; then
    if [[ $PEAK_BYTES -gt 0 ]]; then
      echo -e "\nContainer $CONTAINER stopped. Final Peak Memory: $(format_bytes $PEAK_BYTES)"
      exit 0
    fi
    echo -ne "\rWaiting for container $CONTAINER to start...          "
    sleep "$INTERVAL"
    continue
  fi

  # Extract the usage part
  USAGE_STR=$(echo "$STATS" | awk -F' / ' '{print $1}' | tr -d ' ')
  
  # Parse value and unit
  VALUE=$(echo "$USAGE_STR" | sed 's/[a-zA-Z]//g')
  UNIT=$(echo "$USAGE_STR" | sed 's/[0-9.]//g' | tr '[:upper:]' '[:lower:]')
  
  BYTES=0
  if [[ -n "$VALUE" ]]; then
    case "$UNIT" in
      gib|gb) BYTES=$(awk "BEGIN {print $VALUE * 1024 * 1024 * 1024}") ;;
      mib|mb) BYTES=$(awk "BEGIN {print $VALUE * 1024 * 1024}") ;;
      kib|kb) BYTES=$(awk "BEGIN {print $VALUE * 1024}") ;;
      b|"")   BYTES=$VALUE ;;
      *)      BYTES=0 ;;
    esac
  fi

  # Round BYTES to integer for simple comparison if possible, or use awk
  if [[ $(awk "BEGIN {print ($BYTES > $PEAK_BYTES)}") -eq 1 ]]; then
    PEAK_BYTES=$BYTES
    echo -ne "\rCurrent Peak: $(format_bytes $PEAK_BYTES)          "
  fi
  
  sleep "$INTERVAL"
done
