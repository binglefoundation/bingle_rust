# AWS account setup — production relays

Runbook for creating and bootstrapping the AWS account that hosts Bingle's
**production** relays and related infrastructure.

- **Root login:** `bingle.foundation@gmail.com`
- **Account model:** standalone account (not part of an AWS Organization — can be
  invited into one later)
- **Relay launch type:** Fargate (the `--express` path of `aws/deploy_relay.sh`)

Keep production isolated from the existing dev account (the deploy scripts tag dev
spend with `bingle_dev`). This account is dedicated to production.

---

## 0. Prerequisites

- A company payment card for the account.
- Control of the `bingle.foundation@gmail.com` mailbox (it becomes the root
  identity and the password-reset path).
- A password manager the team controls, for the root credentials.

---

## 1. Create the account

1. Go to <https://aws.amazon.com> → **Create an AWS Account**.
2. Root email: **`bingle.foundation@gmail.com`**; account name e.g.
   `bingle-foundation-prod`.
3. Set a long, unique root password and store it in the shared password manager.
4. Provide billing details, complete phone/identity verification, and select the
   **Basic** support plan (upgrade to Developer/Business later if you want faster
   incident support on production).
5. **Secure the mailbox:** enable 2FA on `bingle.foundation@gmail.com` and restrict
   who can read it — anyone with inbox access can reset the AWS root user.

---

## 2. Lock down the root user (do this immediately)

- Sign in as root → **enable MFA** (hardware key or authenticator; ideally register
  two devices).
- **Do not create root access keys.** Delete any that exist.
- In **Account settings**, enable *IAM user/role access to Billing*.
- After this, **stop using root** except for the few tasks that require it (closing
  the account, changing the support plan, some billing actions).

---

## 3. Human admin access (don't use root day-to-day)

Pick one:

- **Preferred — IAM Identity Center (SSO):** enable it, create a permission set
  (`AdministratorAccess` to start), and assign team members. Gives per-person logins
  with MFA and short-lived CLI credentials via `aws sso login`.
- **Lighter — IAM admin user:** create a single IAM user with `AdministratorAccess`
  and MFA.

---

## 4. Billing guardrails (before spinning anything up)

- **AWS Budgets:** a monthly cost budget with alerts at 50 / 80 / 100 % to a team
  address.
- **Cost Explorer:** enable it, then activate the cost-allocation tag key the relay
  stacks use. `aws/deploy_relay.sh` tags resources via `--cost-tag`; for production
  use `bingle_prod` and activate that key under **Billing → Cost allocation tags** so
  spend is trackable.
- Optionally add a **cost-anomaly / zero-spend** alert to catch surprises.

---

## 5. Region and account baseline

- Choose one **home region** near the relay user base (relays are latency-sensitive
  P2P). Standardize on it — `aws/deploy_relay.sh` reads the region from
  `aws configure` unless `--region` is passed.
- Baseline hygiene:
  - Enable **CloudTrail** (account trail).
  - Turn on **default EBS encryption**.
  - The Fargate deploy provisions its own VPC/subnet, so no networking pre-work is
    required (see [DEPLOY_RELAY.md](./DEPLOY_RELAY.md)).

---

## 6. Deploy identity for the relay scripts

`aws/deploy_relay.sh --express` needs an identity (assumed role via Identity Center,
or a scoped CI/deploy IAM user) allowed to let CloudFormation build the stack and to
push the image to ECR. Required service actions:

| Service | Why |
| --- | --- |
| **ECR** (`GetAuthorizationToken`, `CreateRepository`, `DescribeRepositories`, push actions) | Script auto-creates the `bingle-relay` repo and pushes the image |
| **CloudFormation** (create/update/describe/delete stacks + events) | Provisions the whole relay stack |
| **EC2** (VPC, subnet, internet gateway, route table, security group) | The `--express` stack builds its own network |
| **ECS** (cluster, task definition, service) | Runs the relay on Fargate |
| **IAM** (`CreateRole`, `PutRolePolicy`, `AttachRolePolicy`, `DeleteRole`, **`PassRole`**) | The stack creates the task + execution roles; ECS assumes them |
| **Secrets Manager** + **SSM Parameter** | The stack stores the relay passphrase/config |
| **CloudWatch Logs** (`Create/Delete/DescribeLogGroups`) | Script deletes a stale log group; the stack recreates it |
| **STS** (`GetCallerIdentity`) | Script resolves the account ID for the ECR URL |

Because the stack creates **IAM roles**, the deploy identity is inherently
privileged. Pragmatic for a small prod account: `PowerUserAccess` **plus** a narrow
policy granting only the `iam:*Role*` / `iam:PassRole` actions the stack needs —
rather than full `AdministratorAccess`.

Configure a named CLI profile locally:

```bash
aws configure --profile bingle-prod        # region + credentials
export AWS_PROFILE=bingle-prod
aws sts get-caller-identity                 # confirm it is the new prod account
```

---

## 7. Hand off to relay deployment

Account is ready. Production relay deployment — including selecting the mainnet node
config on the command line — is documented in
[DEPLOY_RELAY.md](./DEPLOY_RELAY.md).
