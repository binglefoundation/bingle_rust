#!/usr/bin/env bash
# Tests for aws/lib/changeset.sh (issue #528): which change sets a roll executes unattended, and the
# parameters it sends. Offline: they read describe-change-set, get-template-summary and
# describe-stacks JSON from scripts/tests/fixtures/changeset, and call no AWS.
set -uo pipefail

TESTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$TESTS_DIR/../.." && pwd)"
FIXTURES="$TESTS_DIR/fixtures/changeset"
# shellcheck source=aws/lib/changeset.sh
source "$REPO_ROOT/aws/lib/changeset.sh"

passes=0 fails=0
check() { # <name> <expected> <actual>
  if [[ "$2" == "$3" ]]; then
    passes=$((passes + 1)); echo "  ok   $1"
  else
    fails=$((fails + 1)); echo "  FAIL $1: expected '$2', got '$3'"
  fi
}
verdict() { changeset_classify "$FIXTURES/$1" | head -n 1; }
reasons() { changeset_classify "$FIXTURES/$1" | tail -n +2; }

# classification
check "image only: task definition replaced, service updated" safe "$(verdict image_only.json)"
check "IAM role change needs review" review "$(verdict iam_change.json)"
check "IAM change names the role" "Modify ECSTaskExecutionRole (AWS::IAM::Role, Replacement: False)" "$(reasons iam_change.json)"
check "security group change needs review" review "$(verdict security_group_change.json)"
check "service networking change needs review" review "$(verdict service_network_change.json)"
check "task role change needs review" review "$(verdict task_role_change.json)"
check "service replacement needs review" review "$(verdict service_replacement.json)"
check "added resource needs review" review "$(verdict added_resource.json)"
check "dashboard that only reads the updated service's name is safe" safe "$(verdict dashboard_follows_service.json)"
check "dashboard changed directly needs review" review "$(verdict dashboard_direct_change.json)"
check "dashboard direct change is named" "Modify RelayDashboard (AWS::CloudWatch::Dashboard, Replacement: False)" "$(reasons dashboard_direct_change.json)"
check "dashboard that follows the service and also changes directly needs review" review "$(verdict dashboard_follows_and_direct.json)"
check "dashboard that follows an unchanged resource needs review" review "$(verdict dashboard_follows_other_resource.json)"
check "dashboard that follows the service but may be replaced needs review" review "$(verdict dashboard_follows_service_replaced.json)"
check "empty (FAILED, didn't contain changes) is nothing to do" empty "$(verdict empty.json)"
check "empty (complete, no changes) is nothing to do" empty "$(verdict empty_complete.json)"
check "other failure is a failure" failed "$(verdict failed.json)"
check "failure gives CloudFormation's reason" "Requires capabilities : [CAPABILITY_NAMED_IAM]" "$(reasons failed.json)"

# the printed summary names each changed resource
check "print lists the task definition" 1 "$(changeset_print "$FIXTURES/image_only.json" | grep -c 'RelayTaskDefinition')"
check "print of an empty set" "    (no resource changes)" "$(changeset_print "$FIXTURES/empty.json")"

# parameters
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
changeset_parameters "$FIXTURES/template_summary.json" "$FIXTURES/stack.json" ImageUri "111111111111.dkr.ecr.us-east-1.amazonaws.com/bingle-relay@sha256:bbbb" "$tmp/params.json" 2>"$tmp/notes"
check "parameters build" 0 "$?"
check "parameters: new image, previous values, new default left out, dropped one left out" \
  'ImageUri=111111111111.dkr.ecr.us-east-1.amazonaws.com/bingle-relay@sha256:bbbb Handle=previous Passphrase=previous NatMode=previous' \
  "$(python3 -c 'import json,sys; print(" ".join(p["ParameterKey"] + "=" + ("previous" if p.get("UsePreviousValue") else p["ParameterValue"]) for p in json.load(open(sys.argv[1]))))' "$tmp/params.json")"
check "parameters: new default reported" 1 "$(grep -c "new parameter NewSetting takes its default '1'" "$tmp/notes")"
check "parameters: dropped parameter reported" 1 "$(grep -c "parameter OldSetting is no longer in the template" "$tmp/notes")"
changeset_parameters "$FIXTURES/template_summary_new_required.json" "$FIXTURES/stack.json" ImageUri "x" "$tmp/params2.json" 2>"$tmp/notes2"
check "new parameter with no default refuses" 2 "$?"
check "refusal names it" 1 "$(grep -c "NewRequired" "$tmp/notes2")"
check "refusal writes no parameters" no "$([[ -e "$tmp/params2.json" ]] && echo yes || echo no)"

