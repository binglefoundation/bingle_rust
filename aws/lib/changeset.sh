# aws/lib/changeset.sh: apply the checked-out CloudFormation template to an existing stack through
# a change set, changing only the image parameter (issue #528). Sourced by aws/deploy_relay.sh and
# aws/deploy_gateway.sh on their --redeploy-only path, which scripts/roll_relays.sh uses.
#
# A redeploy used to run `update-stack --use-previous-template`, so a template change never reached
# a running stack through a roll. Now each redeploy:
#   1. creates a change set from the template in this checkout, keeping every stack parameter the
#      template still has (UsePreviousValue) except the image;
#   2. prints its resource changes and classifies them (changeset_classify);
#   3. executes it without asking when every change is in the expected set (the task definition,
#      the service picking it up), and otherwise asks y/N, unless --yes was given.
# An empty change set is "nothing to do", not a failure. With --dry-run the change set is printed
# and deleted, and nothing is executed.
#
# Needs aws and python3. Callers set CS_PREFIX (the log prefix, e.g. "[deploy]") before calling.
# Bash 3.2 compatible (no associative arrays, no mapfile).

CS_PREFIX=${CS_PREFIX:-"[changeset]"}

# changeset_classify <describe-change-set json file>
# Prints one word on the first line, then one line per reason:
#   empty   the change set has no changes (CloudFormation fails such a set with "didn't contain
#           changes"); nothing to do
#   safe    every change is in the expected set, so it may run unattended
#   review  something else changes; each following line names one such change
#   failed  the change set could not be created; the following line is CloudFormation's reason
# The expected set: an AWS::ECS::TaskDefinition modified (always a replacement), unless it changes
# its roles or network mode; an AWS::ECS::Service modified in place, unless it changes networking,
# load balancing, service discovery or its role. Anything else (IAM, security groups, VPC, a launch
# template, a log group, any added or removed resource, any other replacement) needs review.
changeset_classify() {
  python3 - "$1" <<'PY'
import json, sys

with open(sys.argv[1]) as f:
    cs = json.load(f)

status = cs.get("Status", "")
reason = cs.get("StatusReason") or ""
changes = cs.get("Changes") or []
EMPTY_REASONS = ("didn't contain changes", "No updates are to be performed")

if status == "FAILED":
    if any(r in reason for r in EMPTY_REASONS):
        print("empty")
    else:
        print("failed")
        print(reason or "the change set failed with no reason given")
    sys.exit(0)
if status != "CREATE_COMPLETE":
    print("failed")
    print(f"the change set is {status or 'in an unknown state'}: {reason}")
    sys.exit(0)
if not changes:
    print("empty")
    sys.exit(0)

TASK_DEF_REVIEW = {"TaskRoleArn", "ExecutionRoleArn", "NetworkMode"}
SERVICE_REVIEW = {"NetworkConfiguration", "LoadBalancers", "Role", "ServiceRegistries",
                  "ServiceConnectConfiguration", "VpcLatticeConfigurations"}

reasons = []
for c in changes:
    rc = c.get("ResourceChange") or {}
    if c.get("Type") != "Resource" or not rc:
        reasons.append(f"an unrecognised change of type {c.get('Type')}")
        continue
    action = rc.get("Action", "?")
    rtype = rc.get("ResourceType", "?")
    lid = rc.get("LogicalResourceId", "?")
    replacement = rc.get("Replacement") or "False"
    props = {d.get("Target", {}).get("Name") for d in rc.get("Details") or []
             if d.get("Target", {}).get("Attribute") == "Properties"}
    if rtype == "AWS::ECS::TaskDefinition" and action == "Modify":
        bad = sorted(props & TASK_DEF_REVIEW)
        if bad:
            reasons.append(f"{lid} ({rtype}) changes {', '.join(bad)}")
        continue
    if rtype == "AWS::ECS::Service" and action == "Modify":
        if replacement != "False":
            reasons.append(f"{lid} ({rtype}) would be replaced (Replacement: {replacement})")
        bad = sorted(props & SERVICE_REVIEW)
        if bad:
            reasons.append(f"{lid} ({rtype}) changes {', '.join(bad)}")
        continue
    extra = f", Replacement: {replacement}" if action == "Modify" else ""
    reasons.append(f"{action} {lid} ({rtype}{extra})")

print("review" if reasons else "safe")
for r in reasons:
    print(r)
PY
}

