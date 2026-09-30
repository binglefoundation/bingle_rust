#!/usr/bin/env bash
# aws/destroy_relay.sh
#
# Purpose: Shut down the ECS tasks and delete the CloudFormation stack (and cluster) for the Bingle Relay server.
#
# Usage:
#   aws/destroy_relay.sh (--handle <handle> | --stack-name <name>) [options]
#
# Options:
#   --handle <handle>      Relay handle; the stack name is derived as 'bingle-relay-<handle>'
#   --stack-name <name>    CloudFormation stack name (overrides --handle)
#   --region <region>      AWS region (default: from aws configure)
#   --delete-repo          Also delete the ECR repository

set -euo pipefail

# Same handle -> stack name convention as deploy_relay.sh (keep the two in sync): 'bingle-relay-
# <handle>' with non-alphanumeric characters collapsed to single hyphens.
stack_name_for_handle() {
  local h
  h=$(printf '%s' "$1" | tr -c 'A-Za-z0-9' '-' | tr -s '-' | sed 's/^-//; s/-$//')
  printf 'bingle-relay-%s' "$h"
}

# Default values
STACK_NAME=""
HANDLE=""
REGION=$(aws configure get region)
DELETE_REPO=0
REPO_NAME="bingle-relay"

usage() {
  echo "Usage: $0 (--handle <handle> | --stack-name <name>) [options]"
  echo "Options:"
  echo "  --handle <handle>      Relay handle; stack name derived as 'bingle-relay-<handle>'"
  echo "  --stack-name <name>    CloudFormation stack name (overrides --handle)"
  echo "  --region <region>      AWS region (default: $REGION)"
  echo "  --delete-repo          Also delete the ECR repository '$REPO_NAME'"
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --handle) HANDLE="$2"; shift 2 ;;
    --stack-name) STACK_NAME="$2"; shift 2 ;;
    --region) REGION="$2"; shift 2 ;;
    --delete-repo) DELETE_REPO=1; shift ;;
    *) echo "Unknown argument: $1"; usage ;;
  esac
done

if [[ -z "$STACK_NAME" ]]; then
  if [[ -z "$HANDLE" ]]; then
    echo "Error: provide --handle <handle> or --stack-name <name>."
    usage
  fi
  STACK_NAME=$(stack_name_for_handle "$HANDLE")
fi

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
