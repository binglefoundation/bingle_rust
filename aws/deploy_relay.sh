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
#   --redeploy-only        Update only the stack's image (all other parameters keep their
#                          previous values) instead of a full CloudFormation deploy
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
  echo "  --redeploy-only        Update only the stack's image, keeping all other parameters (requires existing stack)"
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

# Whether the stack exists decides how to treat its log group and whether a deploy may be a no-op.
STACK_EXISTS=0
if aws cloudformation describe-stacks --stack-name "$STACK_NAME" --region "$REGION" >/dev/null 2>&1; then
  STACK_EXISTS=1
fi
if [[ $REDEPLOY_ONLY -eq 1 && $STACK_EXISTS -eq 0 ]]; then
  echo "[deploy] ✗ --redeploy-only needs an existing stack, and '$STACK_NAME' does not exist." >&2
  exit 1
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

# Pin the task definition to the pushed image's digest, not its tag. Rebuilding under an unchanged
# .build_number re-pushes the same tag, so a tag reference leaves every stack parameter identical:
# CloudFormation reports "No changes to deploy" and the service keeps running the old image (#312).
# A digest reference makes any new image a parameter change, and so a new deployment.
IMAGE_DIGEST=$(aws ecr describe-images --repository-name "$REPO_NAME" --image-ids imageTag="$TAG" --region "$REGION" --query "imageDetails[0].imageDigest" --output text)
if [[ "$IMAGE_DIGEST" != sha256:* ]]; then
  echo "[deploy] ✗ Could not read the digest of the pushed image '$FULL_IMAGE_URI' (got '$IMAGE_DIGEST')." >&2
  exit 1
fi
IMAGE_REF="${ECR_URL}/${REPO_NAME}@${IMAGE_DIGEST}"
echo "[deploy] Image pinned by digest: $IMAGE_REF"

# 3) Deploy/Update

# The log group is DeletionPolicy: Retain, so after destroy_relay.sh it outlives its stack and a
# fresh create would fail with "already exists". Remove it only in that case — when the stack does
# not exist. On an update the group belongs to the running relay and must be left alone.
LOG_GROUP_NAME="/bingle/relay/${STACK_NAME}"
if [[ $STACK_EXISTS -eq 0 ]] && aws logs describe-log-groups --log-group-name-prefix "$LOG_GROUP_NAME" --region "$REGION" \
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

stack_resource_id() {
  aws cloudformation describe-stack-resource --stack-name "$STACK_NAME" --logical-resource-id "$1" --region "$REGION" --query "StackResourceDetail.PhysicalResourceId" --output text
}

# The id of the service's PRIMARY deployment. If it is the same after a stack deploy as before, the
# deploy did not restart the relay.
primary_deployment_id() {
  aws ecs describe-services --cluster "$(stack_resource_id ECSCluster)" --services "$(stack_resource_id RelayService)" --region "$REGION" --query "services[0].deployments[?status=='PRIMARY'] | [0].id" --output text
}

# Restart the service on its current task definition and wait for the new deployment to reach a
# steady state. Used when the stack update was a no-op, so a rebuild still ends with a fresh task.
force_new_deployment() {
  local cluster service deployment_id rollout
  service=$(stack_resource_id RelayService)
  cluster=$(stack_resource_id ECSCluster)
  echo "[deploy] Forcing new deployment for service '$service' in cluster '$cluster'..."
  deployment_id=$(aws ecs update-service --cluster "$cluster" --service "$service" --force-new-deployment --region "$REGION" --query "service.deployments[?status=='PRIMARY'] | [0].id" --output text)
  echo "[deploy] Waiting for deployment '$deployment_id' to reach a steady state..."
  if ! aws ecs wait services-stable --cluster "$cluster" --services "$service" --region "$REGION"; then
    show_relay_failure_logs
    exit 1
  fi
  # services-stable is also satisfied after the circuit breaker rolls back to the old deployment,
  # so check that the deployment we started is the one that completed.
  rollout=$(aws ecs describe-services --cluster "$cluster" --services "$service" --region "$REGION" --query "services[0].deployments[?id=='${deployment_id}'] | [0].rolloutState" --output text)
  if [[ "$rollout" != "COMPLETED" ]]; then
    echo "[deploy] ✗ Deployment '$deployment_id' did not complete (rolloutState: $rollout)." >&2
    show_relay_failure_logs
    exit 1
  fi
}

