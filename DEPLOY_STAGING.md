# Staging Deployment Guide

This guide describes how to deploy the Bingle DAPP to the Algorand Testnet as a staging application with its own `app_id` and `asset_id`.

## 1. Prerequisites

- [AlgoKit CLI](https://github.com/algorandfoundation/algokit-cli) installed.
- `bingle_admin` CLI tool built and in your PATH (see `bingle_admin` repository).
- `nodely_staging_testnet_node.json` configuration file (created in this repository).
- A Testnet account with enough ALGO for deployment.

## 2. Compile Smart Contracts

Before deploying, you must compile the Python smart contracts to TEAL. Use the
build script from the project root — it verifies the Python 3.12+ / AlgoKit /
Poetry toolchain, bootstraps the environment (safe on a clean checkout), builds,
and then checks the schema against `master` (see below):

```bash
scripts/build_dapp.sh
```

The compiled TEAL files will be located in:
`dapp_projects/smart_contracts/artifacts/bingle_dapp/`

(The lower-level `cd dapp_projects && algokit project run build` still works if you
only want to compile without the environment and schema checks.)

### Schema check: does this need a new app?

The generated artifacts are not committed. Instead, `build_dapp.sh` maintains a
small tracked baseline — `dapp_projects/smart_contracts/bingle_dapp/app_schema.json`
— recording the app's **state schema** plus the approval/clear program hashes, and
compares it against the copy committed on `master`. Its warning tells you which
deploy path below to take:

- **"app state schema differs"** — the state schema is fixed at app creation, so
  the existing app cannot be updated. Use **§3** (or §4 "New App, Same Asset") to
  deploy a **new** app.
- **"approval/clear program differs"** — schema is unchanged; roll out the new
  program with an in-place **§4 update** (same `app_id`).
- **"matches master"** — nothing to deploy.
- **"no schema baseline committed on master"** — treat as a fresh app (§3).

After a successful deploy, commit the regenerated `app_schema.json` so `master`
becomes the new baseline for the next comparison.

## 3. Initial Staging Deployment (New App & New Asset)

To deploy a brand new staging application and a new Bingle$ asset:

(from project root)

```bash
bingle_admin deploy dapp_projects/smart_contracts/artifacts/bingle_dapp \
  --new-app \
  --new-asset \
  --node-file nodely_staging_testnet_node.json \
  --passphrase "YOUR_DEPLOYER_MNEMONIC"
```

*Tip: You can set the `BINGLE_PASSPHRASE` environment variable instead of using `--passphrase`.*

Once successful, the command will print the new `Application ID` and `Asset ID`. Update your `nodely_staging_testnet_node.json` with these values.

## 4. Updating the Staging Application

### Update without `app_id` change (Same App, Same Asset)

To update the existing application code while keeping the same application ID and asset:

```bash
bingle_admin deploy dapp_projects/smart_contracts/artifacts/bingle_dapp \
  --app-id <CURRENT_APP_ID> \
  --node-file nodely_staging_testnet_node.json
```

*Note: The `asset_id` is automatically picked up from the node file. If you haven't set the `BINGLE_PASSPHRASE` env var, remember to add `--passphrase`.*

### Update with `app_id` change (New App, Same Asset)

To deploy a new application instance while reusing the existing Bingle$ asset:

```bash
bingle_admin deploy dapp_projects/smart_contracts/artifacts/bingle_dapp \
  --new-app \
  --node-file nodely_staging_testnet_node.json
```

*Note: Since `--new-asset` is NOT provided, it will use the `asset_id` found in `nodely_staging_testnet_node.json`. After deployment, you MUST update the `app_id` in your `nodely_staging_testnet_node.json` to the new ID.*

## 5. Summary of ID Precedence in `bingle_admin deploy`

- **App ID**:
  - `--new-app`: Creates a new app (ignores `app_id` in node file or env).
  - `--app-id <id>`: Updates the specified app.
  - Neither: Creates a new app.
- **Asset ID**:
  - `--new-asset`: Creates a new asset (ignores node file and env).
  - Else, use `asset_id` from `--node-file` (if present).
  - Else, use `BINGLE_ASSET_ID` environment variable (if set).
  - Else, if `--new-app` is set, a new asset is created implicitly.
