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

### #455 jsi bridges stop the engine when React Native reloads

- **Merged in:** #456, ships in 0.3.35.
- **Needs:** bingle_ux_rn on `react-native-bingle-jsi` 0.3.35 or later, as a Debug build loading its JavaScript from Metro. Use the iOS simulator and an Android emulator or device, both on TestNet, with a second account to send from.
- **Steps:**
  1. Start the app, let it go online, and count its UDP sockets with the command for the platform.
     - iOS simulator:
       ```
       lsof -nP -a -p $(pgrep -f "bingle_ux.app/bingle_ux" | head -1) -iUDP | tail -n +2 | awk '{print $9}' | sort -u | wc -l
       ```
     - Android (generic flavour, `com.bingle_ux`):
       ```
       adb shell "U=\$(stat -c %u /proc/\$(pidof com.bingle_ux)); cat /proc/net/udp /proc/net/udp6 | awk -v u=\$U '\$8==u' | wc -l"
       ```
  2. Reload the JavaScript three times (⌘R in the simulator, `r` in Metro), wait for it to go online, and count again.
  3. Send three messages from the second account.
- **Passes when:** the UDP socket count is the same after the reloads as before (before the fix, each reload added an engine's sockets), and every message shows in the app. None is acknowledged as `Direct` without appearing.
