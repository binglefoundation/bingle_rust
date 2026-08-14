/**
 * Detox configuration for the bingle_jsi example harness (issue #109).
 *
 * iOS needs no native project changes — Detox is configured entirely here; just run
 * `cd ios && pod install` before the first build. The app is driven through the command-dispatcher
 * screen in App.tsx (see e2e/*.test.js), so this same config exercises the whole BingleJsi surface.
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
  },
  devices: {
    simulator: {
      type: 'ios.simulator',
      device: {
        // Adjust to any installed simulator (`xcrun simctl list devicetypes`).
        type: 'iPhone 16',
      },
    },
  },
  configurations: {
    'ios.sim.debug': {
      device: 'simulator',
      app: 'ios.debug',
    },
  },
};
