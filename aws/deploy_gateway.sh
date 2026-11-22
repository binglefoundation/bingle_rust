#!/usr/bin/env bash
# aws/deploy_gateway.sh
#
# Purpose: build and deploy the Bingle email gateway (epic #63, issue #473): `bingle_cli logclient`
# and `scripts/mailer.sh` as one Fargate service in an existing relay stack's cluster, with the
# state file and Maildir on EFS (aws/gateway_stack.yaml). See SUPPORT_EMAIL.md.
#
# Usage:
#   aws/deploy_gateway.sh --relay-stack <name> --to <email> --from <address> [options]
#   aws/deploy_gateway.sh --relay-stack <name> --redeploy-only [options]
#
# Options:
#   --relay-stack <name>       (Required) The relay stack whose cluster, VPC and subnet to use
#   --to <email>               Where the emails go (required, except with --redeploy-only)
#   --from <address>           The verified SES address they come from (required, except with
#                              --redeploy-only)
#   --handle <handle>          The account handle, with --passphrase-secret
#   --passphrase-secret <arn>  Secrets Manager secret holding the account's recovery phrase, read
#                              on the first start to import the account into the state file. A
#                              new stack needs it; a later full deploy without it stops passing it.
#   --spam-limit <n>           Per-sender daily limit on forwarded messages (default: 20)
#   --node-file <path>         Algorand node file in the image (default: the relay stack's)
#   --stack-name <name>        Gateway stack name (default: bingle-gateway-<relay stack>)
#   --region <region>          AWS region (default: from aws configure)
#   --repo-name <name>         ECR repository name (default: bingle-gateway)
#   --redeploy-only            Update only the stack's image, keeping every other parameter
#   --tag <tag>                Image tag (default: latest, or from .build_number if it exists)
#   --no-build                 Deploy the local image <repo-name>:<tag> as built
#   --cost-tag <tag>           Cost allocation tag (default: bingle_dev)

set -euo pipefail

RELAY_STACK=""
STACK_NAME=""
TO_EMAIL=""
FROM_ADDRESS=""
HANDLE=""
PASSPHRASE_SECRET=""
SPAM_LIMIT="20"
NODE_FILE=""
REGION=$(aws configure get region || true)
REPO_NAME="bingle-gateway"
COST_TAG="bingle_dev"
REDEPLOY_ONLY=0
NO_BUILD=0
TAG="latest"
# Seconds verify_service_image waits for the ECS rollout to finish after CloudFormation does.
ROLLOUT_TIMEOUT=600

if [[ -f .build_number ]]; then
  TAG=$(cat .build_number)
fi

usage() {
  echo "Usage: $0 --relay-stack <name> --to <email> --from <address> [options]"
  echo "       $0 --relay-stack <name> --redeploy-only [options]"
  echo "Options:"
  echo "  --relay-stack <name>       (Required) The relay stack whose cluster, VPC and subnet to use"
  echo "  --to <email>               Where the emails go (required, except with --redeploy-only)"
  echo "  --from <address>           The verified SES address they come from (required, except with --redeploy-only)"
  echo "  --handle <handle>          The account handle, with --passphrase-secret"
  echo "  --passphrase-secret <arn>  Secret holding the account's recovery phrase, for the first start (a new stack needs it)"
  echo "  --spam-limit <n>           Per-sender daily limit (default: $SPAM_LIMIT)"
  echo "  --node-file <path>         Algorand node file in the image (default: the relay stack's)"
  echo "  --stack-name <name>        Gateway stack name (default: bingle-gateway-<relay stack>)"
  echo "  --region <region>          AWS region (default: $REGION)"
  echo "  --repo-name <name>         ECR repository name (default: $REPO_NAME)"
  echo "  --redeploy-only            Update only the stack's image, keeping every other parameter"
  echo "  --tag <tag>                Image tag (default: $TAG)"
  echo "  --no-build                 Deploy the local image <repo-name>:<tag> instead of building it"
  echo "  --cost-tag <tag>           Cost allocation tag (default: $COST_TAG)"
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --relay-stack) RELAY_STACK="$2"; shift 2 ;;
    --to) TO_EMAIL="$2"; shift 2 ;;
    --from) FROM_ADDRESS="$2"; shift 2 ;;
    --handle) HANDLE="$2"; shift 2 ;;
    --passphrase-secret) PASSPHRASE_SECRET="$2"; shift 2 ;;
    --spam-limit) SPAM_LIMIT="$2"; shift 2 ;;
    --node-file) NODE_FILE="$2"; shift 2 ;;
    --stack-name) STACK_NAME="$2"; shift 2 ;;
    --region) REGION="$2"; shift 2 ;;
    --repo-name) REPO_NAME="$2"; shift 2 ;;
    --redeploy-only) REDEPLOY_ONLY=1; shift ;;
    --tag) TAG="$2"; shift 2 ;;
    --no-build) NO_BUILD=1; shift ;;
    --cost-tag) COST_TAG="$2"; shift 2 ;;
    -h|--help) usage ;;
    *) echo "Unknown argument: $1"; usage ;;
  esac
