#!/usr/bin/env bash
# aws/stop_relay.sh
#
# Purpose: Stop the Bingle Relay tasks in an ECS cluster by setting desired count to 0.
#
# Usage:
#   aws/stop_relay.sh [--stack-name <name>] [--region <region>]
#
# Options:
#   --stack-name <name>    CloudFormation stack name (default: bingle-relay)
#   --region <region>      AWS region (default: from aws configure)

set -euo pipefail

# Default values
STACK_NAME="bingle-relay"
REGION=$(aws configure get region)

usage() {
  echo "Usage: $0 [options]"
  echo "Options:"
  echo "  --stack-name <name>    CloudFormation stack name (default: $STACK_NAME)"
  echo "  --region <region>      AWS region (default: $REGION)"
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --stack-name) STACK_NAME="$2"; shift 2 ;;
    --region) REGION="$2"; shift 2 ;;
    *) echo "Unknown argument: $1"; usage ;;
  esac
done

echo "[stop] Discovering resources for stack '$STACK_NAME' in region '$REGION'..."

# Find the service and cluster names from the stack.
# We try to get both to ensure the stack exists and has the expected resources.
SERVICE_NAME=$(aws cloudformation describe-stack-resource --stack-name "$STACK_NAME" --logical-resource-id RelayService --region "$REGION" --query "StackResourceDetail.PhysicalResourceId" --output text 2>/dev/null)
CLUSTER_NAME=$(aws cloudformation describe-stack-resource --stack-name "$STACK_NAME" --logical-resource-id ECSCluster --region "$REGION" --query "StackResourceDetail.PhysicalResourceId" --output text 2>/dev/null)

if [[ -z "$SERVICE_NAME" || -z "$CLUSTER_NAME" ]]; then
  echo "Error: Could not find RelayService or ECSCluster resources for stack '$STACK_NAME'."
  echo "Make sure the stack exists and was deployed using the Bingle Relay templates."
  exit 1
fi

echo "[stop] Setting desired count to 0 for service '$SERVICE_NAME' in cluster '$CLUSTER_NAME'..."

aws ecs update-service \
  --cluster "$CLUSTER_NAME" \
  --service "$SERVICE_NAME" \
  --desired-count 0 \
  --region "$REGION" > /dev/null

echo "[stop] Success! Service is being scaled down to 0 tasks."
echo "[stop] You can verify the status in the AWS Console or by running:"
echo "      aws ecs describe-services --cluster $CLUSTER_NAME --services $SERVICE_NAME --region $REGION --query 'services[0].runningCount'"
