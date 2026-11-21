#!/usr/bin/env bash
# scripts/mailer.sh
#
# Purpose: send each email that `bingle_cli logclient --maildir` writes into a Maildir, through
# Amazon Simple Email Service (SES), then delete it (epic #63, issue #67). See SUPPORT_EMAIL.md.
#
# Usage:
#   scripts/mailer.sh [--once] <to_email> <maildir>
#
# Every MAILER_INTERVAL seconds it sends each file in <maildir>/new/, oldest first, adding a
# `To: <to_email>` header. A file is deleted only once SES accepts it. A send that fails for a reason
# that may clear (network, credentials, throttling, an unverified address) leaves the file in new/ to
# retry on the next pass. A message SES rejects for its content is moved to <maildir>/failed/ so it
# does not block the queue. SIGINT/SIGTERM exits after the current send.
#
# Options:
#   --once                 Make one pass over new/ and exit (exit 1 if any file is left in new/).
#
# Environment:
#   MAILER_SEND            smtp (default): curl to the SES SMTP endpoint; ses-api: `aws sesv2`
#                          with ambient AWS credentials (the task role in production)
#   MAILER_INTERVAL        seconds between passes (default 10)
#   SES_REGION             SES region, e.g. ca-central-1 (required)
#   SES_RELAY_FROM         envelope sender; must equal `logclient --from` and be a verified SES
#                          identity (required)
#   SES_SMTP_USER          SES SMTP user name (smtp only)
#   SES_SMTP_PASS          SES SMTP password (smtp only)

set -euo pipefail

usage() {
  echo "Usage: $0 [--once] <to_email> <maildir>" >&2
  echo "  Environment: MAILER_SEND=smtp|ses-api, MAILER_INTERVAL, SES_REGION, SES_RELAY_FROM, and SES_SMTP_USER/SES_SMTP_PASS for smtp." >&2
}

log() {
  printf '%s mailer: %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >&2
}

die() {
  log "error: $*"
  exit 2
}

# An address safe to put in a header and in JSON unescaped: local@domain, no spaces, quotes,
# backslashes or angle brackets.
valid_address() {
  [[ "$1" =~ ^[^[:space:]@\"\\\<\>]+@[^[:space:]@\"\\\<\>]+$ ]]
}

ONCE=0
POSITIONAL=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --once) ONCE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    -*) usage; die "unknown option $1" ;;
    *) POSITIONAL+=("$1"); shift ;;
  esac
done
if [[ ${#POSITIONAL[@]} -ne 2 ]]; then
  usage
  exit 2
fi
TO_EMAIL="${POSITIONAL[0]}"
MAILDIR="${POSITIONAL[1]}"

MAILER_SEND="${MAILER_SEND:-smtp}"
MAILER_INTERVAL="${MAILER_INTERVAL:-10}"

valid_address "$TO_EMAIL" || die "<to_email> must be an address like support@example.com, got '$TO_EMAIL'"
[[ -n "${SES_REGION:-}" ]] || die "SES_REGION is not set"
[[ "$SES_REGION" =~ ^[a-z0-9-]+$ ]] || die "SES_REGION looks wrong: '$SES_REGION'"
[[ -n "${SES_RELAY_FROM:-}" ]] || die "SES_RELAY_FROM is not set"
valid_address "$SES_RELAY_FROM" || die "SES_RELAY_FROM must be an address, got '$SES_RELAY_FROM'"
[[ "$MAILER_INTERVAL" =~ ^[1-9][0-9]*$ ]] || die "MAILER_INTERVAL must be a whole number of seconds, got '$MAILER_INTERVAL'"
case "$MAILER_SEND" in
  smtp)
    [[ -n "${SES_SMTP_USER:-}" ]] || die "SES_SMTP_USER is not set (needed for MAILER_SEND=smtp)"
    [[ -n "${SES_SMTP_PASS:-}" ]] || die "SES_SMTP_PASS is not set (needed for MAILER_SEND=smtp)"
    command -v curl >/dev/null || die "curl is not installed"
    ;;
  ses-api)
    command -v aws >/dev/null || die "the aws command is not installed (needed for MAILER_SEND=ses-api)"
    ;;
  *) die "MAILER_SEND must be smtp or ses-api, got '$MAILER_SEND'" ;;
esac

mkdir -p "$MAILDIR/new" "$MAILDIR/failed" || die "cannot use Maildir $MAILDIR"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/mailer.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

STOP=0
trap 'STOP=1' INT TERM

# Outcome of a send attempt, set by send_file: sent, retry or reject.
OUTCOME=""