# changeset_apply's flow, with aws stubbed by a function: it logs each call and serves the fixture
# named by STUB_CHANGESET as the change set. Nothing reaches AWS.
STUB_LOG="$tmp/aws.log"
aws() {
  echo "$1 $2" >>"$STUB_LOG"
  case "$1 $2" in
    "cloudformation get-template-summary") cat "$FIXTURES/template_summary.json" ;;
    "cloudformation describe-stacks") cat "$FIXTURES/stack.json" ;;
    "cloudformation describe-change-set") cat "$FIXTURES/$STUB_CHANGESET" ;;
    "cloudformation wait") [[ "$3" != change-set-create-complete ]] || [[ "$STUB_CHANGESET" != empty.json ]] ;;
  esac
}
apply() { # <fixture> <yes> <dry run>: prints CS_RESULT, the exit code and the calls that change state
  STUB_CHANGESET=$1
  : >"$STUB_LOG"
  changeset_apply example-relay-stack us-east-1 "$REPO_ROOT/aws/relay_express.yaml" ImageUri "example-image" "$2" "$3" </dev/null >/dev/null 2>&1
  local rc=$?
  echo "$CS_RESULT $rc $(grep -E 'create-change-set|execute-change-set|delete-change-set' "$STUB_LOG" | sed 's/^cloudformation //' | tr '\n' ' ' | sed 's/ $//')"
}
CS_PREFIX="[test]"
check "apply image only: executed without asking" "executed 0 create-change-set execute-change-set" "$(apply image_only.json 0 0)"
check "apply empty: nothing to do, set deleted" "empty 0 create-change-set delete-change-set" "$(apply empty.json 0 0)"
check "apply IAM change, no terminal: declined, set deleted" "declined 1 create-change-set delete-change-set" "$(apply iam_change.json 0 0)"
check "apply IAM change with --yes: executed" "executed 0 create-change-set execute-change-set" "$(apply iam_change.json 1 0)"
check "apply dry run: shown and deleted, not executed" "dry-run 0 create-change-set delete-change-set" "$(apply iam_change.json 1 1)"
check "apply failed set: failure, set deleted" "failed 1 create-change-set delete-change-set" "$(apply failed.json 0 0)"
check "apply unreadable description: never executed" "failed 1 create-change-set delete-change-set" "$(apply unreadable.json 1 0)"

# pre-flight (#548): CS_REPORT records the classification; CS_APPROVED lets a change set that was
# approved at the start of the roll execute without asking (here: with no terminal, which otherwise
# declines it).
CS_REPORT="$tmp/report" apply iam_change.json 0 1 >/dev/null
check "a dry run writes its classification to CS_REPORT" "$(changeset_classify "$FIXTURES/iam_change.json")" "$(cat "$tmp/report")"
changeset_classify "$FIXTURES/iam_change.json" >"$tmp/approved_iam"
changeset_classify "$FIXTURES/image_only.json" >"$tmp/approved_safe"
cat "$tmp/approved_iam" >"$tmp/approved_more"
echo "Modify ExtraThing (AWS::EC2::SecurityGroup, Replacement: False)" >>"$tmp/approved_more"
check "approved at pre-flight: executed without asking" "executed 0 create-change-set execute-change-set" "$(CS_APPROVED="$tmp/approved_iam" apply iam_change.json 0 0)"
check "fewer changes than approved: executed without asking" "executed 0 create-change-set execute-change-set" "$(CS_APPROVED="$tmp/approved_more" apply iam_change.json 0 0)"
check "a change that was not approved: asks (declined, no terminal)" "declined 1 create-change-set delete-change-set" "$(CS_APPROVED="$tmp/approved_safe" apply iam_change.json 0 0)"
check "approval file missing: asks (declined, no terminal)" "declined 1 create-change-set delete-change-set" "$(CS_APPROVED="$tmp/nonexistent" apply iam_change.json 0 0)"
STUB_CHANGESET=iam_change.json
check "a change that was not approved is named" "[test] These differ from the changes approved at the start of the roll:" \
  "$(CS_APPROVED="$tmp/approved_safe" changeset_apply example-relay-stack us-east-1 x ImageUri img 0 0 </dev/null 2>/dev/null | grep -F 'These differ')"
unset -f aws

echo "  $passes passed, $fails failed"
[[ $fails -eq 0 ]]
