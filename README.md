# rust_comms

This repository contains a Rust library with Algorand helpers and an iOS-compatible FFI surface. It includes a ready-to-use Swift Package and test target so you can build and run iOS unit tests entirely from the command line.

Below are the steps to build the iOS XCFramework and run the iOS tests on a Simulator via the command line.

## Prerequisites
- macOS with Xcode installed
- Xcode Command Line Tools: `xcode-select --install`
- Rust toolchain with iOS targets:
  - `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`
  - Optionally (Intel Simulator): `rustup target add x86_64-apple-ios`

The repository already includes:
- A C header and module map under `include/` used by the XCFramework
- A build script at `scripts/build_ios_xcframework.sh`
- A Swift Package scaffold in `ios/` with a test target.

## Repository layout and legacy sources
- Active library root: `src/lib.rs` re-exports modules from `src/blockchain/` (e.g., `rust_comms::algo_ops`).
- Tests and FFI test runners live in the separate crate `bingle_test/` (see `bingle_test/src`). Rust integration tests import `bingle_test::tests_common`.
- Legacy, unreferenced copies exist under `src/` (e.g., `src/tests_common/`, `src/ffi_tests.rs`, `src/lib_mod.rs`). These are intentionally left in place for reference during the transition. Because `src/lib.rs` does not `mod` or `pub use` them, Cargo does not compile them. You can safely ignore them; they do not affect builds. If/when you remove them, ensure all references point to `bingle_test` (for tests) and `src/blockchain` (for core code).

## 1) Build the iOS XCFramework
From the repository root, run:

```
bash scripts/build_ios_xcframework.sh
```

This will:
- Build the Rust static library for device and simulator architectures
- Package them into `ios/RustCommsFFI.xcframework`
- Ensure `ios/Package.swift` and a simple test file exist under `ios/Tests/RustCommsFFITests/`

## 2) Choose a Simulator
List available iOS Simulators:

```
xcrun simctl list devices 'iOS'
```

Pick a device name (e.g. "iPhone 15"). Optionally note the OS version (e.g. 17.5).

## 3) Run iOS Tests from the Command Line
There are two equivalent ways to run tests using `xcodebuild`:

### Run from within the `ios/` directory
```
cd ios
# Clean + test on a specific simulator
xcodebuild \
  -scheme RustCommsFFI \
  -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  clean test
```

If you want to pin the OS version (adjust device/OS to what you have installed):
```
cd ios
xcodebuild \
  -scheme RustCommsFFI \
  -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPhone 16,OS=18.6' \
  clean test
```

Notes:
- The scheme name is the package name, e.g. `RustCommsFFI`. To list available schemes from the ios folder:
  - `cd ios && xcodebuild -list`
- The test target is `RustCommsFFITests` and is included in the package scheme.

## 4) What the Tests Do
The included test `RustCommsFFITests` verifies the Swift package can import the RustCommsFFI module and runs a trivial assertion. This confirms tests can run under the iOS target and that the XCFramework is discoverable.

## Environment Variables

- `BINGLE_ALGO_DEBUG`: Set to `true` or `1` to enable verbose logging for Algorand blockchain operations (algod/indexer requests, transaction preflights, etc.). These logs are disabled by default.
- `RUST_COMMS_RUN_LOCALNET`: Set to `true` to enable integration tests that require a running Algorand localnet.
- `MEASURE_MEMORY`: Set to `1` to enable peak memory usage reporting in Docker containers.

## 5) Running Localnet Integration Tests
Integration tests that require a running Algorand localnet (via `algokit`) are marked as `#[ignore]` and skipped by default in standard `cargo test` runs.

To run these tests:
1. Ensure `algokit` is installed.
2. Run the provided script from the repository root:
   ```bash
   bash scripts/run_localnet_tests.sh
   ```
This script will:
- Check and start the Algorand localnet sandbox if needed.
- Set `RUST_COMMS_RUN_LOCALNET=true` to enable localnet-dependent logic.
- Execute all integration tests containing "localnet" in their name, including those marked as ignored.

## 6) Running All Rust Tests
To run all tests in the project (including unit tests, workspace members, and all integration tests in the `tests/` folder), use the provided helper script:

```bash
bash scripts/run_all_tests.sh
```

This script runs:
1. `cargo test --workspace`: Executes all standard tests.
2. `cargo test --workspace -- --ignored`: Executes tests that are skipped by default (e.g., slow or environment-dependent integration tests).

Alternatively, you can run `cargo test --workspace` directly for standard tests.

### Running from RustRover (JetBrains)
Pre-configured Run Configurations are provided in the `.run` directory. These will automatically appear in your RustRover / IntelliJ UI:

- **All Tests (Standard)**: Runs standard tests with full reporting.
- **All Tests (Ignored)**: Runs only the ignored tests.
- **All Tests (Complete Suite)**: Runs standard tests after the ignored ones have finished (sequential).

To run these with nice UI reporting:
1. Select the desired configuration from the dropdown at the top right.
2. Click the green **Run** button.
3. Results will appear in the **Run** or **Services** tool window with a hierarchical tree view of all passed and failed tests.

## 7) Docker Support

This project provides separate Dockerfiles for the application and integration tests:
- `Dockerfile`: Contains `base`, `cli`, and `webserver` stages.
- `Dockerfile.tests`: Contains the `tests` stage for running integration tests in an environment that matches the runtime.

All Docker builds are "runtime-only", meaning they expect the Rust binaries to be prebuilt on the host (e.g., using `cargo zigbuild` for cross-compilation to Linux musl).

### Build the Application Image
To build the CLI or Webserver images, use the provided scripts. By default, they target `aarch64-unknown-linux-musl` and use `cargo-zigbuild`.

```bash
# Build the CLI image (bingle:local)
bash scripts/build_cli_image.sh

# Build the Webserver image (bingle-webserver:local)
bash scripts/build_webserver_image.sh
```

### Build the Test Image
To build the integration test image, use:

```bash
# Build the test image (bingle-tests:local)
bash scripts/build_tests_image.sh --test <test_name>
```

Example: `bash scripts/build_tests_image.sh --test endpoint_available`

### Running Testnet Integration Tests
For a full end-to-end test run involving multiple Docker containers (relays, STUN servers, and the test runner), use:

```bash
bash scripts/run_testnet_tests.sh
```

### Measuring Peak Memory Usage
You can measure the peak memory usage of any container over its lifetime using two methods:

#### 1) Container-Side (Automatic)
The `cli`, `webserver`, and `tests` Docker containers can report their own peak memory usage just before they exit. To enable this, set the `MEASURE_MEMORY=1` environment variable when running the container:

```bash
docker run --rm -e MEASURE_MEMORY=1 bingle:local
```

For the test suite scripts (`scripts/run_testnet_tests.sh` and `scripts/run_webserver_testnet.sh`), setting `MEASURE_MEMORY=1` will propagate this setting to all started containers (relays, pingable, webserver, and test runners) and consolidate their peak memory reports into a single summary at the end of the run.

#### 2) Host-Side Monitoring
A standalone script is provided to monitor any running container from the host by polling `docker stats`. This is useful for long-running containers or those you don't want to modify:

```bash
# Monitor a container named 'bingle_webserver' with 0.5s polling interval
bash scripts/measure_memory.sh bingle_webserver 0.5
```

The script will track and display the highest memory usage observed until the container stops or you press Ctrl+C.

### 8) AWS Deployment

You can deploy the Bingle Relay server to AWS using the provided CloudFormation templates and deployment script.

#### Prerequisites
- AWS CLI configured with appropriate permissions.
- Docker installed and running.

#### Deploying a Relay
To deploy a relay server to AWS ECS (using an EC2 cluster with Graviton `t4g.nano` by default):

```bash
bash aws/deploy_relay.sh --handle "my-relay" --passphrase "my-secret-passphrase"
```

#### Express Mode (Faster Deployment)
If you want a faster and simpler deployment, you can use the `--express` flag. This uses **AWS Fargate** instead of EC2, which skips the time-consuming process of provisioning EC2 instances and waiting for them to join the cluster.

```bash
bash aws/deploy_relay.sh --handle "my-relay" --passphrase "my-secret-passphrase" --express
```

**Why use Express Mode?**
- **Speed**: Deploys in minutes rather than 10-15 minutes.
- **Simplicity**: Fewer AWS resources to manage (no ASGs, Launch Templates, or Instance Profiles).
- **Cost**: For low-traffic relays, Fargate is often more cost-effective as you only pay for the exact CPU/Memory used by the container.

Options:
- `--express`: Use Fargate-based deployment (recommended for rapid setup).
- `--instance-type`: EC2 instance type (default: `t4g.nano`, only used if NOT in express mode).
- `--port`: UDP port to expose (default: `12121`).
- `--stack-name`: CloudFormation stack name.
- `--region`: AWS region.

The script will:
1. Build the relay Docker image for the `linux/arm64` platform.
2. Create an ECR repository and push the image.
3. Deploy a CloudFormation stack containing a VPC, ECS Cluster, IAM roles, Secrets Manager secret, SSM parameters, and the ECS Service.
4. Create a CloudWatch Dashboard for monitoring CPU and Memory.

