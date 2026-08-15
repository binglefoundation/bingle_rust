/**
 * Detox configuration for the bingle_jsi example harness (issue #109; Android added in #130).
 *
 * iOS needs no native project changes — Detox is configured entirely here; just run
 * `cd ios && pod install` before the first build. Android needs the Detox instrumentation wired into
 * `android/app` (androidTest deps + a DetoxTest runner + a test-runner override in build.gradle).
 * The app is driven through the command-dispatcher screen in App.tsx (see e2e/*.test.js), so the
 * same config exercises the whole BingleJsi surface on both platforms.
 */
/** @type {Detox.DetoxConfig} */
module.exports = {
  testRunner: {
    args: {
      $0: 'jest',
      config: 'e2e/jest.config.js',
    },
    jest: {
      setupTimeout: 120000,
    },
  },
  apps: {
    'ios.debug': {
      type: 'ios.app',
      binaryPath:
        'ios/build/Build/Products/Debug-iphonesimulator/BingleJsiExample.app',
      // ARCHS=arm64 restricts the build to the Apple-silicon simulator arch, matching the fast
      // sim-only xcframework (BINGLE_IOS_SIM_ONLY=1 build_ios.sh, which omits the x86_64 slice).
      // For an Intel host or CI, build the full xcframework (default build_ios.sh) and drop ARCHS.
      build:
        'xcodebuild -workspace ios/BingleJsiExample.xcworkspace -scheme BingleJsiExample -configuration Debug -sdk iphonesimulator -derivedDataPath ios/build ARCHS=arm64 ONLY_ACTIVE_ARCH=YES',
    },
    'android.debug': {
      type: 'android.apk',
      binaryPath: 'android/app/build/outputs/apk/debug/app-debug.apk',
      testBinaryPath:
        'android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk',
      // Build both the app and the androidTest (Detox instrumentation) APKs.
      build:
        'cd android && ./gradlew :app:assembleDebug :app:assembleAndroidTest -DtestBuildType=debug && cd ..',
      // 8081 = Metro. 4001/8980 = algod/indexer: the Android emulator's localhost is the emulator,
      // not the host, so adb-reverse the localnet ports the app connects to (localhost:4001/8980).
      reversePorts: [8081, 4001, 8980],
    },
  },
  devices: {
    simulator: {
      type: 'ios.simulator',
      device: {
        // Adjust to any installed simulator (`xcrun simctl list devicetypes`).
        type: 'iPhone 16',
      },
    },
    emulator: {
      type: 'android.emulator',
      device: {
        // Adjust to any installed AVD (`emulator -list-avds`). CI creates its own AVD (#132).
        avdName: 'Pixel_3a_API_34_extension_level_7_arm64-v8a',
      },
    },
  },
  configurations: {
    'ios.sim.debug': {
      device: 'simulator',
      app: 'ios.debug',
    },
    'android.emu.debug': {
      device: 'emulator',
      app: 'android.debug',
    },
  },
};