# changeset_print <describe-change-set json file>: one line per resource change.
changeset_print() {
  python3 - "$1" <<'PY'
import json, sys

with open(sys.argv[1]) as f:
    cs = json.load(f)
changes = cs.get("Changes") or []
if not changes:
    print("    (no resource changes)")
for c in changes:
    rc = c.get("ResourceChange") or {}
    props = sorted({d.get("Target", {}).get("Name") or d.get("Target", {}).get("Attribute", "?")
                    for d in rc.get("Details") or []})
    replacement = rc.get("Replacement")
    repl = f"replace: {replacement}" if rc.get("Action") == "Modify" and replacement else ""
    print("    {:<7} {:<28} {:<34} {:<20} {}".format(
        rc.get("Action", "?"), rc.get("LogicalResourceId", "?"), rc.get("ResourceType", "?"),
        repl, ", ".join(props)).rstrip())
PY
}

# changeset_parameters <template summary json> <describe-stacks json> <image key> <image value> <out file>
# Writes the --parameters JSON for the change set: the image key gets the new value, every other
# parameter the template declares keeps the stack's value (UsePreviousValue). A parameter the stack
# has but the template dropped is left out (CloudFormation rejects it); one the template adds takes
# its default, and is reported. Exits 2, naming it, when a new parameter has no default: a roll
# cannot choose its value, so that stack needs a full deploy.
changeset_parameters() {
  python3 - "$@" <<'PY'
import json, sys

summary_file, stack_file, image_key, image_value, out_file = sys.argv[1:]
with open(summary_file) as f:
    template_params = json.load(f).get("Parameters") or []
with open(stack_file) as f:
    stack = json.load(f)
stack = (stack.get("Stacks") or [stack])[0]
existing = {p["ParameterKey"] for p in stack.get("Parameters") or []}

params, missing = [], []
for p in template_params:
    key = p["ParameterKey"]
    if key == image_key:
        params.append({"ParameterKey": key, "ParameterValue": image_value})
    elif key in existing:
        params.append({"ParameterKey": key, "UsePreviousValue": True})
    elif "DefaultValue" in p:
        print(f"new parameter {key} takes its default '{p['DefaultValue']}'", file=sys.stderr)
    else:
        missing.append(key)
for key in sorted(existing - {p["ParameterKey"] for p in template_params}):
    print(f"parameter {key} is no longer in the template and is dropped", file=sys.stderr)
if image_key not in {p["ParameterKey"] for p in template_params}:
    print(f"the template has no {image_key} parameter", file=sys.stderr)
    sys.exit(2)
if missing:
    print(f"the template adds parameter(s) with no default: {', '.join(missing)}; "
          "a roll cannot choose their values, so deploy this stack in full", file=sys.stderr)
    sys.exit(2)
with open(out_file, "w") as f:
    json.dump(params, f, indent=1)
PY
}

# changeset_preview_image_ref <ecr url> <repo> <tag> <region>: the image reference a dry run puts in
# its change set, without pushing anything: the digest the tag already has in ECR, or else the tag
# itself (the image is not pushed yet, so the change set shows an image change either way).
changeset_preview_image_ref() {
  local digest
  digest=$(aws ecr describe-images --repository-name "$2" --image-ids imageTag="$3" --region "$4" --query "imageDetails[0].imageDigest" --output text 2>/dev/null || true)
  if [[ "$digest" == sha256:* ]]; then
    printf '%s/%s@%s\n' "$1" "$2" "$digest"
  else
    printf '%s/%s:%s\n' "$1" "$2" "$3"
  fi
}

_cs_delete() { # <stack> <change set name> <region>
  aws cloudformation delete-change-set --stack-name "$1" --change-set-name "$2" --region "$3" >/dev/null 2>&1 || true
}

