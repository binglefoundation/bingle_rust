# Post-release checks

Checks that can only run once a change is released: on a device, in the app (bingle_ux_rn) after it picks up the new `react-native-bingle-jsi`, or on MainNet. CI can't cover them, so the issue stays open until someone runs the check.

When a release finishes, `scripts/release.sh` lists the checks below for changes that release ships.

## How to use this list

- **Adding a check:** a PR that leaves a check for after release adds an entry here, in the same PR. Write it in the PR description as "Part of #N" so the issue stays open.
- **Running a check:** after a release that includes the change, follow the entry's steps.
  - If it passes, post the result on the issue and close it, then remove the entry in a PR.
  - If it fails, post what happened on the issue and keep the entry.
- **Entry format:** `### #N short title`, then:
  - **Merged in:** the PR, and the first release version that ships it;
  - **Needs:** the builds, devices or networks;
  - **Steps:** single-line commands where possible;
  - **Passes when:** the expected result.

## Waiting for a release

### #490 blocked_user bit on the DApps

- **Merged in:** #503; first shipped in 0.3.39.
- **Needs:** the TestNet and MainNet accounts directories (`APP_CREATOR` signs the update, `APP_ADMIN` the block) and node files, and a TestNet account that is opted in to the DApp to block and unblock. The new approval program is 2,340 bytes; the deployed apps already need one extra program page (the current one is 2,309 bytes), so it fits.
- **Steps** (from a checkout of the release tag; the `bingle_admin` runbook is `admin_docs/UPGRADE_APP.md`, Path A):
  1. Build the contract and check the change is an in-place update. Expect `approval/clear program differs from staging, but the state schema is unchanged`:

         scripts/build_dapp.sh

  2. Update the TestNet DApp. Expect `update complete: app_id=<APP_ID>, asset_id=<ASSET_ID>`:

         bingle_admin deploy dapp_projects/smart_contracts/artifacts/bingle_dapp --accounts <TESTNET_ACCOUNTS_DIR> --node-file <TESTNET_NODE_FILE>

  3. Block, then unblock, a TestNet account with the admin tool's `moderator_block` (bingle_admin issue 40), which reads the bit back after each write:

         bingle_admin moderator_block <ADDRESS> --block --accounts <TESTNET_ACCOUNTS_DIR> --node-file <TESTNET_NODE_FILE>

         bingle_admin moderator_block <ADDRESS> --unblock --accounts <TESTNET_ACCOUNTS_DIR> --node-file <TESTNET_NODE_FILE>

     To read the bit independently after each, decode the account's flags word (key `allow_static`; bit 5 is the block). Python, not jq, since the word's top bit is beyond a float's precision:

         curl -s https://testnet-api.4160.nodely.dev/v2/accounts/<ADDRESS>/applications/<APP_ID> | python3 -c "import json,sys; kv=json.load(sys.stdin)['app-local-state'].get('key-value', []); v=next((x['value']['uint'] for x in kv if x['key'] == 'YWxsb3dfc3RhdGlj'), 0); print('blocked' if v & 32 else 'not blocked', hex(v))"

  4. Update the MainNet DApp the same way:

         bingle_admin deploy dapp_projects/smart_contracts/artifacts/bingle_dapp --accounts <MAINNET_ACCOUNTS_DIR> --node-file <MAINNET_NODE_FILE>

  5. Commit the `dapp_projects/smart_contracts/bingle_dapp/app_schema.json` that step 1 updated (its new `approval_sha256`) in a bingle_rust pull request, so the next build compares against the deployed program.
- **Passes when:** both updates report `update complete` with the existing app and asset ids; on TestNet the read prints `blocked` after the block and `not blocked` after the unblock, and the rest of the printed word is the same before and after.

### #483 direct send after the sender's NAT moves it

- **Merged in:** #488; first shipped in 0.3.38.
- **Needs:** the email gateway (`bingle_cli logclient`) rebuilt on 0.3.38, and the app (bingle_ux_rn) on `react-native-bingle-jsi` 0.3.38 on a phone behind NAT (mobile data).
- **Steps:**
  1. Send the gateway a message from the phone, and check it is delivered directly.
  2. Leave the app open and idle for at least 5 minutes, then send another.
  3. In the gateway's log, look for a second `run_accept_stream starting` from the phone's address, followed by `handshake complete`.
- **Passes when:** the second message is delivered directly (`handshake complete`, then the message), not through the Mailbox, and the gateway logs no `DTLS accept handshake timeout`.
