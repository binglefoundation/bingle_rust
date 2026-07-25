#!/usr/bin/env bash
# scripts/download_latest_logs.sh
#
# Purpose: Download the latest log stream from a specified AWS CloudWatch log group.
#
# Usage:
#   scripts/download_latest_logs.sh [--region <region>] <relay_name>
#
# The AWS region defaults to your `aws configure` region; override it with
# --region. CloudWatch log groups are region-scoped, so this must match the
# region the relay was deployed to (see aws/deploy_relay.sh).
#
# Example:
#   scripts/download_latest_logs.sh bingle-relay-staging-1
#   scripts/download_latest_logs.sh --region eu-west-1 bingle-relay-staging-1

set -euo pipefail

REGION=$(aws configure get region 2>/dev/null || true)
RELAY_NAME=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --region) REGION="$2"; shift 2 ;;
    -h|--help) echo "Usage: $0 [--region <region>] <relay_name>"; exit 0 ;;
    -*) echo "Unknown option: $1" >&2; exit 1 ;;
    *)
      if [[ -n "$RELAY_NAME" ]]; then
        echo "Unexpected extra argument: $1" >&2; exit 1
      fi
      RELAY_NAME="$1"; shift ;;
  esac
done

if [[ -z "$RELAY_NAME" ]]; then
  echo "Usage: $0 [--region <region>] <relay_name>" >&2
  exit 1
fi

if [[ -z "$REGION" ]]; then
  echo "Error: no AWS region set. Pass --region <region> or run 'aws configure set region <region>'." >&2
  exit 1
fi
# Replace slashes with underscores for the filename
SAFE_RELAY_NAME=$(echo "$RELAY_NAME" | sed 's/\//_/g' | sed 's/^_//')
OUTPUT_FILE="tmp/$SAFE_RELAY_NAME".log

LOG_GROUP_NAME="/bingle/relay/$RELAY_NAME"
echo "Fetching latest log stream for group: $LOG_GROUP_NAME (region: $REGION)"

# 1) Get the latest log stream name
STREAM_NAME=$(aws logs describe-log-streams \
  --region "$REGION" \
  --log-group-name "$LOG_GROUP_NAME" \
  --order-by LastEventTime \
  --descending \
  --limit 1 \
  --query 'logStreams[0].logStreamName' \
  --output text)

if [[ "$STREAM_NAME" == "None" || -z "$STREAM_NAME" ]]; then
  echo "Error: No log streams found for log group $LOG_GROUP_NAME"
  exit 1
fi

echo "Latest log stream: $STREAM_NAME"
echo "Downloading logs to $OUTPUT_FILE ..."

# 2) Download all log events from the stream handling pagination
# Using sed 's/^[[:space:]]*//' to remove leading whitespace
# Use a temporary file to accumulate logs
TEMP_LOG_FILE=$(mktemp)

NEXT_TOKEN=""
echo "Fetching events..."
while true; do
  if [[ -z "$NEXT_TOKEN" ]]; then
    # First call
    RESPONSE=$(aws logs get-log-events \
      --region "$REGION" \
      --log-group-name "$LOG_GROUP_NAME" \
      --log-stream-name "$STREAM_NAME" \
      --start-from-head \
      --output json)
  else
    # Subsequent calls with nextForwardToken
    RESPONSE=$(aws logs get-log-events \
      --region "$REGION" \
      --log-group-name "$LOG_GROUP_NAME" \
      --log-stream-name "$STREAM_NAME" \
      --next-token "$NEXT_TOKEN" \
      --output json)
  fi

  # Extract messages and append to temp file
  # Use sed to remove leading whitespace and delete empty lines
  echo "$RESPONSE" | jq -r '.events[].message' | sed 's/^[[:space:]]*//; /^[[:space:]]*$/d' >> "$TEMP_LOG_FILE"

  # Get next token
  NEW_TOKEN=$(echo "$RESPONSE" | jq -r '.nextForwardToken')

  # If NEW_TOKEN is the same as NEXT_TOKEN, we've reached the end
  if [[ "$NEW_TOKEN" == "$NEXT_TOKEN" || "$NEW_TOKEN" == "null" ]]; then
    break
  fi
  NEXT_TOKEN="$NEW_TOKEN"
done

mv "$TEMP_LOG_FILE" "$OUTPUT_FILE"

echo "Done. Logs saved to $OUTPUT_FILE"