#### Redeploying after a change
If you have made changes to the code and want to redeploy the relay:

1. **Full redeploy** (re-runs CloudFormation):
   ```bash
   bash aws/deploy_relay.sh --handle "my-relay" --passphrase "my-secret-passphrase"
   ```
   By default, the script uses the build number from `.build_number` as the image tag, which ensures CloudFormation detects a change in the `ImageUri` and updates the ECS service.

2. **Quick redeploy** (updates image and forces ECS restart, skipping CloudFormation):
   ```bash
   bash aws/deploy_relay.sh --handle "my-relay" --passphrase "my-secret-passphrase" --redeploy-only
   ```
   This is faster if you only changed the code and don't need to update any AWS infrastructure.

#### Shutting Down and Deleting Resources
To shut down the relay and delete all AWS resources created by the CloudFormation stack:
```bash
bash aws/destroy_relay.sh --stack-name "bingle-relay"
```
Options:
- `--stack-name`: The name of the stack to delete (default: `bingle-relay`).
- `--region`: AWS region.
- `--delete-repo`: Also delete the ECR repository (useful for a complete cleanup).

#### Monitoring
Logs are available in CloudWatch Logs under the group `/bingle/relay`. A dashboard named `<stack-name>-Dashboard` is also created in CloudWatch.

## Troubleshooting
- Missing framework error: Ensure you ran `bash scripts/build_ios_xcframework.sh` and that `ios/RustCommsFFI.xcframework` exists.
- Simulator not found: Use `xcrun simctl list devices 'iOS'` to pick a valid device name and optionally boot it using `xcrun simctl boot "iPhone 15"`.
- Scheme not found: From the repo root run `cd ios && xcodebuild -list` to confirm the `RustCommsFFI` scheme is present.
- Codesigning: Running on Simulator does not require code signing. Running on a physical device would require additional signing setup and is outside the scope of this quickstart.

## Clean Rebuild
To clean the build outputs:
```
rm -rf ios/RustCommsFFI.xcframework target
bash scripts/build_ios_xcframework.sh
```

## Optional: Open in Xcode GUI
You can also open the package in Xcode to run tests interactively:
- Open Xcode -> File -> Open -> select `ios/Package.swift`.
- Choose a simulator and run the `RustCommsFFITests` test suite.



### Troubleshooting: E0463 “can't find crate for std” when targeting iOS
- Symptom: `cargo build --target aarch64-apple-ios` errors with E0463 even though `rustup target list --installed` shows the iOS targets.
- Likely cause: The iOS targets are installed for a different toolchain than the one your `cargo`/`rustc` is using, or your PATH `cargo` is not rustup-managed (e.g., Homebrew cargo).
- Fix:
  1) Install targets for the active toolchain explicitly:
     ```bash
     TC="$(rustup show active-toolchain | awk '{print $1}')"
     rustup target add --toolchain "$TC" aarch64-apple-ios aarch64-apple-ios-sim
     # Optional (if supported):
     rustup target add --toolchain "$TC" x86_64-apple-ios
     ```
  2) Ensure you are using rustup-managed cargo:
     ```bash
     echo "PATH cargo:   $(command -v cargo)"
     echo "rustup cargo: $(rustup which cargo)"
     # If different, run cargo via rustup for the active toolchain:
     rustup run "$TC" cargo build --release --target aarch64-apple-ios
     ```
  3) Or simply run the provided script (it enforces the correct toolchain and targets):
     ```bash
     bash scripts/build_ios_xcframework.sh
     ```


## `initBingleJsi` Call Stack

The exported TypeScript function `initBingleJsi` is defined in `bingle_jsi/ts/index.ts`.
It initialises the native Bingle API on both iOS and Android via React Native's bridge.
The call flows through the following layers on each platform:

### TypeScript (common)

```
initBingleJsi(config)                        — bingle_jsi/ts/index.ts
  └─ NativeModules.BingleJsi.initialize(config)
```

`NativeModules.BingleJsi` resolves to the platform native module registered
under the name **"BingleJsi"**.

### iOS

```
NativeModules.BingleJsi.initialize(config)
  │
  ├─ ObjC registration (RCT_EXTERN_METHOD)   — bingle_jsi/ios/BingleJsiBridge.m
  │    Exposes the Swift method to React Native's bridge via
  │    RCT_EXTERN_MODULE(BingleJsi, RCTEventEmitter).
  │
  ├─ Swift bridge                             — bingle_jsi/ios/BingleJsiBridge.swift
  │    BingleJsiBridge.initialize(_:resolver:rejecter:)
  │    Converts the NSDictionary config into a uniffi BingleJsiConfig struct
  │    and dispatches to a background queue.
  │
  ├─ uniffi-generated Swift binding
  │    createBingleApi(config: BingleJsiConfig)
  │    Auto-generated by uniffi from the #[uniffi::export] annotation.
  │
  └─ Rust                                    — bingle_jsi/src/lib.rs
       create_bingle_api(config)
         └─ BingleJsiApiImpl::init(config)   — bingle_jsi/src/api/bingle_jsi_api_impl.rs
```

