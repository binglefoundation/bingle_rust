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
      build:
        'xcodebuild -workspace ios/BingleJsiExample.xcworkspace -scheme BingleJsiExample -configuration Debug -sdk iphonesimulator -derivedDataPath ios/build',
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
