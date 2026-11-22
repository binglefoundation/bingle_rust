# Support email: forwarding Bingle messages by email

Bingle messages sent to a support handle can be forwarded to an ordinary mailbox, so the operator
sees them without running a Bingle client (epic #63). Two programs share a **Maildir**, a folder of
one-file-per-message email:

1. `bingle_cli logclient` runs as the support account and writes each message it receives into the
   Maildir as an email.
2. `scripts/mailer.sh` sends each email in the Maildir through Amazon Simple Email Service (SES),
   and deletes it once SES accepts it.

Email is one-way: replying to a forwarded email does not reach the sender. Reply with
`bingle_cli chat` or the app.

## Running the pipeline

Start both on the same Maildir, in two terminals or as two services:

```bash
bingle_cli logclient --state_file support_state.json --maildir support_maildir --from relay@example.com --spam-limit 20 --node-file nodely_deployed_mainnet_node.json --stun-servers-file stunservers.txt
```

```bash
SES_REGION=ca-central-1 SES_RELAY_FROM=relay@example.com SES_SMTP_USER=<user> SES_SMTP_PASS=<password> scripts/mailer.sh helpdesk@example.com support_maildir
```

- On its first run `logclient` needs `--passphrase` (and `--handle` to register a new account); see
  the [developer guide](DEVELOPER.md#receiving-messages-for-a-mailer-logclient). Afterwards the
  state file is enough.
- `--from` (for `logclient`) and `SES_RELAY_FROM` (for the mailer) must be the same verified SES
  address.
- Either side can be stopped and restarted on its own: messages wait in the Maildir.

### The mailer

```bash
scripts/mailer.sh [--once] <to_email> <maildir>
```

Every `MAILER_INTERVAL` seconds it sends each file in `<maildir>/new/`, oldest first, adding a
`To: <to_email>` header (`logclient` leaves the recipient to the mailer). `--once` makes one pass
and exits, with status 1 if any file is left in `new/`. SIGINT or SIGTERM stops it after the current
send.

| Variable | Meaning |
| --- | --- |
| `MAILER_SEND` | `smtp` (default): send with `curl` to the SES Simple Mail Transfer Protocol (SMTP) endpoint. `ses-api`: send with `aws sesv2 send-email` using the ambient AWS credentials, such as an ECS task role. |
| `MAILER_INTERVAL` | Seconds between passes; default 10. |
| `SES_REGION` | The SES region, for example `ca-central-1`. Required. |
| `SES_RELAY_FROM` | The envelope sender: the verified SES address `logclient --from` writes. Required. |
| `SES_SMTP_USER`, `SES_SMTP_PASS` | SES SMTP credentials. Required for `smtp`; not used by `ses-api`. |

The mailer never logs the SMTP password.

## What happens when a send fails

| Outcome | Examples | What the mailer does |
| --- | --- | --- |
| Accepted | | Deletes the file. |
| May clear later | No network, SES throttling, a temporary `4xx` reply, wrong SMTP credentials, an address not verified in SES | Logs a warning and leaves the file in `new/`; it is retried on every pass until it goes. |
| Rejected for its content | An SMTP `5xx` reply to the message itself, or an SES API `MessageRejected` error | Logs an error and moves the file to `<maildir>/failed/`, so later messages are not held up. |

A login problem or an unverified address would reject every message, so those are retried rather
than failed: fix the setup and the queue drains by itself. To retry a failed message after fixing
the cause, move it back:

```bash
mv support_maildir/failed/* support_maildir/new/
```

If `logclient` cannot write a message into the Maildir (a full disk, say), it logs an error and the
message stays in its state file's history, from where it can be read with `bingle_cli chat`.

**Spam limit.** `logclient --spam-limit <n>` forwards at most `n` messages a UTC day from each
sender; later ones that day stay in the state file's history only, and the first is logged. The
counts survive a restart. See the
[developer guide](DEVELOPER.md#receiving-messages-for-a-mailer-logclient) for details. The limit is
per Bingle account, so one person with many registered accounts is not covered; registration cost
limits that.

## Setting up SES

Do this once per AWS account and region, in the AWS console under **Amazon SES** or with the AWS
command-line interface.

1. **Verify the sender address** (`SES_RELAY_FROM`). Either verify the single address, which sends a
   confirmation link to it, or verify the whole domain with DomainKeys Identified Mail (DKIM)
   records:

   ```bash
   aws sesv2 create-email-identity --email-identity relay@example.com --region ca-central-1
   ```

2. **Leave the sandbox, or verify the recipient too.** A new SES account is in the *sandbox*: it may
   send only to verified addresses, at most 200 a day. That is enough when the recipient is the
   operator's own mailbox: verify it with the same command. Otherwise request production access in
   the console (**Account dashboard → Request production access**).
3. **Credentials**, depending on `MAILER_SEND`:
   - `smtp`: create SMTP credentials (**SMTP settings → Create SMTP credentials**). This makes an
     Identity and Access Management (IAM) user allowed to send; keep the user name and password for
     `SES_SMTP_USER` and `SES_SMTP_PASS`. They are not the IAM user's access keys.
   - `ses-api`: give the role the mailer runs under permission for `ses:SendRawEmail` on the
     verified identity. No password is needed. The production deployment (#473) uses this.

## Deploying on AWS

In production the pipeline runs as its own Amazon Elastic Container Service (ECS) service on
Fargate, in an existing relay stack's cluster and network (issue #473). `aws/deploy_gateway.sh`
builds the `gateway` image and deploys `aws/gateway_stack.yaml`, which adds:

- **one task, two containers** from the same image: `logclient` and the mailer (with
  `MAILER_SEND=ses-api`). The service never runs two tasks at once, since two `logclient`s as one
  account would split its messages between them;
- **an encrypted Elastic File System (EFS) volume** at `/data` holding `state.json` and `maildir/`,
  so the account, its history, the spam counts and any unsent email survive redeploys and task
  replacement. Only the task's role can mount it, over TLS;
- **a task role** that can send through SES only as `--from`. There are no SMTP credentials;
- **a log group**, `/bingle/gateway/<stack>`, kept for 30 days.

It costs about $12 a month, almost all of it the always-on Fargate task and its public IPv4
address. The relay stack is not changed, so relay redeploys don't restart the gateway.

### Before the first deploy

1. **SES**: verify the `--from` identity in the relay's region, as in [Setting up SES](#setting-up-ses).
   While the account is in the sandbox, the destination must be verified too; an address in a
   verified domain counts.
2. **Stop reading the account elsewhere.** Once the gateway runs, it takes every message sent to
   the account. A `bingle_cli chat` or app session signed in as the same account at the same time
   would take some messages that are then never emailed. Signing in briefly to reply is fine.
3. **Put the account's recovery phrase in Secrets Manager**, for the first start only. This reads
   it without echoing it or keeping it in shell history, and prints the secret's Amazon Resource
   Name (ARN):

   ```bash
   printf 'Recovery phrase: ' && read -rs P && echo && aws secretsmanager create-secret --name /bingle/gateway/first-start-phrase --secret-string "$P" --region <region> --query ARN --output text; unset P
   ```

### First deploy

```bash
aws/deploy_gateway.sh --relay-stack <relay stack> --to <inbox> --from <verified address> --handle <account handle> --passphrase-secret <secret ARN>
```

The stack is named `bingle-gateway-<relay stack>` unless `--stack-name` says otherwise.
`--spam-limit` defaults to 20, and the Algorand node file to the relay stack's. Then:

1. Follow the logs until `logclient: receiving as '<handle>'`:

   ```bash
   aws logs tail /bingle/gateway/bingle-gateway-<relay stack> --follow --region <region>
   ```

2. Send the account a message from the app, and check it arrives at the inbox.
3. The account is now in the state file on EFS. Deploy again without the phrase, so the task no
   longer reads it, then delete the secret:

   ```bash
   aws/deploy_gateway.sh --relay-stack <relay stack> --to <inbox> --from <verified address>
   ```

   ```bash
   aws secretsmanager delete-secret --secret-id <secret ARN> --force-delete-without-recovery --region <region>
   ```

The state file on EFS holds the account's key, as a `bingle_cli` state file does. Anyone who can
mount the volume or change the task can act as the account.

### Redeploying

After a code change, update only the image; every other setting is kept:

```bash
aws/deploy_gateway.sh --relay-stack <relay stack> --redeploy-only
```

To change `--to`, `--from` or `--spam-limit`, run a full deploy (without the phrase). The old task
stops first, and `logclient` saves its state file on the way out, so nothing is lost or sent twice.
While SES rejects sends (or is unreachable), messages wait in `/data/maildir/new/` and go out once
it accepts them again.

### Removing it

`aws cloudformation delete-stack --stack-name bingle-gateway-<relay stack> --region <region>`
deletes the service and roles but **keeps the EFS volume and the log group**, since the volume
holds the account and any unsent email. A new stack makes a new, empty volume and needs the phrase
again. Delete the old volume by hand once it is no longer needed.

## Testing against SES

To check the SES setup and the mailer without a Bingle client, put a hand-made email in a test
Maildir and make one pass. Use verified addresses for both ends while in the sandbox.

```bash
mkdir -p tmp/test_maildir/new
```

```bash
printf 'From: "mailer test (Bingle)" <relay@example.com>\r\nSubject: Bingle mailer test\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHello from scripts/mailer.sh\r\n' > tmp/test_maildir/new/test.1
```

```bash
SES_REGION=ca-central-1 SES_RELAY_FROM=relay@example.com SES_SMTP_USER=<user> SES_SMTP_PASS=<password> scripts/mailer.sh --once helpdesk@example.com tmp/test_maildir
```

Expect `sent test.1 to helpdesk@example.com` on stderr, exit status 0, an empty
`tmp/test_maildir/new/`, and the email in the recipient's inbox. For `ses-api`, set
`MAILER_SEND=ses-api` with AWS credentials in place of the SMTP variables.
