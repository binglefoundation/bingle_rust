# Deploying a Bingle relay

How to build and deploy a Bingle relay server to AWS with
[`aws/deploy_relay.sh`](../../aws/deploy_relay.sh).

For first-time setup of the production AWS account, see
[AWS_SETUP.md](./AWS_SETUP.md).

---

## Overview

`aws/deploy_relay.sh`:

1. Builds the relay container image and pushes it to **ECR** (auto-creating the
   `bingle-relay` repository if needed).
2. Deploys a **CloudFormation** stack that runs the image on **ECS**.
3. Streams container logs to **CloudWatch Logs**.

Two launch types:

- **EC2** (default) — runs the task on a `t4g` ARM instance you manage
  (`aws/relay_stack.yaml`).
- **Fargate** — `--express`, serverless containers with no instance to manage
  (`aws/relay_express.yaml`). This is the **production** launch type. The express
  stack is self-contained: it provisions its own VPC, subnet, internet gateway,
  route table, security group, ECS cluster, both task roles, a Secrets Manager
  secret, an SSM parameter, the log group, and the Fargate service (ARM64,
  256 CPU / 512 MB, public IP enabled).

---

## Command-line options

| Option | Default | Notes |
| --- | --- | --- |
| `--handle <handle>` | *(required)* | Relay handle |
| `--passphrase <pass>` | *(required)* | Relay passphrase |
| `--node-file <path>` | `/app/nodely_staging_testnet_node.json` | Node config **path inside the container** (see [Selecting the node config](#selecting-the-node-config)) |
| `--stack-name <name>` | `bingle-relay` | CloudFormation stack name |
| `--instance-type <type>` | `t4g.micro` | EC2 only (ignored with `--express`) |
| `--port <port>` | `12121` | UDP port |
| `--nat-mode <mode>` | `Direct` | `Direct` \| `Full` \| `Restricted` |
| `--region <region>` | from `aws configure` | AWS region |
| `--repo-name <name>` | `bingle-relay` | ECR repository name |
| `--tag <tag>` | `.build_number` or `latest` | Image tag |
| `--cost-tag <tag>` | `bingle_dev` | Cost-allocation tag on all stack resources |
| `--redeploy-only` | off | Skip CloudFormation; just update the ECS service |
| `--express` | off | Use the Fargate stack |

---

## Selecting the node config

`--node-file` selects which Algorand node configuration the relay uses. The value
flows through the deploy end to end:

```
--node-file <path>  →  CloudFormation "NodeFile" parameter  →  container NODE_FILE env var
```

The value is a **path inside the container image**, not a path on your machine.
That means a node config is only selectable if it has been **baked into the image**
with a `Dockerfile` `COPY`. Today the image ships one:

```dockerfile
COPY nodely_staging_testnet_node.json /app/nodely_staging_testnet_node.json
```

So the default `--node-file /app/nodely_staging_testnet_node.json` targets
**testnet**. To target another network, add its config to the image (below) and
pass the matching in-container path.

---

## Production (mainnet)

Production relays must run against **mainnet**, using `nodely_deployed_mainnet_node.json`.

### One-time changes to make the mainnet config selectable

1. **Add the mainnet node file to the repository** —
   `nodely_deployed_mainnet_node.json`, of the same shape as the staging file but
   pointing at the nodely **mainnet** endpoints and the **deployed mainnet**
   `app_id` / `asset_id`:

   ```json
   {
     "network": "mainnet",
     "client_api_url": "https://mainnet-api.4160.nodely.dev",
     "client_api_port": 443,
     "indexer_api_url": "https://mainnet-idx.4160.nodely.dev",
     "indexer_api_port": 443,
     "token": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
     "token_key": null,
     "app_id": <MAINNET_APP_ID>,
     "asset_id": <MAINNET_ASSET_ID>
   }
   ```

2. **Bake it into the image** — add to the `Dockerfile` alongside the staging copy:

   ```dockerfile
   COPY nodely_deployed_mainnet_node.json /app/nodely_deployed_mainnet_node.json
   ```

### Deploying a production relay

```bash
AWS_PROFILE=bingle-prod aws/deploy_relay.sh \
  --handle <relay-handle> --passphrase <pass> \
  --node-file /app/nodely_deployed_mainnet_node.json \
  --stack-name bingle-relay-prod \
  --cost-tag bingle_prod \
  --region <home-region> \
  --express
```

Notes:

- `--node-file /app/nodely_deployed_mainnet_node.json` selects the mainnet config
  baked in above.
- `--stack-name bingle-relay-prod` keeps the production stack separate from any dev
  stack.
- `--cost-tag bingle_prod` tags production spend (activate the tag key in
  **Billing → Cost allocation tags**).
- `--express` uses Fargate.
- Ensure `AWS_PROFILE` points at the production account
  (`aws sts get-caller-identity` to confirm).

---

## Verify a deployment

- ECS service reaches **RUNNING** with a healthy task.
- Tail the relay's **CloudWatch** log group.
- Confirm the relay **registers on-chain** and is reachable on its UDP port.

---

## Related commands

- `aws/stop_relay.sh` — stop a running relay (scale the service to zero).
- `aws/destroy_relay.sh` — tear down the CloudFormation stack.
