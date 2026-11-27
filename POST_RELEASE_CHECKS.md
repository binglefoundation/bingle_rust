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

### #516 ripples to a peer relay resume after it is reported failed, without a restart

- **Merged in:** the PR for #516; first ships in 0.3.42. The check runs on TestNet, after the release is rolled to both TestNet relays.
- **Done so far (2026-11-27, on the issue):** recovery after a graceful stop passed. Stopping relay A through ECS makes it sign off, and relay B drops it at once, so that run never reached the suspect path. The steps below make relay A fail without signing off.
- **Needs:**
  - both TestNet relays (relay A and relay B below), signed in to the TestNet AWS account;
  - a TestNet app build on a phone, to produce a failure report.
  Each relay stack has its own VPC (virtual private cloud) and subnet, so a network ACL (access control list) rule on relay A's subnet affects only relay A. Network ACLs are stateless, so the rule cuts traffic at once, which a security group change doesn't do for an existing session. Relay log groups are `/bingle/relay/<stack>`.
- **Steps:**
  1. Find relay A's network ACL:

         aws ec2 describe-network-acls --region <relay A region> --filters Name=association.subnet-id,Values=$(aws cloudformation describe-stack-resource --stack-name <relay A stack> --logical-resource-id Subnet --region <relay A region> --query StackResourceDetail.PhysicalResourceId --output text) --query 'NetworkAcls[0].NetworkAclId' --output text

  2. Follow relay B's log in one terminal:

         aws logs tail /bingle/relay/<relay B stack> --follow --region <relay B region> --format short

  3. Cut relay A off by denying inbound UDP to its relay port, for IPv4 and IPv6:

         aws ec2 create-network-acl-entry --network-acl-id <acl> --ingress --rule-number 50 --protocol 17 --port-range From=12121,To=12121 --cidr-block 0.0.0.0/0 --rule-action deny --region <relay A region> && aws ec2 create-network-acl-entry --network-acl-id <acl> --ingress --rule-number 51 --protocol 17 --port-range From=12121,To=12121 --ipv6-cidr-block ::/0 --rule-action deny --region <relay A region>

  4. Open the app on the phone, background and reopen it until relay B logs `relay_mark_suspect] relay <relay A id> reported failed`. The app reports relay A to relay B when relay selection tries A first, so it may take a few tries, or a phone whose preferred relay is A.
  5. Remove both rules:

         aws ec2 delete-network-acl-entry --network-acl-id <acl> --ingress --rule-number 50 --region <relay A region> && aws ec2 delete-network-acl-entry --network-acl-id <acl> --ingress --rule-number 51 --region <relay A region>

  6. Watch relay B's log for up to 10 minutes.
- **Passes when:**
  - in step 4, relay B logs `kept in the relay set as suspect`, not `removed relay`, and while A is cut off each ripple to it logs `ripple to relay <relay A id> (suspect) ... failed` at warn;
  - after step 5, relay B logs `no longer suspect` for relay A (`answered Available` or `heard from suspect relay`);
  - relay B's later ripples to relay A log no failure, with no restart of either relay.
- **If the run is abandoned:** remove both rules (step 5) anyway. Relay A stays cut off until they're gone.

### #554 a relay failure report says why the relay failed

- **Merged in:** the PR for #554; first ships in the next release. The reports come from the app, so the check needs a TestNet app build on the jsi release that has it, and the relays rolled to that release for the log line.
- **Needs:** the same setup as the #516 check above. Run it during that check: its step 4 makes the report this one reads.
- **Steps:** in the #516 check's step 4, find relay B's line `[handlers::on_report_fail] <app account> reported relay <relay A id> as "<fail_type>"`.
- **Passes when:** the `fail_type` is `"timeout"` or `"send_failed"`, not `"send_rejected"`.
- **Not checked here:** the ripple handler's `rippled a report ... as` line needs a third relay to receive the ripple, which TestNet's two relays can't show. Not reporting a relay the app's send backoff refused is covered by unit tests, because on a device the app's relay cooldown usually skips that relay before any probe.

### #464 Android shows the specific Delete my account error when offline

- **Merged in:** the PR for #464; first ships in the next release. The check needs an Android build of bingle_ux_rn on the `react-native-bingle-jsi` release that has it. No app change is needed: `deleteErrorText` in bingle_ux_rn's `src/screens/DeleteAccountScreen.tsx` already reads the variant name from the message.
- **Needs:** an Android phone or emulator with that TestNet Bingle Client build, signed in to a test account.
- **Steps:**
  1. Turn on airplane mode.
  2. Settings → Delete my account, and confirm.
  3. Turn airplane mode off.
- **Passes when:**
  - step 2 shows the app's text for an unreachable network (the `NoBlockchain` case), not its generic failure text;
  - after step 3 the account is still there: the failed deletion changed nothing.

### #470 a failed call's structured kind reaches JavaScript on both platforms

- **Merged in:** the PR for #470; first ships in the next release. The Swift bridge tests (`StructuredErrorBridgeTests`) and Kotlin unit tests (`BingleJsiErrorsTest`) passed locally before merge. The e2e test was written with the change but not run. bingle_ux_rn doesn't read `userInfo` yet (bingle_ux_rn #220 tracks that), so the example app checks it.
- **Needs:** the release's commit, with the iOS simulator framework and the Android libraries built for it (`BINGLE_IOS_SIM_ONLY=1 bash bingle_jsi/scripts/build_ios.sh`, `bash bingle_jsi/scripts/build_android.sh`), `npm install` in `bingle_jsi/example` (it has no lock file, so not `npm ci`), pods installed in `bingle_jsi/example/ios`, and an Android emulator.
- **Steps:** run the lifecycle e2e on each platform, from `bingle_jsi/example`:

      npm run e2e:build:ios && npm run e2e:test:ios -- e2e/lifecycle.test.ts

      npm run e2e:build:android && npm run e2e:test:android -- e2e/lifecycle.test.ts

- **Passes when:** `rejects a bad call with the variant name and a structured kind` passes on both platforms: the rejection has `userInfo.kind`, and on Android its message starts with the variant name, as on iOS.

