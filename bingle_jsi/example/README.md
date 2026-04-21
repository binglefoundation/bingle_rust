### BingleJsiExample

A minimal React Native app that demonstrates the `react-native-bingle-jsi`
module by calling `initBingleJsi` and `version()`, logging the results to the
console.

---

### What It Does

On launch the app:

1. Calls `initBingleJsi(config)` to create the native Bingle API
2. Calls `BingleJsi.version()` to retrieve version info
3. Logs both results to the console (`[BingleJsiExample] ...`)
4. Displays the version string on screen (or an error message)

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

### Project Structure

```
example/
├── App.tsx                  # Main app component (init + version + log)
├── index.js                 # React Native entry point
├── package.json             # Dependencies (react-native 0.85, bingle_jsi)
├── app.json                 # App name/display name
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
