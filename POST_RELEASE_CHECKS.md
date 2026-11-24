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

### #95 the app drops messages from a blocked sender

- **Merged in:** #504; first shipped in 0.3.39.
- **Needs:** the #490 check done on TestNet (the DApp has `set_blocked_user`), a TestNet build of the app (bingle_ux_rn) on `react-native-bingle-jsi` 0.3.39 that passes `enable_global_block: true` to `init`, and a second TestNet account to send from.
- **Steps:**
  1. From the second account, send the phone a message, and check it arrives.
  2. Block the second account:

         bingle_admin moderator_block <SENDER_ADDRESS> --block --accounts <TESTNET_ACCOUNTS_DIR> --node-file <TESTNET_NODE_FILE>

  3. Wait 10 minutes (the "not blocked" refresh), or restart the app, then send another message from the second account, once directly and once while the phone is offline (so it goes through the Mailbox).
  4. Unblock it (`--unblock`). The app still drops its messages until restarted, since a block is kept for good in the running app.
- **Passes when:** neither message sent after the block appears on the phone, and the device log shows `which a moderator has blocked` for each.
