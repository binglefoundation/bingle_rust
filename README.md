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