done

fail() {
  echo "[gateway] ✗ $*" >&2
  exit 1
}

[[ -n "$RELAY_STACK" ]] || { echo "Error: --relay-stack is required."; usage; }
[[ -n "$REGION" ]] || fail "no region: pass --region or set one with aws configure"
if [[ $REDEPLOY_ONLY -eq 0 ]]; then
  [[ -n "$TO_EMAIL" && -n "$FROM_ADDRESS" ]] || { echo "Error: --to and --from are required, except with --redeploy-only."; usage; }
  [[ "$SPAM_LIMIT" =~ ^[0-9]*$ ]] || fail "--spam-limit must be a whole number, got '$SPAM_LIMIT'"
  if [[ -n "$PASSPHRASE_SECRET" && -z "$HANDLE" ]] || [[ -z "$PASSPHRASE_SECRET" && -n "$HANDLE" ]]; then
    fail "--handle and --passphrase-secret go together"
  fi
elif [[ -n "$TO_EMAIL$FROM_ADDRESS$HANDLE$PASSPHRASE_SECRET$NODE_FILE" ]]; then
  fail "--redeploy-only changes only the image; drop the other settings or do a full deploy"
fi
STACK_NAME="${STACK_NAME:-bingle-gateway-${RELAY_STACK}}"

relay_resource() {
  aws cloudformation describe-stack-resource --stack-name "$RELAY_STACK" --logical-resource-id "$1" --region "$REGION" --query "StackResourceDetail.PhysicalResourceId" --output text
}

stack_resource() {
  aws cloudformation describe-stack-resource --stack-name "$STACK_NAME" --logical-resource-id "$1" --region "$REGION" --query "StackResourceDetail.PhysicalResourceId" --output text
}

