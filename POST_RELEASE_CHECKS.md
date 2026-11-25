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