# changeset_apply <stack> <region> <template file> <image key> <image value> <yes 0|1> <dry run 0|1>
# Creates, shows and (unless a dry run, or declined) executes the change set, waiting for the stack
# update. Sets CS_RESULT:
#   empty          nothing to change; the change set was deleted
#   dry-run        shown and deleted
#   executed       the stack update completed
#   declined       review needed and refused (or no terminal to ask on); the change set was deleted
#   failed         the change set could not be created; it was deleted
#   update-failed  the stack update failed (the caller shows the container logs)
# Returns 0 for empty, dry-run and executed, 1 otherwise.
changeset_apply() {
  local stack=$1 region=$2 template=$3 image_key=$4 image_value=$5 yes=$6 dry_run=$7
  local tmp name verdict answer line rc=0
  CS_RESULT=failed
  tmp=$(mktemp -d "${TMPDIR:-/tmp}/changeset.XXXXXX")
  name="roll-$(date +%Y%m%d%H%M%S)"

  if ! aws cloudformation get-template-summary --template-body "file://$template" --region "$region" --output json >"$tmp/summary.json" \
      || ! aws cloudformation describe-stacks --stack-name "$stack" --region "$region" --output json >"$tmp/stack.json"; then
    echo "$CS_PREFIX ✗ could not read the template $template or the stack $stack" >&2
    rm -rf "$tmp"; return 1
  fi
  changeset_parameters "$tmp/summary.json" "$tmp/stack.json" "$image_key" "$image_value" "$tmp/params.json" 2>"$tmp/params.log" || rc=$?
  while IFS= read -r line; do echo "$CS_PREFIX   $line"; done <"$tmp/params.log"
  if [[ $rc -ne 0 ]]; then
    echo "$CS_PREFIX ✗ could not build the parameters for stack '$stack' from $template" >&2
    rm -rf "$tmp"; return 1
  fi

  echo "$CS_PREFIX Creating change set '$name' on stack '$stack' from $template..."
  if ! aws cloudformation create-change-set --stack-name "$stack" --change-set-name "$name" --change-set-type UPDATE --template-body "file://$template" --parameters "file://$tmp/params.json" --capabilities CAPABILITY_IAM --region "$region" >/dev/null; then
    echo "$CS_PREFIX ✗ could not create a change set on stack '$stack'" >&2
    rm -rf "$tmp"; return 1
  fi
  # The waiter fails on a FAILED set, which includes an empty one; the description says which.
  aws cloudformation wait change-set-create-complete --stack-name "$stack" --change-set-name "$name" --region "$region" 2>/dev/null || true
  if ! aws cloudformation describe-change-set --stack-name "$stack" --change-set-name "$name" --region "$region" --output json >"$tmp/changeset.json"; then
    echo "$CS_PREFIX ✗ could not read change set '$name' on stack '$stack'" >&2
    _cs_delete "$stack" "$name" "$region"
    rm -rf "$tmp"; return 1
  fi
  changeset_classify "$tmp/changeset.json" >"$tmp/verdict"
  verdict=$(head -n 1 "$tmp/verdict")

  case "$verdict" in
    empty)
      echo "$CS_PREFIX Stack '$stack' already matches the template and image: nothing to change."
      _cs_delete "$stack" "$name" "$region"; CS_RESULT=empty; rm -rf "$tmp"; return 0 ;;
    failed)
      echo "$CS_PREFIX ✗ change set on stack '$stack' failed: $(sed -n 2p "$tmp/verdict")" >&2
      _cs_delete "$stack" "$name" "$region"; rm -rf "$tmp"; return 1 ;;
    safe|review) ;;
    *)
      # never execute a change set that could not be classified
      echo "$CS_PREFIX ✗ could not classify change set '$name' on stack '$stack'; not executing it" >&2
      _cs_delete "$stack" "$name" "$region"; rm -rf "$tmp"; return 1 ;;
  esac

  echo "$CS_PREFIX Change set for stack '$stack':"
  changeset_print "$tmp/changeset.json"
  if [[ "$verdict" == review ]]; then
    echo "$CS_PREFIX These changes are outside the expected task definition and service update:"
    tail -n +2 "$tmp/verdict" | sed 's/^/      - /'
  fi

  if [[ $dry_run -eq 1 ]]; then
    if [[ "$verdict" == safe ]]; then
      echo "$CS_PREFIX Dry run: a roll would execute this without asking. Deleting the change set."
    else
      echo "$CS_PREFIX Dry run: a roll would stop and ask before executing this (or run it with --yes). Deleting the change set."
    fi
    _cs_delete "$stack" "$name" "$region"; CS_RESULT=dry-run; rm -rf "$tmp"; return 0
  fi

  if [[ "$verdict" == review && $yes -ne 1 ]]; then
    if [[ ! -t 0 ]]; then
      echo "$CS_PREFIX ✗ stack '$stack' needs review and there is no terminal to ask on; review the changes above and rerun with --yes" >&2
      _cs_delete "$stack" "$name" "$region"; CS_RESULT=declined; rm -rf "$tmp"; return 1
    fi
    read -r -p "$CS_PREFIX Execute this change set on '$stack'? [y/N] " answer || answer=""
    if [[ "$answer" != y && "$answer" != Y ]]; then
      echo "$CS_PREFIX ✗ not executed; stack '$stack' is unchanged" >&2
      _cs_delete "$stack" "$name" "$region"; CS_RESULT=declined; rm -rf "$tmp"; return 1
    fi
  fi

  echo "$CS_PREFIX Executing change set '$name' on stack '$stack'..."
  if ! aws cloudformation execute-change-set --stack-name "$stack" --change-set-name "$name" --region "$region" >/dev/null \
      || ! aws cloudformation wait stack-update-complete --stack-name "$stack" --region "$region"; then
    echo "$CS_PREFIX ✗ the update of stack '$stack' failed" >&2
    CS_RESULT=update-failed; rm -rf "$tmp"; return 1
  fi
  CS_RESULT=executed; rm -rf "$tmp"; return 0
}
