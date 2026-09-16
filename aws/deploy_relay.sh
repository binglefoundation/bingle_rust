#!/usr/bin/env bash
# aws/deploy_relay.sh
#
# Purpose: Build and deploy the Bingle Relay server to AWS ECS.
#
# Usage:
#   aws/deploy_relay.sh --handle <handle> --passphrase <passphrase> [options]
#
# Options:
#   --stack-name <name>    CloudFormation stack name (default: derived from the handle,
#                          'bingle-relay-<handle>' — see stack_name_for_handle)
#   --instance-type <type> EC2 instance type (default: t4g.nano, ignored in --express)
#   --port <port>          UDP port (default: 12121)
#   --nat-mode <mode>      Direct|Full|Restricted (default: Direct)
#   --region <region>      AWS region (default: from aws configure)
#   --repo-name <name>     ECR repository name (default: bingle-relay)
#   --redeploy-only        Skip CloudFormation and just update the ECS service
#   --tag <tag>            Image tag (default: latest, or from .build_number if exists)
#   --express              Use simplified Fargate-based deployment (faster, cheaper for small loads)

set -euo pipefail

# Derive the CloudFormation stack name from a relay handle: 'bingle-relay-<handle>', with any
# character that is not a letter or digit collapsed to a single hyphen (CFN stack names allow only
# [A-Za-z0-9-] and must start with a letter — the 'bingle-relay-' prefix guarantees that). Making
# the stack name a deterministic function of the handle means redeploying a handle updates its one
# stack in place, instead of spawning a parallel stack — and a fresh VPC each time — under an
# ad-hoc name (which is how we exhausted the region's VPC limit).
stack_name_for_handle() {
  local h
  h=$(printf '%s' "$1" | tr -c 'A-Za-z0-9' '-' | tr -s '-' | sed 's/^-//; s/-$//')
  printf 'bingle-relay-%s' "$h"
}

# Default values
# STACK_NAME left empty: derived from --handle after parsing, unless --stack-name overrides it.
STACK_NAME=""
INSTANCE_TYPE="t4g.micro"
PORT="12121"
NAT_MODE="Direct"
NODE_FILE="/app/nodely_staging_testnet_node.json"
REPO_NAME="bingle-relay"
REGION=$(aws configure get region)
COST_TAG="bingle_dev"
HANDLE=""
PASSPHRASE=""
REDEPLOY_ONLY=0
TAG="latest"
EXPRESS=0

if [[ -f .build_number ]]; then
  TAG=$(cat .build_number)
fi

usage() {
  echo "Usage: $0 --handle <handle> --passphrase <passphrase> [options]"
  echo "Options:"
  echo "  --handle <handle>      (Required) Relay handle"
  echo "  --passphrase <pass>    (Required) Relay passphrase"
  echo "  --stack-name <name>    CloudFormation stack name (default: derived from handle, 'bingle-relay-<handle>')"
  echo "  --instance-type <type> EC2 instance type (default: $INSTANCE_TYPE)"
  echo "  --port <port>          UDP port (default: $PORT)"
  echo "  --nat-mode <mode>      Direct|Full|Restricted (default: $NAT_MODE)"
  echo "  --node-file <path>     Path to node configuration file (default: $NODE_FILE)"
  echo "  --region <region>      AWS region (default: $REGION)"
  echo "  --repo-name <name>     ECR repository name (default: $REPO_NAME)"
  echo "  --redeploy-only        Skip CloudFormation and just update the ECS service (requires existing stack)"
  echo "  --tag <tag>            Image tag (default: $TAG)"
  echo "  --express              Use simplified Fargate-based deployment (faster)"
  echo "  --cost-tag <tag>       Cost allocation tag (default: $COST_TAG)"
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --handle) HANDLE="$2"; shift 2 ;;
    --passphrase) PASSPHRASE="$2"; shift 2 ;;
    --stack-name) STACK_NAME="$2"; shift 2 ;;
    --instance-type) INSTANCE_TYPE="$2"; shift 2 ;;
    --port) PORT="$2"; shift 2 ;;
    --nat-mode) NAT_MODE="$2"; shift 2 ;;
    --node-file) NODE_FILE="$2"; shift 2 ;;
    --region) REGION="$2"; shift 2 ;;
    --repo-name) REPO_NAME="$2"; shift 2 ;;
    --redeploy-only) REDEPLOY_ONLY=1; shift ;;
    --tag) TAG="$2"; shift 2 ;;
    --express) EXPRESS=1; shift ;;
    --cost-tag) COST_TAG="$2"; shift 2 ;;
    *) echo "Unknown argument: $1"; usage ;;
  esac
