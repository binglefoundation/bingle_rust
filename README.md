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

For the test suite, this is useful to see how much memory each test stage consumes.

#### 2) Host-Side Monitoring
A standalone script is provided to monitor any running container from the host by polling `docker stats`. This is useful for long-running containers or those you don't want to modify:

```bash
# Monitor a container named 'bingle_webserver' with 0.5s polling interval
bash scripts/measure_memory.sh bingle_webserver 0.5
```

The script will track and display the highest memory usage observed until the container stops or you press Ctrl+C.

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
