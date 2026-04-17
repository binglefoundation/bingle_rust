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
│       └── bingle_jsi_api_impl.rs# Concrete implementation + init()
├── scripts/
│   ├── build_ios.sh              # iOS cross-compile + XCFramework
│   └── build_android.sh          # Android cross-compile + jniLibs
├── ios/
│   ├── generated/                # (created by build_ios.sh) Swift + C headers
│   └── BingleJsi.xcframework/   # (created by build_ios.sh)
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
- **uniffi-bindgen** CLI matching the crate's uniffi version:
  ```bash
  cargo install uniffi-bindgen-cli --version 0.28.3
  ```

#### iOS

- **macOS** with **Xcode** (command-line tools installed)
- Rust iOS targets (installed automatically by the build script):
  - `aarch64-apple-ios` (device)
  - `aarch64-apple-ios-sim` (Apple Silicon simulator)
  - `x86_64-apple-ios` (Intel simulator, optional)

#### Android

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

#### Step 4: Android — Gradle Sync

The `android/build.gradle` in the module configures jniLibs and Kotlin
bindings. After adding the module dependency, sync your Gradle project
in Android Studio or run:

```bash
cd android
./gradlew sync
cd ..
```

#### Step 5: Use in TypeScript

```typescript
import {
  BingleJsiApi,
  BingleJsiConfig,
  BingleMessage,
  ContactSource,
  MessageCallback,
} from 'react-native-bingle-jsi';

// The native module is loaded by uniffi's generated bindings.
// Access varies by platform; typically via the generated Swift/Kotlin module.
// Example usage (assuming the native module is exposed as `BingleJsi`):

// Initialize
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

// Call init (from the generated native module)
// const api = BingleJsi.init(config);

// Get version info
// const version = api.version();
// console.log(`Bingle v${version.version}`);

// Send a message
// const msg: BingleMessage = {
//   app: null,
//   type: null,
//   tag: null,
//   response_tag: null,
//   text: 'Hello from React Native!',
//   data: null,
// };
// api.sendMessageToHandle('bob', msg);

// Register a message callback
// const callback: MessageCallback = {
//   onMessage(senderId, senderHandle, message) {
//     console.log(`Message from ${senderHandle}: ${message.text}`);
//   },
// };
// api.setMessageCallback(callback);

// Local API (when `local` is set in config)
// const keypair = api.generateKeypair();
// console.log(`Generated keypair: ${keypair.id}`);
// api.addContact('bob', 'BOB_ALGO_ADDRESS', ContactSource.Manual);
// const contacts = api.getContacts();
```

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

### API Reference

The full API is defined in `src/api/bingle_jsi_api.rs`. Key methods:

| Method | Description |
|--------|-------------|
| `init(config)` | Initialize the API with a `BingleJsiConfig` |
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
