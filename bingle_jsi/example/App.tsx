/**
 * BingleJsi Example App
 *
 * Minimal React Native application that initializes the Bingle JSI module
 * and calls `version()`, logging the result to the console.
 */
import React, {useEffect, useState} from 'react';
import {
  SafeAreaView,
  StyleSheet,
  Text,
  View,
  ActivityIndicator,
  NativeEventEmitter,
  NativeModules,
} from 'react-native';

import {initBingleJsi, BingleJsi} from 'react-native-bingle-jsi';
import type {BingleJsiConfig} from 'react-native-bingle-jsi';

const defaultConfig: BingleJsiConfig = {
  handle: 'example-user',
  passphrase: null,
  relay: false,
  static_ip: null,
  stun_servers: null,
  stun_servers_file: null,
  node_file: null,
  log_level: 'info',
  app_id: null,
  asset_id: null,
  handle_cache_expiry_secs: null,
  debug: true,
  local: null,
};

function App(): React.JSX.Element {
  const [versionInfo, setVersionInfo] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function run() {
      try {
        // Set up log forwarding BEFORE init so that logs emitted during
        // initialization are captured in the JS console.
        const emitter = new NativeEventEmitter(NativeModules.BingleJsi);
        emitter.addListener('onLog', (event: {timestamp: number; level: string; message: string}) => {
          const ts = new Date(event.timestamp).toISOString();
          const line = `[Bingle/${event.level}] ${ts} ${event.message}`;
          switch (event.level) {
            case 'ERROR':
              console.error(line);
              break;
            case 'WARN':
              console.warn(line);
              break;
            case 'DEBUG':
            case 'TRACE':
              console.debug(line);
              break;
            default:
              console.log(line);
              break;
          }
        });
        await BingleJsi.setLogCallback(defaultConfig.log_level);
        console.log('[BingleJsiExample] Log callback registered (before init).');

        // version() can also be called before init
        console.log('[BingleJsiExample] Calling version() (before init)...');
        const version = await BingleJsi.version();
        const versionStr = `v${version.version} (build ${version.build_number}, ${version.build_timestamp})`;
        console.log(`[BingleJsiExample] Version: ${versionStr}`);
        setVersionInfo(versionStr);

        console.log('[BingleJsiExample] Initializing BingleJsi...');
        await initBingleJsi(defaultConfig);
        console.log('[BingleJsiExample] BingleJsi initialized successfully.');
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        console.error(`[BingleJsiExample] Error: ${msg}`);
        setError(msg);
      } finally {
        setLoading(false);
      }
    }
    run();
  }, []);

  return (
    <SafeAreaView style={styles.container}>
      <View style={styles.content}>
        <Text style={styles.title}>Bingle JSI Example</Text>

        {loading && (
          <View style={styles.row}>
            <ActivityIndicator size="small" />
            <Text style={styles.label}> Initializing...</Text>
          </View>
        )}

        {versionInfo && (
          <View style={styles.row}>
            <Text style={styles.label}>Version: </Text>
            <Text style={styles.value}>{versionInfo}</Text>
          </View>
        )}

        {error && (
          <View style={styles.row}>
            <Text style={styles.error}>Error: {error}</Text>
          </View>
        )}
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  content: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 20,
  },
  title: {
    fontSize: 24,
    fontWeight: 'bold',
    marginBottom: 30,
    color: '#333',
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    marginVertical: 8,
  },
  label: {
    fontSize: 16,
    color: '#666',
  },
  value: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
  },
  error: {
    fontSize: 16,
    color: '#cc0000',
  },
});

export default App;
