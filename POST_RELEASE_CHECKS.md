# Post-release checks

Checks that can only run once a change is released: on a device, in the app (bingle_ux_rn) after it picks up the new `react-native-bingle-jsi`, or on MainNet. CI can't cover them, so the issue stays open until someone runs the check.

`scripts/release.sh` lists the checks below when a release finishes.

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

- **Merged in:** this PR; first shipped in 0.3.39.
- **Needs:** the app creator account for each network, to update its DApp in place (UpdateApplication), and `APP_ADMIN` for the check. The new approval program is 2,340 bytes, so the app must have at least one extra program page (the current contract, 2,309 bytes, already needs one).
- **Steps:**
  1. Update the TestNet DApp to the release's contract, then the MainNet one.
  2. On TestNet, block and unblock a test account with the admin tool's `moderator_block` (or `AlgoBingle::set_blocked_user`), reading the bit back with `check_blocked_user` each time.
- **Passes when:** block reads back `Some(true)`, unblock `Some(false)`, and the account's other flags are unchanged.

### #483 direct send after the sender's NAT moves it

- **Merged in:** #488; first shipped in 0.3.38.
- **Needs:** the email gateway (`bingle_cli logclient`) rebuilt on 0.3.38, and the app (bingle_ux_rn) on `react-native-bingle-jsi` 0.3.38 on a phone behind NAT (mobile data).
- **Steps:**
  1. Send the gateway a message from the phone, and check it is delivered directly.
  2. Leave the app open and idle for at least 5 minutes, then send another.
  3. In the gateway's log, look for a second `run_accept_stream starting` from the phone's address, followed by `handshake complete`.
- **Passes when:** the second message is delivered directly (`handshake complete`, then the message), not through the Mailbox, and the gateway logs no `DTLS accept handshake timeout`.