# Classify a failed send from the server's last reply. A rejection of the message itself is
# permanent; anything else (no reply, a temporary 4xx, a login or address problem) may clear, so it
# is retried. An unverified-address rejection is a setup problem, not the message's fault.
classify_failure() {
  local content_stage="$1" reply="$2"
  if [[ "$reply" == *"not verified"* ]]; then
    OUTCOME=retry
  elif [[ "$content_stage" == 1 && "$reply" =~ ^5[0-9][0-9] ]]; then
    OUTCOME=reject
  else
    OUTCOME=retry
  fi
}

send_smtp() {
  local staged="$1" trace reply content_stage=0
  # --verbose is only parsed for the server's reply codes. It includes the login, so it is never
  # printed.
  if trace=$(curl --silent --show-error --verbose --ssl-reqd \
      "smtps://email-smtp.${SES_REGION}.amazonaws.com:465" \
      --mail-from "$SES_RELAY_FROM" --mail-rcpt "$TO_EMAIL" \
      --upload-file "$staged" --user "$SES_SMTP_USER:$SES_SMTP_PASS" 2>&1 >/dev/null); then
    OUTCOME=sent
    return
  fi
  # `< 354` is the server inviting the message content: a later 5xx is about the message itself.
  grep -q '^< 354' <<<"$trace" && content_stage=1
  reply=$(grep -E '^< [0-9]{3}' <<<"$trace" | tail -n 1 | cut -c3- || true)
  [[ -n "$reply" ]] || reply=$(grep -E '^curl: ' <<<"$trace" | tail -n 1 || true)
  LAST_REPLY="${reply:-no reply}"
  classify_failure "$content_stage" "$LAST_REPLY"
}

send_ses_api() {
  local staged="$1" input="$WORK/input.json" output
  # Blobs in --cli-input-json are base64 (AWS CLI v2); --cli-binary-format makes v1 fail loudly.
  printf '{"FromEmailAddress":"%s","Destination":{"ToAddresses":["%s"]},"Content":{"Raw":{"Data":"%s"}}}' \
    "$SES_RELAY_FROM" "$TO_EMAIL" "$(base64 <"$staged" | tr -d '\n')" >"$input"
  if output=$(aws sesv2 send-email --region "$SES_REGION" --cli-binary-format base64 \
      --cli-input-json "file://$input" 2>&1 >/dev/null); then
    OUTCOME=sent
    return
  fi
  LAST_REPLY=$(tail -n 1 <<<"$output")
  if [[ "$output" == *"not verified"* ]]; then
    OUTCOME=retry
  elif [[ "$output" == *MessageRejected* ]]; then
    OUTCOME=reject
  else
    OUTCOME=retry
  fi
}

# Send one Maildir file with a To: header added, and act on the outcome.
send_file() {
  local file="$1" name staged="$WORK/staged.eml"
  name=$(basename "$file")
  printf 'To: %s\r\n' "$TO_EMAIL" >"$staged"
  cat "$file" >>"$staged"
  LAST_REPLY=""
  case "$MAILER_SEND" in
    smtp) send_smtp "$staged" ;;
    ses-api) send_ses_api "$staged" ;;
  esac
  case "$OUTCOME" in
    sent)
      rm -f "$file"
      log "sent $name to $TO_EMAIL"
      ;;
    reject)
      mv "$file" "$MAILDIR/failed/$name"
      log "error: SES rejected $name ($LAST_REPLY); moved to failed/"
      ;;
    *)
      log "warning: could not send $name ($LAST_REPLY); will retry"
      ;;
  esac
}

# One pass over new/, oldest first. Stops early on a signal, leaving the rest for next time.
pass() {
  local name
  while IFS= read -r name; do
    [[ $STOP -eq 1 ]] && return
    [[ -z "$name" || "$name" == .* ]] && continue
    [[ -f "$MAILDIR/new/$name" ]] || continue
    send_file "$MAILDIR/new/$name"
  done < <(ls -1tr "$MAILDIR/new")
}

log "sending $MAILDIR/new/ to $TO_EMAIL via $MAILER_SEND every ${MAILER_INTERVAL}s"
while true; do
  pass
  if [[ $ONCE -eq 1 ]]; then
    if [[ -n "$(ls -A "$MAILDIR/new")" ]]; then
      exit 1
    fi
    exit 0
  fi
  [[ $STOP -eq 1 ]] && break
  # Sleep in the background and wait, so a signal ends the wait at once.
  sleep "$MAILER_INTERVAL" &
  wait $! || true
  kill $! 2>/dev/null || true
  [[ $STOP -eq 1 ]] && break
done
log "stopped"