# The relay stack's network and cluster (both relay templates use these logical ids).
CLUSTER=$(relay_resource ECSCluster) || fail "relay stack '$RELAY_STACK' not found in $REGION"
VPC_ID=$(relay_resource VPC)
SUBNET_ID=$(relay_resource Subnet)
if [[ -z "$NODE_FILE" ]]; then
  NODE_FILE=$(aws cloudformation describe-stacks --stack-name "$RELAY_STACK" --region "$REGION" --query "Stacks[0].Parameters[?ParameterKey=='NodeFile'] | [0].ParameterValue" --output text)
  [[ "$NODE_FILE" == /app/* ]] || fail "could not read the relay stack's NodeFile (got '$NODE_FILE'); pass --node-file"
fi
echo "[gateway] Relay stack '$RELAY_STACK': cluster $CLUSTER, node file $NODE_FILE"

STACK_EXISTS=0
if STACK_STATUS=$(aws cloudformation describe-stacks --stack-name "$STACK_NAME" --region "$REGION" --query "Stacks[0].StackStatus" --output text 2>/dev/null); then
  STACK_EXISTS=1
fi
# A first deploy that failed leaves a stack that can only be deleted. Its EFS volume is retained,
# and is empty unless logclient got as far as saving the state file.
if [[ "${STACK_STATUS:-}" == "ROLLBACK_COMPLETE" ]]; then
  fail "stack '$STACK_NAME' is ROLLBACK_COMPLETE from a failed first deploy; delete it (aws cloudformation delete-stack --stack-name $STACK_NAME --region $REGION) and the EFS volume it retained, named $STACK_NAME-data, then deploy again"
fi
if [[ $REDEPLOY_ONLY -eq 1 && $STACK_EXISTS -eq 0 ]]; then
  fail "--redeploy-only needs an existing stack, and '$STACK_NAME' does not exist"
fi
if [[ $STACK_EXISTS -eq 0 && -z "$PASSPHRASE_SECRET" ]]; then
  fail "a new gateway has an empty state file, so the first deploy needs --handle and --passphrase-secret"
fi

# 1) Build the image, unless the caller already built it
if [[ $NO_BUILD -eq 1 ]]; then
  docker image inspect "$REPO_NAME:$TAG" >/dev/null 2>&1 || fail "--no-build needs the local image '$REPO_NAME:$TAG'; build it with scripts/build_cli_image.sh --stage gateway --platform linux/arm64 --tag $REPO_NAME:$TAG"
  echo "[gateway] Using the local image $REPO_NAME:$TAG (--no-build)"
else
  echo "[gateway] Building the gateway image for linux/arm64..."
  bash scripts/build_cli_image.sh --stage gateway --platform linux/arm64 --tag "$REPO_NAME:$TAG"
fi

# 2) Push it to ECR and pin it by digest, so any new image is a stack change (as for relays, #312)
ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
ECR_URL="${ACCOUNT_ID}.dkr.ecr.${REGION}.amazonaws.com"
echo "[gateway] Ensuring ECR repository '$REPO_NAME' exists..."
aws ecr describe-repositories --repository-names "$REPO_NAME" --region "$REGION" >/dev/null 2>&1 || aws ecr create-repository --repository-name "$REPO_NAME" --region "$REGION" >/dev/null
aws ecr get-login-password --region "$REGION" | docker login --username AWS --password-stdin "$ECR_URL"
docker tag "$REPO_NAME:$TAG" "${ECR_URL}/${REPO_NAME}:${TAG}"
docker push "${ECR_URL}/${REPO_NAME}:${TAG}"
IMAGE_DIGEST=$(aws ecr describe-images --repository-name "$REPO_NAME" --image-ids imageTag="$TAG" --region "$REGION" --query "imageDetails[0].imageDigest" --output text)
[[ "$IMAGE_DIGEST" == sha256:* ]] || fail "could not read the digest of the pushed image (got '$IMAGE_DIGEST')"
IMAGE_REF="${ECR_URL}/${REPO_NAME}@${IMAGE_DIGEST}"
echo "[gateway] Image pinned by digest: $IMAGE_REF"

# 3) Deploy
LOG_GROUP_NAME="/bingle/gateway/${STACK_NAME}"

# The last container log lines, for a failed deploy: the CloudFormation error is usually just the
# circuit breaker, and the cause is in the logs (the log group is retained through a rollback).
show_failure_logs() {
  local events
  echo "" >&2
  echo "[gateway] ✗ Deploy failed. Last container log lines from '$LOG_GROUP_NAME':" >&2
  events=$(aws logs filter-log-events --log-group-name "$LOG_GROUP_NAME" --region "$REGION" --start-time $(( ($(date +%s) - 900) * 1000 )) --query "events[].message" --output text 2>/dev/null || true)
  if [[ -z "$events" ]]; then
    echo "[gateway] None: the task likely never started (an image, secret or EFS mount error). Check the stopped task's reason:" >&2
    echo "         aws ecs describe-services --cluster $CLUSTER --services $STACK_NAME --region $REGION --query 'services[0].events[:5]'" >&2
    return
  fi
  tail -n 30 <<<"$events" >&2
}

# The log group is retained when a stack is deleted, so a fresh create would find it in the way.
if [[ $STACK_EXISTS -eq 0 ]] && aws logs describe-log-groups --log-group-name-prefix "$LOG_GROUP_NAME" --region "$REGION" --query "logGroups[?logGroupName=='${LOG_GROUP_NAME}'].logGroupName" --output text 2>/dev/null | grep -q .; then
  echo "[gateway] Removing the retained log group '$LOG_GROUP_NAME' from an earlier stack..."
  aws logs delete-log-group --log-group-name "$LOG_GROUP_NAME" --region "$REGION" || true
fi

primary_deployment_id() {
  aws ecs describe-services --cluster "$CLUSTER" --services "$(stack_resource Service)" --region "$REGION" --query "services[0].deployments[?status=='PRIMARY'] | [0].id" --output text
}

DEPLOYMENT_BEFORE=""
if [[ $STACK_EXISTS -eq 1 ]]; then
  DEPLOYMENT_BEFORE=$(primary_deployment_id)
fi

if [[ $REDEPLOY_ONLY -eq 1 ]]; then
  echo "[gateway] Updating the image of stack '$STACK_NAME'..."
  UPDATE_PARAMS=("ParameterKey=ImageUri,ParameterValue=$IMAGE_REF")
  for key in $(aws cloudformation describe-stacks --stack-name "$STACK_NAME" --region "$REGION" --query "Stacks[0].Parameters[?ParameterKey!='ImageUri'].ParameterKey" --output text); do
    UPDATE_PARAMS+=("ParameterKey=$key,UsePreviousValue=true")
  done
  CURRENT_IMAGE=$(aws cloudformation describe-stacks --stack-name "$STACK_NAME" --region "$REGION" --query "Stacks[0].Parameters[?ParameterKey=='ImageUri'] | [0].ParameterValue" --output text)
  if [[ "$CURRENT_IMAGE" != "$IMAGE_REF" ]]; then
    aws cloudformation update-stack --stack-name "$STACK_NAME" --use-previous-template --capabilities CAPABILITY_IAM --region "$REGION" --parameters "${UPDATE_PARAMS[@]}" >/dev/null
    aws cloudformation wait stack-update-complete --stack-name "$STACK_NAME" --region "$REGION" || { show_failure_logs; exit 1; }
  fi
else
  echo "[gateway] Deploying CloudFormation stack '$STACK_NAME'..."
  aws cloudformation deploy --stack-name "$STACK_NAME" --template-file aws/gateway_stack.yaml --capabilities CAPABILITY_IAM --region "$REGION" --tags CostCenter="$COST_TAG" --parameter-overrides ClusterName="$CLUSTER" VpcId="$VPC_ID" SubnetId="$SUBNET_ID" ImageUri="$IMAGE_REF" ToEmail="$TO_EMAIL" FromAddress="$FROM_ADDRESS" SpamLimit="$SPAM_LIMIT" Handle="$HANDLE" PassphraseSecretArn="$PASSPHRASE_SECRET" NodeFile="$NODE_FILE" CostTag="$COST_TAG" || { show_failure_logs; exit 1; }
fi

# An unchanged stack leaves the old task running; restart it so a rebuild still takes effect.
if [[ $STACK_EXISTS -eq 1 && "$(primary_deployment_id)" == "$DEPLOYMENT_BEFORE" ]]; then
  echo "[gateway] CloudFormation did not redeploy the service; restarting it."
  aws ecs update-service --cluster "$CLUSTER" --service "$(stack_resource Service)" --force-new-deployment --region "$REGION" >/dev/null
fi

# 4) Check the service runs the pushed image in a steady state, with one completed deployment.
SERVICE=$(stack_resource Service)
deadline=$(( $(date +%s) + ROLLOUT_TIMEOUT ))
while :; do
  read -r n_deployments rollout running desired task_def <<<"$(aws ecs describe-services --cluster "$CLUSTER" --services "$SERVICE" --region "$REGION" --query "services[0].[length(deployments), deployments[?status=='PRIMARY'] | [0].rolloutState, runningCount, desiredCount, taskDefinition]" --output text)"
  [[ "$rollout" == "IN_PROGRESS" || "$n_deployments" != "1" ]] && [[ $(date +%s) -lt $deadline ]] || break
  echo "[gateway] Rollout $rollout ($n_deployments deployments); checking again in 15s..."
  sleep 15
done
images=$(aws ecs describe-task-definition --task-definition "$task_def" --region "$REGION" --query "taskDefinition.containerDefinitions[].image" --output text | tr '\t' '\n' | sort -u)
if [[ "$n_deployments" != "1" || "$rollout" != "COMPLETED" || "$running" != "$desired" || "$images" != "$IMAGE_REF" ]]; then
  echo "[gateway] ✗ Service '$SERVICE' is not running the pushed image in a steady state:" >&2
  echo "         deployments=$n_deployments rolloutState=$rollout running=$running/$desired" >&2
  echo "         images=$images" >&2
  echo "         expected=$IMAGE_REF" >&2
  show_failure_logs
  exit 1
fi

echo "[gateway] Success: '$SERVICE' is running $IMAGE_REF."
if [[ -n "$PASSPHRASE_SECRET" ]]; then
  echo "[gateway] Once the logs show 'logclient: receiving as', the account is in the state file on EFS:"
  echo "          redeploy without --handle/--passphrase-secret, then delete the secret."
fi
echo "[gateway] To follow the logs:"
echo "  aws logs tail $LOG_GROUP_NAME --follow --region $REGION"
