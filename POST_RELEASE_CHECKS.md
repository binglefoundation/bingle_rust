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
- **CloudFormation template changes** need no entry of their own: each roll applies the checked-out relay and gateway templates to every stack in the inventory, showing each change set first (#528, `RELEASING.md`). Add an entry only for a behaviour to check once the change runs.
- **App log lines:** a Release build of the app (bingle_ux_rn) passes only errors from the engine to the device log. A check that relies on an `info` line needs a Debug build run from Xcode or Metro, so say so under **Needs**, or give another way to tell the result.

## Waiting for a release

### #528 a relay roll applies the current CloudFormation templates

- **Merged in:** the PR that closes #528; first ships in the next release. The check runs on TestNet, from a `staging` commit that has it.
- **Needs:** the TestNet relay inventory with both TestNet relays, signed in to the TestNet Amazon Web Services (AWS) account. A TestNet roll runs only a commit on `origin/staging`, so the template-only change goes in as a small PR to `staging`, and a follow-up PR removes it.
- **Steps:**
  1. Sign in and check out the head of `staging` in a clean worktree:

         git worktree add tmp/roll-528 origin/staging && cd tmp/roll-528 && source ~/.config/bingle/bingle_env.sh testnet

  2. Dry-run with no template change, to see the baseline:

         scripts/roll_relays.sh --dry-run

  3. Merge a template-only PR to `staging`: a new environment variable `BINGLE_ROLL_CHECK=528` in the relay container of both `aws/relay_stack.yaml` and `aws/relay_express.yaml`. Then pull it into the worktree and dry-run again:

         git pull --ff-only origin staging && scripts/roll_relays.sh --dry-run

  4. Roll:

         scripts/roll_relays.sh

  5. Read the variable from each relay's running task definition (`<cluster>` and `<service>` from the stack's resources):

         aws ecs describe-task-definition --task-definition "$(aws ecs describe-services --cluster <cluster> --services <service> --region <region> --query 'services[0].taskDefinition' --output text)" --region <region> --query "taskDefinition.containerDefinitions[0].environment[?name=='BINGLE_ROLL_CHECK']"

  6. Remove the variable in a follow-up PR, and roll again once it merges.
- **Passes when:**
  - the step 2 dry run shows, for each relay, only the task definition replaced and the service updated (or nothing to change), and says a roll would execute it without asking;
  - the step 3 dry run shows the same, for both relays, with no other resource;
  - the step 4 roll executes both change sets without a prompt, and both relays pass the start check;
  - step 5 shows `BINGLE_ROLL_CHECK` = `528` on both relays;
  - no stack has a change set left behind (`aws cloudformation list-change-sets --stack-name <stack> --region <region>` is empty).

### #531 the app's moderator blocklist switch takes effect without a restart

- **Merged in:** the PR for #531; first ships in the next release.
- **Needs:** a TestNet build of the app (bingle_ux_rn) with bingle_ux_rn #213 (the Settings switch that calls `setGlobalBlock` and saves the choice), not yet built, and a second TestNet account the phone can receive messages from.
- **Steps:**
  1. Block the second account:

         bingle_admin moderator_block <SENDER_ADDRESS> --block --accounts <TESTNET_ACCOUNTS_DIR> --node-file <TESTNET_NODE_FILE>

  2. With the switch on, send a message from the second account to the phone.
  3. Turn the switch off in Settings, without restarting, and send another message from the second account.
  4. Restart the app and check the switch is still off; then turn it back on and send a third message.
  5. Unblock the account (`--unblock`).
- **Passes when:** the first and third messages never appear, the second does, and the switch keeps its setting across the restart.

### #539 a moderator-blocked user who isn't a contact can't be found or messaged

- **Merged in:** the PR for #539; first ships in the next release.
- **Needs:** a TestNet build of the app (bingle_ux_rn) with this release of bingle_jsi and the moderator blocklist on, and a second registered TestNet account, with handle `<BLOCKED_HANDLE>`, that isn't one of the phone's contacts.
- **Steps:**
  1. Check the handle is found: search for `<BLOCKED_HANDLE>` in New Message.
  2. Block the second account:

         bingle_admin moderator_block <SENDER_ADDRESS> --block --accounts <TESTNET_ACCOUNTS_DIR> --node-file <TESTNET_NODE_FILE>

  3. Restart the app, or wait 10 minutes (the "not blocked" refresh), and search for `<BLOCKED_HANDLE>` again.
  4. If the app can still address the handle (for example from a link or by typing it in full), try to send it a message.
  5. Unblock it (`--unblock`) and restart the app.
- **Passes when:** before the block the handle is found; after it, the search doesn't return it and any send to it is refused, with nothing delivered to the second account; after the unblock and restart, it is found again.

### #516 ripples to a peer relay resume after it is reported failed, without a restart

- **Merged in:** the PR for #516; first ships in the next release. The check runs on TestNet, after the release is rolled to both TestNet relays.
- **Needs:** both TestNet relays (relay A and relay B below), signed in to the TestNet AWS account; a TestNet app build on a phone, to produce a failure report. Relay log groups are `/bingle/relay/<stack>`.
- **Steps:**
  1. Follow relay B's log in one terminal:

         aws logs tail /bingle/relay/<relay B stack> --follow --region <relay B region> --format short

  2. Stop relay A:

         aws ecs update-service --cluster <relay A cluster> --service <relay A service> --desired-count 0 --region <relay A region>

  3. Open the app on the phone, background and reopen it until relay B logs `relay_mark_suspect] relay <relay A id> reported failed`. The app reports relay A to relay B when relay selection tries A first, so it may take a few tries, or a phone whose preferred relay is A.
  4. Start relay A again:

         aws ecs update-service --cluster <relay A cluster> --service <relay A service> --desired-count 1 --region <relay A region>

  5. Wait for relay A to pass its start check, then watch relay B's log for up to 10 minutes.
- **Passes when:**
  - in step 3, relay B logs `kept in the relay set as suspect`, not `removed relay`, and while A is down each ripple to it logs `ripple to relay <relay A id> (suspect) ... failed` at warn;
  - after step 4, relay B logs `no longer suspect` for relay A (`answered Available` or `heard from suspect relay`);
  - relay B's later ripples to relay A log no failure, with no restart of relay B;
  - an account that advertises on relay B (the TestNet gateway, if it is on B) is found by a lookup on relay A after its next advert refresh: no `found: false` for it in relay A's log.