# Final check before reporting success: the service has a single, completed deployment whose task
# definition runs the image just pushed, with its desired task count running.
verify_service_image() {
  local cluster service state task_def running_image n_deployments rollout running desired
  service=$(stack_resource_id RelayService)
  cluster=$(stack_resource_id ECSCluster)
  state=$(aws ecs describe-services --cluster "$cluster" --services "$service" --region "$REGION" --query "services[0].[length(deployments), deployments[?status=='PRIMARY'] | [0].rolloutState, runningCount, desiredCount, taskDefinition]" --output text)
  read -r n_deployments rollout running desired task_def <<<"$state"
  running_image=$(aws ecs describe-task-definition --task-definition "$task_def" --region "$REGION" --query "taskDefinition.containerDefinitions[0].image" --output text)
  if [[ "$n_deployments" != "1" || "$rollout" != "COMPLETED" || "$running" != "$desired" || "$running_image" != "$IMAGE_REF" ]]; then
    echo "[deploy] ✗ Service '$service' is not running the pushed image in a steady state:" >&2
    echo "         deployments=$n_deployments rolloutState=$rollout running=$running/$desired" >&2
    echo "         image=$running_image" >&2
    echo "         expected=$IMAGE_REF" >&2
    exit 1
  fi
  echo "[deploy] Service '$service' is running $IMAGE_REF ($running/$desired tasks)."
}

if [[ $REDEPLOY_ONLY -eq 1 ]]; then
  # The task definition is pinned to an image digest, so restarting the service alone would rerun
  # the old image. Update just ImageUri on the stack, keeping every other parameter and the template.
  CURRENT_IMAGE=$(aws cloudformation describe-stacks --stack-name "$STACK_NAME" --region "$REGION" --query "Stacks[0].Parameters[?ParameterKey=='ImageUri'] | [0].ParameterValue" --output text)
  if [[ "$CURRENT_IMAGE" == "$IMAGE_REF" ]]; then
    echo "[deploy] Stack '$STACK_NAME' already references $IMAGE_REF; restarting the service on it."
    force_new_deployment
  else
    echo "[deploy] Updating the image of stack '$STACK_NAME'..."
    UPDATE_PARAMS=("ParameterKey=ImageUri,ParameterValue=$IMAGE_REF")
    for key in $(aws cloudformation describe-stacks --stack-name "$STACK_NAME" --region "$REGION" --query "Stacks[0].Parameters[?ParameterKey!='ImageUri'].ParameterKey" --output text); do
      UPDATE_PARAMS+=("ParameterKey=$key,UsePreviousValue=true")
    done
    aws cloudformation update-stack --stack-name "$STACK_NAME" --use-previous-template --capabilities CAPABILITY_IAM --region "$REGION" --parameters "${UPDATE_PARAMS[@]}" >/dev/null
    if ! aws cloudformation wait stack-update-complete --stack-name "$STACK_NAME" --region "$REGION"; then
      show_relay_failure_logs
      exit 1
    fi
  fi
else
  DEPLOYMENT_BEFORE=""
  if [[ $STACK_EXISTS -eq 1 ]]; then
    DEPLOYMENT_BEFORE=$(primary_deployment_id)
  fi
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
        ImageUri="$IMAGE_REF" \
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
        ImageUri="$IMAGE_REF" \
        Handle="$HANDLE" \
        Passphrase="$PASSPHRASE" \
        NatMode="$NAT_MODE" \
        NodeFile="$NODE_FILE" \
        CostTag="$COST_TAG"; then
      show_relay_failure_logs
      exit 1
    fi
  fi
  # `cloudformation deploy` exits 0 on an empty change set ("No changes to deploy"), and a change
  # that does not touch the task definition leaves the old task running too. Either way the
  # service's PRIMARY deployment is unchanged: restart it so the rebuild still takes effect.
  if [[ $STACK_EXISTS -eq 1 && "$(primary_deployment_id)" == "$DEPLOYMENT_BEFORE" ]]; then
    echo "[deploy] CloudFormation did not redeploy the service; restarting it."
    force_new_deployment
  fi
fi

verify_service_image

echo "[deploy] Success! Relay server deployed."
echo "[deploy] You can check the logs in CloudWatch Logs under group: /bingle/relay/${STACK_NAME}"
echo ""
echo "[deploy] To tail live task logs (run in another terminal after deploy):"
echo "  aws logs tail /bingle/relay/${STACK_NAME} --follow --region ${REGION}"
echo ""
echo "[deploy] To fetch logs from stopped/crashed tasks (useful when circuit breaker fires):"
echo "  aws logs filter-log-events --log-group-name /bingle/relay/${STACK_NAME} --region ${REGION} --start-time \$(date -d '1 hour ago' +%s000 2>/dev/null || date -v-1H +%s000) --output text | tail -100"
