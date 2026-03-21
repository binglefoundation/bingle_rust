# Staging Deployment Guide

This guide describes how to deploy the Bingle DAPP to the Algorand Testnet as a staging application with its own `app_id` and `asset_id`.

## 1. Prerequisites

- [AlgoKit CLI](https://github.com/algorandfoundation/algokit-cli) installed.
- `bingle_admin` CLI tool built and in your PATH (see `bingle_admin` repository).
- `nodely_staging_testnet_node.json` configuration file (created in this repository).
- A Testnet account with enough ALGO for deployment.

## 2. Compile Smart Contracts

Before deploying, you must compile the Python smart contracts to TEAL.

```bash
cd dapp/projects/dapp
algokit project run build
```

The compiled TEAL files will be located in:
`dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/`

## 3. Initial Staging Deployment (New App & New Asset)

To deploy a brand new staging application and a new Bingle$ asset:

(from project root)

```bash
bingle_admin deploy dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp \
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
bingle_admin deploy dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp \
  --app-id <CURRENT_APP_ID> \
  --node-file nodely_staging_testnet_node.json
```

*Note: The `asset_id` is automatically picked up from the node file. If you haven't set the `BINGLE_PASSPHRASE` env var, remember to add `--passphrase`.*

### Update with `app_id` change (New App, Same Asset)

To deploy a new application instance while reusing the existing Bingle$ asset:

```bash
bingle_admin deploy dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp \
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