done

if [[ -z "$HANDLE" || -z "$PASSPHRASE" ]]; then
  echo "Error: --handle and --passphrase are required."
  usage
fi

# Derive the stack name from the handle unless the caller pinned one with --stack-name. Same handle
# => same stack => in-place update rather than a new parallel stack (and VPC) each deploy.
if [[ -z "$STACK_NAME" ]]; then
  STACK_NAME=$(stack_name_for_handle "$HANDLE")
  echo "[deploy] Stack name derived from handle '$HANDLE': $STACK_NAME"
fi

# 1) Build the Docker image
echo "[deploy] Building Docker image for linux/arm64..."
# We use the existing build script to ensure consistency
# It builds target/aarch64-unknown-linux-musl/debug/bingle_cli
bash scripts/build_cli_image.sh --platform linux/arm64 --tag "$REPO_NAME:$TAG"

# 2) Prepare ECR
ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
ECR_URL="${ACCOUNT_ID}.dkr.ecr.${REGION}.amazonaws.com"
FULL_IMAGE_URI="${ECR_URL}/${REPO_NAME}:${TAG}"

echo "[deploy] Ensuring ECR repository '$REPO_NAME' exists..."
aws ecr describe-repositories --repository-names "$REPO_NAME" --region "$REGION" >/dev/null 2>&1 || \
  aws ecr create-repository --repository-name "$REPO_NAME" --region "$REGION"

echo "[deploy] Logging into ECR..."
aws ecr get-login-password --region "$REGION" | docker login --username AWS --password-stdin "$ECR_URL"

echo "[deploy] Pushing image to $FULL_IMAGE_URI..."
docker tag "$REPO_NAME:$TAG" "$FULL_IMAGE_URI"
docker push "$FULL_IMAGE_URI"

# Also tag as latest
LATEST_IMAGE_URI="${ECR_URL}/${REPO_NAME}:latest"
docker tag "$REPO_NAME:$TAG" "$LATEST_IMAGE_URI"
docker push "$LATEST_IMAGE_URI"

# 3) Deploy/Update

# If the log group was retained from a previous stack deletion, remove it so
# CloudFormation can recreate it cleanly.  This avoids "already exists" errors
# while still preserving logs between destroy and the next deploy.
LOG_GROUP_NAME="/bingle/relay/${STACK_NAME}"
if aws logs describe-log-groups --log-group-name-prefix "$LOG_GROUP_NAME" --region "$REGION" \
    --query "logGroups[?logGroupName=='${LOG_GROUP_NAME}']" --output text 2>/dev/null | grep -q "$LOG_GROUP_NAME"; then
  echo "[deploy] Removing retained log group '$LOG_GROUP_NAME' from previous deployment..."
  aws logs delete-log-group --log-group-name "$LOG_GROUP_NAME" --region "$REGION" || true
fi

# On a failed deploy the CloudFormation error is just the symptom (typically the opaque "ECS
# Deployment Circuit Breaker was triggered"). The real cause is usually in the relay container's
# own logs, which survive the circuit-breaker rollback because the log group is DeletionPolicy:
# Retain. Pull them and surface the prominent `RELAY STARTUP ABORTED` banner (or a tail + hint when
# the container never wrote a line, e.g. it was never placed on an instance) so the operator sees
# the cause without going digging in CloudWatch.
show_relay_failure_logs() {
  echo "" >&2
  echo "[deploy] ✗ Deploy failed. Fetching relay container logs from '$LOG_GROUP_NAME'..." >&2
  local start_ms events
  start_ms=$(( ($(date +%s) - 900) * 1000 ))
  events=$(aws logs filter-log-events \
    --log-group-name "$LOG_GROUP_NAME" \
    --region "$REGION" \
    --start-time "$start_ms" \
    --query "events[].message" --output text 2>/dev/null || true)
  if [[ -z "$events" ]]; then
    echo "[deploy] No container logs found — the task likely never started (no container instance" >&2
    echo "         registered, or an image/secret pull error), so nothing was written. Inspect the" >&2
    echo "         ECS service events and stopped-task reasons:" >&2
    echo "         aws ecs describe-services --cluster ${STACK_NAME}-Cluster --services <svc> --region ${REGION} --query 'services[0].events'" >&2
    return
  fi
  if grep -q "RELAY STARTUP ABORTED" <<<"$events"; then
    echo "" >&2
    echo "[deploy] The relay started but refused to run — cause below (from the container logs):" >&2
    echo "" >&2
    grep -A 8 "RELAY STARTUP ABORTED" <<<"$events" | tail -n +1 | head -n 20 >&2
    echo "" >&2
  else
    echo "[deploy] Last relay container log lines:" >&2
    tail -n 30 <<<"$events" >&2
  fi
}

