# bingle_jsi

Bingle is a decentralized, peer-to-peer messaging protocol that lets users communicate securely and privately
— so your conversations stay yours, with nobody able to read or shut them down.

Bingle runs with no centralized server and uses end-to-end encryption, so there is no central infrastructure for third parties to compromise.
Key management uses the Algorand blockchain to prevent impersonation, while messaging runs over the established
DTLS (Datagram Transport Layer Security) protocol.
A low-cost funding mechanism incentivizes the provision of relay nodes, keeping the network robust and resilient.

## Role in Bingle

`bingle_jsi` is the React Native JSI bridge for Bingle. It generates iOS (Swift) and Android
(Kotlin) bindings from the Rust API via [uniffi](https://mozilla.github.io/uniffi-rs/) so mobile
apps can call the Bingle engine in
[`bingle_core`](https://github.com/binglefoundation/bingle_rust) directly.

It is part of [`bingle_rust`](https://github.com/binglefoundation/bingle_rust), the Rust reference implementation of
the Bingle protocol.

> **Android is under development.** The iOS bridge is the primary supported target. The Android
> bridge now builds and links, and the example app runs the Detox e2e harness on an emulator (issue
> #130); messaging/failure-cause coverage and a CI lane are in progress (issues #131, #132). Treat
> Android as not yet production-ready.

## Installing

`bingle_jsi` is a React Native bridge crate and is not published to crates.io — it is consumed from
a clone of the repository, cross-compiled into native libraries by the platform build scripts, and
added to a React Native app as a local package. This requires the Rust stable toolchain (2024
edition, Rust 1.85 or newer) via [rustup](https://rustup.rs):

```bash
git clone https://github.com/binglefoundation/bingle_rust.git
```

See [Using in a React Native Application](#using-in-a-react-native-application) below for the full
setup. The rest of this document is the detailed module reference.

---

### Bingle JSI — React Native Module

React Native native module for Bingle peer-to-peer messaging, built with
Rust and [uniffi](https://mozilla.github.io/uniffi-rs/) proc macros.

The Rust crate (`bingle_jsi`) exposes the full Bingle API over uniffi.
Platform-specific build scripts cross-compile the crate and generate
native bindings (Swift for iOS, Kotlin for Android). A TypeScript layer
provides type definitions for the React Native side.

---

### Directory Layout

```
bingle_jsi/
├── src/                          # Rust source (uniffi crate)
│   ├── lib.rs
│   └── api/
│       ├── mod.rs
│       ├── types.rs              # uniffi Records & Enums
│       ├── error.rs              # BingleJsiError
│       ├── callback.rs           # MessageCallback trait
│       ├── bingle_jsi_api.rs     # BingleJsiApi trait (20 methods)
│       └── bingle_jsi_api_impl.rs# Concrete implementation + create_bingle_api()
├── scripts/
│   ├── build_ios.sh              # iOS cross-compile + XCFramework
│   ├── build_android.sh          # Android cross-compile + jniLibs
│   ├── run_ios_bridge_tests.sh   # run Layer 2 Swift XCTests headlessly
│   ├── add_swift_test_target.rb  # one-time: add BingleJsiBridgeTests Xcode target
│   └── fix_test_target_settings.rb # one-time: configure test target build settings
├── ios/
│   ├── generated/                # (created by build_ios.sh) Swift + C headers
│   ├── BingleJsi.xcframework/    # (created by build_ios.sh)
│   ├── BingleJsiBridge.swift     # React Native ↔ uniffi bridge
│   └── BingleJsiBridge.m         # Objective-C bridge method declarations
├── example/
│   └── ios/
│       └── BingleJsiBridgeTests/ # Swift XCTest target (Layer 2 tests)
│           ├── MockBingleJsiApi.swift     # mock BingleJsiApiProtocol
│           └── BingleJsiBridgeTests.swift # 17 test cases
├── android/
│   ├── build.gradle              # Android library configuration
│   ├── generated/                # (created by build_android.sh) Kotlin bindings
│   └── src/main/jniLibs/        # (created by build_android.sh) .so files
├── ts/
│   ├── NativeBingleJsi.ts        # TypeScript type definitions
│   └── index.ts                  # Module entry point (re-exports)
├── cpp/                          # (reserved for C++ JSI bridge if needed)
├── tests/                        # Rust tests
├── Cargo.toml                    # Rust crate configuration
├── package.json                  # NPM package configuration
├── bingle_jsi.podspec            # CocoaPods spec for iOS
└── README.md                     # This file
```

---

### Prerequisites

#### Common

- **Rust** (via [rustup](https://rustup.rs/)) — stable toolchain
- **uniffi-bindgen** — provided as a binary target in the `bingle_jsi` crate
  (`src/bin/uniffi-bindgen.rs`). No separate install is needed; the build
  scripts invoke it automatically via `cargo run -p bingle_jsi --bin uniffi-bindgen`.

#### iOS

- **macOS** with **Xcode** (command-line tools installed)
- **swiftformat** (installed via Homebrew) — used by `uniffi-bindgen` to auto-format
  generated Swift bindings. Install with: `brew install swiftformat`
- Rust iOS targets (installed automatically by the build script):
  - `aarch64-apple-ios` (device)
  - `aarch64-apple-ios-sim` (Apple Silicon simulator)
  - `x86_64-apple-ios` (Intel simulator, optional)

#### Android

- **ktlint** (installed via Homebrew) — used by `uniffi-bindgen` to auto-format
  generated Kotlin bindings. Install with: `brew install ktlint`
- **Android NDK** — set `ANDROID_NDK_HOME` or install via Android Studio
  SDK Manager (the script auto-detects from `ANDROID_HOME` or
  `~/Library/Android/sdk/ndk/`)
- Rust Android targets (installed automatically by the build script):
  - `aarch64-linux-android` (arm64-v8a)
  - `armv7-linux-androideabi` (armeabi-v7a)
  - `x86_64-linux-android` (x86_64)

---

### Building

#### iOS

From the project root:

```bash
bash bingle_jsi/scripts/build_ios.sh
```

This will:
1. Cross-compile the `bingle_jsi` crate for iOS device and simulator targets
2. Generate Swift bindings and a C header via `uniffi-bindgen`
3. Create a universal simulator library (lipo) if x86_64 target is available
4. Package everything into `bingle_jsi/ios/BingleJsi.xcframework`

**Output:**
- `bingle_jsi/ios/BingleJsi.xcframework` — static library XCFramework
- `bingle_jsi/ios/generated/` — Swift bindings (`.swift`) and C header (`.h`)

#### Android

> **Under development and unsupported.** The Android build is not currently supported and may be
> incomplete or broken. The steps below are for development only.

From the project root:

```bash
bash bingle_jsi/scripts/build_android.sh
```

This will:
1. Cross-compile the `bingle_jsi` crate for arm64-v8a, armeabi-v7a, and x86_64
2. Copy the shared libraries (`.so`) into `android/src/main/jniLibs/`
3. Generate Kotlin bindings via `uniffi-bindgen`

**Output:**
- `bingle_jsi/android/src/main/jniLibs/{arm64-v8a,armeabi-v7a,x86_64}/libbingle_jsi.so`
- `bingle_jsi/android/generated/` — Kotlin bindings

#### Both Platforms

```bash
cd bingle_jsi
npm run build
```

Or individually:
```bash
npm run build:ios
npm run build:android
```

---

### Using in a React Native Application

#### Step 1: Add the Module

Add `react-native-bingle-jsi` as a dependency. For a local development
setup, use a file reference in your React Native app's `package.json`:

```json
{
  "dependencies": {
    "react-native-bingle-jsi": "file:../path/to/bingle_jsi"
  }
}
```

Then run:
```bash
npm install
```

#### Step 2: Build Native Libraries

Before the first build (and after any Rust code changes), run the
platform build scripts:

```bash
# From the bingle_jsi directory (or project root)
bash bingle_jsi/scripts/build_ios.sh      # for iOS
bash bingle_jsi/scripts/build_android.sh   # for Android
```

#### Step 3: iOS — Pod Install

```bash
cd ios
pod install
cd ..
```

The `bingle_jsi.podspec` links the XCFramework and Swift bindings
automatically.

#### Step 4: Android — Register the Package

The `android/build.gradle` in the module configures jniLibs and Kotlin
bindings. You must register `BingleJsiPackage` in your app's
`MainApplication.java` (or `.kt`):

```kotlin
import com.bingle.jsi.BingleJsiPackage

// In getPackages():
packages.add(BingleJsiPackage())
```

Then sync your Gradle project in Android Studio or run:

```bash
cd android
./gradlew sync
cd ..
```

#### Step 5: Use in TypeScript

All native methods are **async** (Promise-based) because they cross the
React Native bridge.

```typescript
import {
  BingleJsi,
  initBingleJsi,
  BingleJsiConfig,
  BingleMessage,
  ContactSource,
} from 'react-native-bingle-jsi';

// Initialize — must be called before any other method
const config: BingleJsiConfig = {
  handle: 'alice',
  passphrase: null,
  relay: false,
  static_ip: null,
  stun_servers: null,
  stun_servers_file: null,
  node_file: '/path/to/node.json',
  log_level: 'info',
  app_id: 12345,
  asset_id: 67890,
  handle_cache_expiry_secs: 300,
  debug: false,
  local: '/path/to/local_state.json',
};

await initBingleJsi(config);

// Get version info
const version = await BingleJsi.version();
console.log(`Bingle v${version.version}`);

// Send a message
const msg: BingleMessage = {
  app: null,
  type: null,
  tag: null,
  response_tag: null,
  text: 'Hello from React Native!',
  data: null,
};
await BingleJsi.sendMessageToHandle('bob', msg);

// Local API (when `local` is set in config)
const keypair = await BingleJsi.generateKeypair();
console.log(`Generated keypair: ${keypair.id}`);
await BingleJsi.addContact('bob', 'BOB_ALGO_ADDRESS', ContactSource.Manual);
const contacts = await BingleJsi.getContacts();
```

---

### Native Module Architecture

The module uses a two-layer architecture:

1. **Rust → uniffi bindings**: The `bingle_jsi` Rust crate is compiled to
   a static library (iOS) or shared library (Android). `uniffi-bindgen`
   generates Swift and Kotlin bindings that call the Rust FFI layer.

2. **uniffi bindings → React Native bridge**: Platform-specific bridge
   classes wrap the uniffi-generated API and register it as a React Native
   native module named `"BingleJsi"`:
   - **iOS**: `ios/BingleJsiBridge.swift` + `ios/BingleJsiBridge.m`
     (extends `RCTEventEmitter`, registered via `RCT_EXTERN_MODULE`)
   - **Android**: `android/src/main/java/com/bingle/jsi/BingleJsiModule.kt`
     + `BingleJsiPackage.kt` (extends `ReactContextBaseJavaModule`,
     registered via `ReactPackage`)

3. **TypeScript**: `ts/index.ts` acquires the native module via
   `NativeModules.BingleJsi` and exports `initBingleJsi()` plus the
   typed `BingleJsi` proxy object.

All bridge methods are **Promise-based** — they dispatch work to a
background thread and resolve/reject via the React Native bridge.

---

### Running Rust Tests

The Rust crate has its own test suite:

```bash
# Run bingle_jsi tests only
cargo test -p bingle_jsi

# Run all workspace tests
cargo test
```

---

### iOS Bridge Tests (Layer 2 — Swift XCTest)

The Swift bridge (`BingleJsiBridge.swift`) is tested independently of the
real Bingle engine using a mock implementation of `BingleJsiApiProtocol`.
This lets the full call path through the bridge be exercised — including
Promise resolution, error propagation, and the "not initialized" rejection
path — without a passphrase, a live network, or a running Bingle node.

#### How it works

`BingleJsiBridge.swift` holds its API instance as `(any BingleJsiApiProtocol)?`
rather than the concrete `BingleJsiApi` type. A package-internal
`injectApi(_ api:)` method allows a test-supplied mock to be injected
before any test case runs.

The test target (`BingleJsiBridgeTests`) lives in:

```
bingle_jsi/example/ios/BingleJsiBridgeTests/
├── MockBingleJsiApi.swift      # mock implementing all protocol methods
└── BingleJsiBridgeTests.swift  # 24 XCTest cases
```

The mock records every call, exposes configurable return values, and can
be made to throw on demand — enabling both happy-path and error-path
coverage.

A `SpyBingleJsiBridge` subclass overrides `sendEvent(withName:body:)` to
capture React Native events in memory, allowing tests to assert the exact
event name and payload delivered to the JS layer.

Tests covered:

| Test | What it exercises |
|------|-------------------|
| `testHandleLookup_resolvesWithExpectedUserId` | successful handle → user-id lookup |
| `testHandleLookup_rejectsOnApiError` | bridge rejects when mock throws |
| `testHandleLookup_notInitialized_rejectsWithCorrectCode` | rejection before `initialize` |
| `testSendMessageToId_resolvesAndRecordsCall` | message forwarded to mock, bool result resolved |
| `testSendMessageToId_mapsAllMessageFields` | all message fields (including `cipher_suite`) mapped correctly |
| `testSendMessageToId_rejectsOnApiError` | bridge rejects when mock throws |
| `testSendMessageToId_notInitialized_rejectsWithCorrectCode` | rejection before `initialize` |
| `testSendMessageToHandle_resolvesAndRecordsCall` | handle-based send forwarded to mock |
| `testVersion_resolvesWithVersionInfo` | `VersionInfo` fields returned via Promise |
| `testIsStarted_resolvesWithTrueWhenStarted` | `true` resolved when mock returns started |
| `testIsStarted_resolvesWithFalseWhenNotStarted` | `false` resolved when mock returns not started |
| `testStart_callsApiStart` | `start()` forwarded and resolved |
| `testSetMessageCallback_registersCallback` | callback registered on mock |
| `testSetMessageCallback_deliversMessageToEventEmitter` | inbound message fires `onMessage` event with correct payload |
| `testKeypairStatus_resolvesWithExpectedFields` | `KeypairStatusResponse` fields resolved |
| `testGetNatType_resolvesWithNatTypeString` | `NatTypeResponse` nat type string resolved |
| `testGenerateKeypair_resolvesWithKeypairFields` | `Keypair` id and passphrase resolved |
| `testIsBlocked_resolvesWithFalseByDefault` | `false` resolved for unblocked contact |
| `testIsBlocked_resolvesWithTrueForBlockedContact` | `true` resolved for blocked contact |
| `testGetMessages_includesCipherSuite` | `cipher_suite` present (or nil) for each returned message |

#### Prerequisites

- macOS with Xcode installed
- CocoaPods (`brew install cocoapods`)
- An iOS 18.x simulator runtime available (`xcrun simctl list runtimes`)
- The example workspace already pod-installed:
  ```bash
  cd bingle_jsi/example/ios && pod install
  ```

The XCFramework (`bingle_jsi/ios/BingleJsi.xcframework`) must exist.
If it is missing, build it first:

```bash
bash bingle_jsi/scripts/build_ios.sh
```

#### Running the tests

Use the provided script from the project root:

```bash
./bingle_jsi/scripts/run_ios_bridge_tests.sh
```

The script will:
1. Boot the `iPhone 16` (iOS 18.6) simulator if it is not already running
2. Run `xcodebuild test` against the `BingleJsiBridgeTests` scheme
3. Write the full `xcodebuild` log to `tmp/ios_bridge_tests.log`
4. Print a pass/fail summary and exit with code 0 (pass) or 1 (fail)

To run directly with `xcodebuild` (from `bingle_jsi/example/ios`):

```bash
xcodebuild test \
  -workspace BingleJsiExample.xcworkspace \
  -scheme BingleJsiBridgeTests \
  -destination 'platform=iOS Simulator,name=iPhone 16,OS=18.6' \
  -sdk iphonesimulator \
  -quiet
```

The simulator runs headlessly — no display or UI interaction is required,
so the tests are suitable for CI.

#### Project setup scripts

The Xcode test target was added to `BingleJsiExample.xcodeproj` using two
Ruby scripts (invoked once; do not need to be re-run unless the project
file is regenerated):

| Script | Purpose |
|--------|---------|
| `bingle_jsi/scripts/add_swift_test_target.rb` | Adds the `BingleJsiBridgeTests` target and registers source files |
| `bingle_jsi/scripts/fix_test_target_settings.rb` | Configures `PRODUCT_NAME`, deployment target, and standalone bundle settings (`TEST_HOST = ''`) |

Both scripts use the `xcodeproj` gem bundled with CocoaPods and are
invoked with:

```bash
GEM_PATH=/opt/homebrew/Cellar/cocoapods/1.16.2_1/libexec \
GEM_HOME=/opt/homebrew/Cellar/cocoapods/1.16.2_1/libexec \
ruby -I /opt/homebrew/Cellar/cocoapods/1.16.2_1/libexec/gems/xcodeproj-1.27.0/lib \
  bingle_jsi/scripts/<script>.rb
```

---

### End-to-end tests (Detox — Layer 3)

Layer 3 drives the **real TypeScript → native → Rust path** in an iOS simulator
via [Detox](https://wix.github.io/Detox/), through the example app's
command-dispatcher harness (issue #109/#116). Unlike the Layer 2 Swift tests
(Swift bridge → Rust), these exercise the shipped `ts/` interface exactly as a
React Native app calls it.

One-time prerequisite (a newer Homebrew needs the third-party tap trusted):

```bash
brew tap wix/brew && brew trust wix/brew && brew install applesimutils
```

Then run the whole suite from a clean checkout with a single command:

```bash
bash bingle_jsi/example/scripts/run_e2e_ios.sh
```

That builds the simulator xcframework, installs JS + pods, builds the app with
Detox, opens Metro in its own Terminal window (like `react-native run-ios`, left
running), and runs the e2e. To iterate on the tests without rebuilding (app
already built, faster):

```bash
bash bingle_jsi/example/scripts/run_e2e_ios.sh --test-only
```

The smoke suite (`bingle_jsi/example/e2e/smoke.test.ts`) launches the app, calls
`version()` before init, then inits (enabling the `bingle_local` API via a
state-file path) and round-trips a generated keypair. Add coverage by dropping a
new `e2e/*.test.ts` that uses the `call({ method, args })` helper in
`e2e/harness.ts` — no per-method UI. Notes and gotchas are documented in
`bingle_jsi/example/README.md` and inline in those files.

#### Network e2e — send + echo (issue #111)

`e2e/messaging.test.ts` drives the full send/receive path against a real network:
it sends a message to a live echo peer and asserts both delivery and the echoed
reply. It **skips cleanly** unless a backend and a funded, already-registered
sender are provided via the environment (so the default run stays offline):

| Env var | Meaning |
| --- | --- |
| `BINGLE_E2E_BACKEND` | `testnet` (default) or `localnet` (self-provisioning, #123) |
| `BINGLE_E2E_PASSPHRASE` | mnemonic of a funded, registered sender account (testnet) |
| `BINGLE_E2E_HANDLE` | that account's registered handle (testnet) |
| `BINGLE_E2E_ECHO_TO` | echo peer handle; defaults to `echo-testnet-1` on testnet |
| `BINGLE_E2E_OFFLINE_HANDLE` | optional; a handle registered but offline, for the `RecipientNotAdvertised` failure-cause case (#112) |

`run_e2e_ios.sh` stages the node-file (`nodely_staging_testnet_node.json` for
testnet) and `stunservers.txt` to `/tmp` and passes the env through. On testnet
`BINGLE_E2E_ECHO_TO` defaults to the always-live `echo-testnet-1`, so you only
supply the funded sender:

```bash
BINGLE_E2E_PASSPHRASE="word word … word" \
BINGLE_E2E_HANDLE=my-testnet-handle \
  bash bingle_jsi/example/scripts/run_e2e_ios.sh --test-only
```

`e2e/messaging.test.ts` (#111) covers send + echo; `e2e/failure_causes.test.ts`
(#112, validates #99) covers typed failure causes — an unregistered handle →
`HandleNotFound` (permanent), and, when `BINGLE_E2E_OFFLINE_HANDLE` is set, a
registered-but-offline recipient → `RecipientNotAdvertised` (retryable) — read
via `getMessages().failure_kind` and the `failureKindIsRetryable` helper.

##### localnet backend (issue #123)

For CI and offline runs, `BINGLE_E2E_BACKEND=localnet` needs no credentials: the
script ensures `algokit localnet` is running, builds and launches the Rust
provisioner (`bingle_test`'s `localnet_e2e_provisioner`), and waits for it to
come up. The provisioner deploys the Bingle app + asset, stands up two root
relays and STUN servers, registers and starts an in-process echo peer
(`echo-localnet-1`, the counterpart of testnet's `echo-testnet-1`), and registers
a funded sender plus a registered-but-offline fixture. It writes the node-file,
STUN list and a `BINGLE_E2E_*` env file (`/tmp/bingle_e2e_localnet.env`, its
readiness signal) that the script sources, so all credentials are derived
automatically. The provisioner is killed on exit. Requires `algokit` +
`algokit localnet` and the `goal` CLI on `PATH`.

On localnet the provisioner registers the offline fixture and exports
`BINGLE_E2E_OFFLINE_HANDLE` automatically, so the failure suite's
`RecipientNotAdvertised` case runs too.

```bash
BINGLE_E2E_BACKEND=localnet \
  bash bingle_jsi/example/scripts/run_e2e_ios.sh
```

---

### API Reference

The full API is defined in `src/api/bingle_jsi_api.rs`. Key methods:

| Method | Description |
|--------|-------------|
| `createBingleApi(config)` | Initialize the API with a `BingleJsiConfig` |
| `handleLookup(handle)` | Look up a user ID by handle |
| `sendMessageToId(userId, message)` | Send a message to a user ID |
| `sendMessageToHandle(handle, message)` | Send a message to a handle |
| `sendMessageToNetwork(nsk, userId, message)` | Send via network endpoint |
| `sendMessageTo*WithResponse(...)` | Send and wait for response |
| `queued()` | Get all queued received messages |
| `version()` | Get version info |
| `getNatType()` | Get detected NAT type |
| `generateKeypair()` | Generate a new Algorand keypair |
| `registerKeypair(handle)` | Register keypair on-chain |
| `addContact(handle, id, source)` | Add a contact |
| `blockContact(id)` | Block a contact |
| `removeContact(id)` | Remove a contact |
| `isBlocked(id)` | Check if contact is blocked |
| `getContacts()` | List unblocked contacts |
| `addMessage(sender, recipients, ts, text)` | Store a message locally |
| `getMessages()` | List stored messages |
| `keypairStatus()` | Check keypair funding status |
| `save(path)` / `load(path)` | Persist/restore local state |
| `setMessageCallback(callback)` | Register incoming message callback |
