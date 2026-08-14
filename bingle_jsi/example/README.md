### BingleJsiExample

A React Native app that acts as a **Detox command-dispatcher harness** for the
`react-native-bingle-jsi` module (issue #109). It is the driver for the
end-to-end (e2e) test suite tracked under #32.

---

### What It Does

The app exposes a single command box instead of one screen per method. You type
a JSON command `{ "method": "...", "args": [...] }`; a dispatch table calls the
matching method on the `BingleJsi` proxy (plus `init` and the
`failureKindIsRetryable` helper), awaits it, and renders the JSON result.
Native callbacks (`onLog` / `onMessage` / `onListening`) append to an event
feed. Every element carries a stable `testID` so Detox can drive the entire API
surface with no per-method UI:

| `testID` | Element |
| --- | --- |
| `app-ready` | shows `ready` once event listeners are registered |
| `cmd-input` | JSON command box |
| `cmd-run` | run button |
| `cmd-status` | `idle` \| `running` \| `ok` \| `error` |
| `cmd-output` | JSON result (or error message) |
| `event-feed` | native callback log |

You can also drive it by hand in a simulator — type a command and tap **Run**.

> **React Native version:** the example is pinned to **RN 0.84.1** because Detox
> supports RN 0.77.x–0.84.x, while production runs 0.85.2. The native module is a
> classic bridge (`peerDependency react-native >=0.71`), so it links unchanged.
> Bumping back to 0.85 once Detox supports it is tracked in **#115**.

---

### Prerequisites

- **Node.js** ≥ 18 and **npm**
- **Xcode** ≥ 15 with command-line tools
- **CocoaPods** (`gem install cocoapods` or `brew install cocoapods`)
- **Rust** (via [rustup](https://rustup.rs/)) with iOS targets
- **swiftformat** (`brew install swiftformat`)

---

### Quick Start (iOS)

From the project root:

```bash
# One-time setup: build native libs, npm install, generate Xcode project, pod install
bash bingle_jsi/example/scripts/setup_example.sh

# Run on iOS Simulator
bash bingle_jsi/example/scripts/run_ios.sh
```

Or manually:

```bash
# 1. Build the bingle_jsi iOS native libraries
bash bingle_jsi/scripts/build_ios.sh

# 2. Install JS dependencies
cd bingle_jsi/example
npm install --legacy-peer-deps

# 3. Generate the Xcode project (first time only)
npx --yes @react-native-community/cli init BingleJsiExample --version 0.85.2 --directory /tmp/rn-init --skip-install --skip-git-init
cp -R /tmp/rn-init/ios/* ios/
rm -rf /tmp/rn-init

# 4. Install pods
cd ios && pod install && cd ..

# 5. Run
npx react-native run-ios
```

---

### After Rust Code Changes

If you modify the Rust code in `bingle_jsi/src/`, rebuild and re-run:

```bash
bash bingle_jsi/example/scripts/run_ios.sh --rebuild
```

This rebuilds the native iOS libraries, re-runs `pod install`, and launches
the app.

---

### End-to-end tests (Detox)

The e2e suite drives the harness in an iOS simulator. Detox iOS needs **no**
native project changes — it is configured entirely in `.detoxrc.js`; just run
`pod install` first (the Quick Start above covers that). One-time tool install:

```bash
npm install -g detox-cli && brew tap wix/brew && brew install applesimutils
```

Then, from `bingle_jsi/example/` (after `npm install` and `pod install`):

```bash
# Build the app in the Detox debug configuration
npm run e2e:build:ios

# Run the e2e suite against the simulator
npm run e2e:test:ios
```

The smoke test (`e2e/smoke.test.js`) launches the app, calls `version()` before
init, then inits, generates a keypair, and reads it back — proving the full
TypeScript → native → Rust path. Adding coverage for another method needs only a
new `e2e/*.test.js` using the `call({ method, args })` helper in
`e2e/harness.js`; no UI changes. Messaging (#111), typed failure causes (#112),
and account lifecycle (#113) build on this.

---

### Project Structure

```
example/
├── App.tsx                  # Detox command-dispatcher harness (issue #109)
├── index.js                 # React Native entry point
├── package.json             # Dependencies (react-native 0.84.1, detox, bingle_jsi)
├── app.json                 # App name/display name
├── .detoxrc.js              # Detox config (iOS sim.debug configuration)
├── e2e/
│   ├── jest.config.js       # Jest runner config for Detox
│   ├── harness.js           # runCommand / call / expectStatus helpers
│   └── smoke.test.js        # Smoke e2e (init → generateKeypair → readback)
├── metro.config.js          # Metro bundler config (resolves parent module)
├── babel.config.js          # Babel config
├── tsconfig.json            # TypeScript config
├── react-native.config.js   # Autolinking config for bingle_jsi
├── ios/
│   ├── Podfile              # CocoaPods config linking bingle_jsi
│   └── .xcode.env           # Xcode environment (NODE_BINARY)
├── scripts/
│   ├── setup_example.sh     # One-time setup (build + install + pod install)
│   └── run_ios.sh           # Build and run on iOS Simulator
└── README.md                # This file
```

---

### Console Output

When the app runs successfully, you should see in the Metro/Xcode console:

```
[BingleJsiExample] Initializing BingleJsi...
[BingleJsiExample] BingleJsi initialized successfully.
[BingleJsiExample] Calling version()...
[BingleJsiExample] Version: v0.1.0.42 (build 42, 2025-01-01T00:00:00Z)
```

---

### Troubleshooting

- **"BingleJsi native module not found"** — The native libraries are not
  linked. Run `setup_example.sh` or rebuild with `run_ios.sh --rebuild`.
- **Pod install fails** — Ensure `swiftformat` is installed and the
  XCFramework exists at `bingle_jsi/ios/BingleJsi.xcframework/`.
- **Build errors in Xcode** — Clean the build folder
  (`Product > Clean Build Folder`) and re-run `pod install`.
- **"Unable to Install" / duplicate hermes framework** — After upgrading
  React Native, stale `hermes.framework` artifacts from the previous version
  can conflict with the new `hermesvm.framework` (both share the same bundle
  identifier). Fix by cleaning all caches:
  ```bash
  # Remove DerivedData
  rm -rf ~/Library/Developer/Xcode/DerivedData
  # Erase simulator content (or delete the specific simulator)
  xcrun simctl erase all
  # Clean and reinstall pods
  cd bingle_jsi/example/ios
  rm -rf Pods Podfile.lock
  pod install
  # Rebuild
  cd ..
  npx react-native run-ios
  ```
