const {getDefaultConfig, mergeConfig} = require('@react-native/metro-config');
const path = require('path');

// The parent bingle_jsi module directory
const bingleJsiRoot = path.resolve(__dirname, '..');

const config = {
  // Watch both the example app and the parent module for changes
  watchFolders: [bingleJsiRoot],

  resolver: {
    // Allow Metro to resolve the parent module's ts/ source
    nodeModulesPaths: [
      path.resolve(__dirname, 'node_modules'),
      path.resolve(bingleJsiRoot, 'node_modules'),
    ],
  },
};

module.exports = mergeConfig(getDefaultConfig(__dirname), config);