if [[ $REDEPLOY_ONLY -eq 1 ]]; then
  echo "[deploy] Redeploying ECS service '$STACK_NAME' with new image..."
  # When using --redeploy-only, we assume the stack exists and we just want to update the ECS service
  # This is faster than CloudFormation if only the code changed.
  
  # Try to find the cluster and service names from the stack.
  SERVICE_NAME=$(aws cloudformation describe-stack-resource --stack-name "$STACK_NAME" --logical-resource-id RelayService --region "$REGION" --query "StackResourceDetail.PhysicalResourceId" --output text)
  CLUSTER_NAME=$(aws cloudformation describe-stack-resource --stack-name "$STACK_NAME" --logical-resource-id ECSCluster --region "$REGION" --query "StackResourceDetail.PhysicalResourceId" --output text)

  echo "[deploy] Forcing new deployment for service '$SERVICE_NAME' in cluster '$CLUSTER_NAME'..."
  aws ecs update-service --cluster "$CLUSTER_NAME" --service "$SERVICE_NAME" --force-new-deployment --region "$REGION"
else
  if [[ $EXPRESS -eq 1 ]]; then
    TEMPLATE_FILE="aws/relay_express.yaml"
    echo "[deploy] Deploying CloudFormation stack '$STACK_NAME' (EXPRESS MODE)..."
    if ! aws cloudformation deploy \
      --stack-name "$STACK_NAME" \
      --template-file "$TEMPLATE_FILE" \
      --capabilities CAPABILITY_IAM \
      --region "$REGION" \
      --parameter-overrides \
        RelayPort="$PORT" \
        ImageUri="$FULL_IMAGE_URI" \
        Handle="$HANDLE" \
        Passphrase="$PASSPHRASE" \
        NatMode="$NAT_MODE" \
        NodeFile="$NODE_FILE" \
        CostTag="$COST_TAG"; then
      show_relay_failure_logs
      exit 1
    fi
  else
    TEMPLATE_FILE="aws/relay_stack.yaml"
    echo "[deploy] Deploying CloudFormation stack '$STACK_NAME'..."
    if ! aws cloudformation deploy \
      --stack-name "$STACK_NAME" \
      --template-file "$TEMPLATE_FILE" \
      --capabilities CAPABILITY_IAM \
      --region "$REGION" \
      --parameter-overrides \
        InstanceType="$INSTANCE_TYPE" \
        RelayPort="$PORT" \
        ImageUri="$FULL_IMAGE_URI" \
        Handle="$HANDLE" \
        Passphrase="$PASSPHRASE" \
        NatMode="$NAT_MODE" \
        NodeFile="$NODE_FILE" \
        CostTag="$COST_TAG"; then
      show_relay_failure_logs
      exit 1
    fi
  fi
fi

echo "[deploy] Success! Relay server deployed."
echo "[deploy] You can check the logs in CloudWatch Logs under group: /bingle/relay/${STACK_NAME}"
echo ""
echo "[deploy] To tail live task logs (run in another terminal after deploy):"
echo "  aws logs tail /bingle/relay/${STACK_NAME} --follow --region ${REGION}"
echo ""
echo "[deploy] To fetch logs from stopped/crashed tasks (useful when circuit breaker fires):"
echo "  aws logs filter-log-events --log-group-name /bingle/relay/${STACK_NAME} --region ${REGION} --start-time \$(date -d '1 hour ago' +%s000 2>/dev/null || date -v-1H +%s000) --output text | tail -100"
