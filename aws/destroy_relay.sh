#!/usr/bin/env bash
# aws/destroy_relay.sh
#
# Purpose: Shut down the ECS tasks and delete the CloudFormation stack (and cluster) for the Bingle Relay server.
#
# Usage:
#   aws/destroy_relay.sh [options]
#
# Options:
#   --stack-name <name>    CloudFormation stack name (default: bingle-relay)
#   --region <region>      AWS region (default: from aws configure)
#   --delete-repo          Also delete the ECR repository

set -euo pipefail

# Default values
STACK_NAME="bingle-relay"
REGION=$(aws configure get region)
DELETE_REPO=0
REPO_NAME="bingle-relay"

usage() {
  echo "Usage: $0 [options]"
  echo "Options:"
  echo "  --stack-name <name>    CloudFormation stack name (default: $STACK_NAME)"
  echo "  --region <region>      AWS region (default: $REGION)"
  echo "  --delete-repo          Also delete the ECR repository '$REPO_NAME'"
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --stack-name) STACK_NAME="$2"; shift 2 ;;
    --region) REGION="$2"; shift 2 ;;
    --delete-repo) DELETE_REPO=1; shift ;;
    *) echo "Unknown argument: $1"; usage ;;
  esac
done

echo "[destroy] Checking if stack '$STACK_NAME' exists in region '$REGION'..."
if ! aws cloudformation describe-stacks --stack-name "$STACK_NAME" --region "$REGION" >/dev/null 2>&1; then
  echo "[destroy] Stack '$STACK_NAME' not found. Nothing to delete."
  exit 0
fi

echo "[destroy] Deleting CloudFormation stack '$STACK_NAME'..."
echo "[destroy] This will shut down all tasks and delete the cluster and other resources."
aws cloudformation delete-stack --stack-name "$STACK_NAME" --region "$REGION"

echo "[destroy] Waiting for stack deletion to complete (this may take a few minutes)..."
aws cloudformation wait stack-delete-complete --stack-name "$STACK_NAME" --region "$REGION"

if [[ $DELETE_REPO -eq 1 ]]; then
  echo "[destroy] Deleting ECR repository '$REPO_NAME'..."
  aws ecr delete-repository --repository-name "$REPO_NAME" --region "$REGION" --force || echo "[destroy] Warning: Failed to delete ECR repo '$REPO_NAME' (maybe it was already deleted?)"
fi

echo "[destroy] Success! Relay stack '$STACK_NAME' has been deleted."