### Android

```
NativeModules.BingleJsi.initialize(config)
  │
  ├─ Package registration                    — .../com/bingle/jsi/BingleJsiPackage.kt
  │    BingleJsiPackage.createNativeModules() returns BingleJsiModule,
  │    which registers itself under getName() = "BingleJsi".
  │
  ├─ Kotlin bridge                           — .../com/bingle/jsi/BingleJsiModule.kt
  │    BingleJsiModule.initialize(config, promise)
  │    Annotated with @ReactMethod. Converts the ReadableMap config into
  │    a uniffi BingleJsiConfig data class on a background thread.
  │
  ├─ uniffi-generated Kotlin binding
  │    createBingleApi(jsiConfig)
  │    Auto-generated by uniffi from the #[uniffi::export] annotation.
  │
  └─ Rust                                    — bingle_jsi/src/lib.rs
       create_bingle_api(config)
         └─ BingleJsiApiImpl::init(config)   — bingle_jsi/src/api/bingle_jsi_api_impl.rs
```

### Key source files

| Layer | iOS | Android |
|---|---|---|
| TypeScript entry | `bingle_jsi/ts/index.ts` | `bingle_jsi/ts/index.ts` |
| ObjC bridge macro | `bingle_jsi/ios/BingleJsiBridge.m` | — |
| Native module | `bingle_jsi/ios/BingleJsiBridge.swift` | `bingle_jsi/android/src/main/java/com/bingle/jsi/BingleJsiModule.kt` |
| Package registration | — | `bingle_jsi/android/src/main/java/com/bingle/jsi/BingleJsiPackage.kt` |
| uniffi scaffolding | auto-generated Swift | auto-generated Kotlin (`bingle_jsi/android/generated/uniffi/bingle_jsi/bingle_jsi.kt`) |
| Rust entry point | `bingle_jsi/src/lib.rs` (`create_bingle_api`) | `bingle_jsi/src/lib.rs` (`create_bingle_api`) |
| Rust implementation | `bingle_jsi/src/api/bingle_jsi_api_impl.rs` (`BingleJsiApiImpl::init`) | `bingle_jsi/src/api/bingle_jsi_api_impl.rs` (`BingleJsiApiImpl::init`) |

## Changelog

2025-08-29
- DTLS (non‑iOS, OpenSSL) server: Implemented DTLSv1.2 accept loop with per‑peer SSL streams and callbacks.
  - Handlers now receive a reference to the Dtls instance (&dyn Dtls) so they can reply via server.send(to, data).
  - Added DTLS cookie callbacks and read_ahead for better DTLS behavior.
- DTLS client: Handshake over UDP via OpenSSL, send application data, and deliver responses to the configured with_handle_message callback. The client handler receives a lightweight handle that can write back on the same DTLS connection.
- Echo semantics in tests: Server echo handler prepends "ECHOED: " to payloads; client echo handler prepends "CLIENT ECHOED: ".
- Dynamic PKI for tests: Integration tests now generate an Ed25519 CA and ECDSA P‑256 server/client certs with the openssl CLI (tests/pki.rs). No static PEMs are required.
- New integration tests:
  - tests/dtls_openssl_smoke.rs: Starts a DTLS server and verifies the handler observes application data from a DTLS client.
  - tests/dtls_loopback_e2e.rs: End‑to‑end loopback where the server echoes back with the required prefix; client validates echoed data and server records original payload.
  - tests/dtls_client_echo_roundtrip.rs: Two‑way messaging where server triggers a Ping and client echoes back with the client prefix; server validates "CLIENT ECHOED: Ping".
- Cleanup:
  - Removed plaintext UDP fallback paths and unused scaffolding types; warnings reduced.
  - Centralized client context preparation; tests pass locally.

2026-03-16
- Added `scripts/run_all_tests.sh` to provide a single command for running all tests in the workspace and `tests/` folder.
- Fixed several orphaned test files in the `tests/` tree that were not being executed by standard `cargo test`.
- Consolidated all integration tests into the `all` test target.

Notes
- For simplicity in tests, peer verification is currently disabled (SslVerifyMode::NONE). The HandlePeerCertificate callback API exists and can be re‑enabled for strict verification when needed.
