/**
 * BingleJsi Example App — Detox command-dispatcher harness (issue #109).
 *
 * Instead of one screen per API method (churn) or a JavaScript `eval` console (Hermes restricts
 * `eval`, and it is unreliable on iOS), this harness exposes a single command box. You type a JSON
 * command `{ "method": "...", "args": [...] }`; a dispatch table calls the matching method on the
 * `BingleJsi` proxy (plus a couple of extras), awaits it, and renders the JSON result. Native
 * callbacks (onLog / onMessage / onListening / onMailboxChecked / onMailboxProgress) are appended
 * to an event feed.
 * Every element a test drives or reads carries a stable `testID` so Detox can operate the whole API
 * surface without any per-method UI.
 */
import React, {useCallback, useEffect, useState, useSyncExternalStore} from 'react';
import {
  AppState,
  SafeAreaView,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  TouchableOpacity,
  View,
  NativeEventEmitter,
  NativeModules,
} from 'react-native';

import {
  initBingleJsi,
  BingleJsi,
  bingleErrorInfo,
  failureKindIsRetryable,
} from 'react-native-bingle-jsi';
import type {BingleJsiConfig, FailureKind} from 'react-native-bingle-jsi';

import {
  getHarnessState,
  pushEvent,
  subscribeHarnessState,
  updateHarnessState,
} from './harnessStore';

/** Default config used when `init` is invoked without an explicit argument. */
const defaultConfig: BingleJsiConfig = {
  handle: 'example-user',
  passphrase: null,
  relay: false,
  static_ip: null,
  stun_servers: null,
  stun_servers_file: null,
  node_file: null,
  // 'warn' (not 'info'): at 'info' the engine floods onLog events once started, which continuously
  // re-renders the event feed. On Android that keeps the view hierarchy laying out so Espresso never
  // sees a stable/focused frame ("not request layout for 10 seconds"). e2e assertions rely on
  // onListening/onMessage feed lines, not logs, so 'warn' is sufficient (issue #131).
  log_level: 'warn',
  app_id: null,
  asset_id: null,
  handle_cache_expiry_secs: null,
  debug: true,
  // Enable the bingle_local API (keypair/contacts/messages) with a state-file path. Local methods
  // — generateKeypair, keypairStatus, getMessages (and the #99 failure_kind), etc. — require this;
  // with `local: null` they fail because the local API is never created. On the iOS simulator the
  // app can write to the host's /tmp; a real device/app supplies a path in its sandbox.
  local: '/tmp/bingle_jsi_harness_state.json',
  // Push give-up nudge is dormant in the harness: no gateway configured, feature left at default.
  notify_gateway_url: null,
  notify_on_giveup: null,
  notify_env: null,
};

/**
 * Extra dispatch entries that are not plain methods on the `BingleJsi` proxy: `init` (a top-level
 * export that must run before other methods) and the pure `failureKindIsRetryable` helper. Any other
 * method name falls through to `BingleJsi[method]` in {@link runCommand}.
 */
const extraDispatch: Record<string, (...args: any[]) => unknown> = {
  // Merge the passed (partial) config over the defaults, so a test can override just the fields it
  // needs — e.g. `{ handle, passphrase, node_file, stun_servers_file }` to point at a real network
  // with a funded account (issue #111) — while keeping the rest of the harness defaults.
  init: (config?: Partial<BingleJsiConfig>) =>
    initBingleJsi({...defaultConfig, ...(config ?? {})}),
  failureKindIsRetryable: (kind: FailureKind) => failureKindIsRetryable(kind),
  // Resolve with `ms` after `ms` milliseconds: a command that is reliably still in flight, for the
  // test that recreates the activity mid-command (issue #431).
  delay: (ms: number) =>
    new Promise<number>(resolve => setTimeout(() => resolve(ms), ms)),
  // Run a BingleJsi method expected to fail, and resolve with its rejection's message and
  // structured info, for the error-reporting tests (bingle_rust #464, #470).
  failureOf: async (method: string, args: unknown[] = []) => {
    const fn = (BingleJsi as Record<string, any>)[method];
    if (typeof fn !== 'function') {
      throw new Error(`unknown method: ${method}`);
    }
    try {
      await fn.apply(BingleJsi, args);
    } catch (e: unknown) {
      return {
        message: e instanceof Error ? e.message : String(e),
        info: bingleErrorInfo(e),
      };
    }
    throw new Error(`expected ${method} to fail`);
  },
};

/**
 * Run a dispatcher command and record its status and result in the harness store, not in a
 * component: if Android recreates the activity while the command runs, the remounted screen still
 * shows the result (issue #431).
 */
async function runHarnessCommand(command: string): Promise<void> {
  updateHarnessState({status: 'running', output: ''});
  try {
    const parsed = JSON.parse(command) as {method?: string; args?: unknown[]};
    const method = parsed.method;
    const args = parsed.args ?? [];
    if (!method) {
      throw new Error('command must include a "method"');
    }
    const fn =
      extraDispatch[method] ?? (BingleJsi as Record<string, any>)[method];
    if (typeof fn !== 'function') {
      throw new Error(`unknown method: ${method}`);
    }
    const result = await fn.apply(BingleJsi, args as unknown[]);
    // `undefined` (void methods) render as `null` so the output element is always valid JSON.
    updateHarnessState({output: JSON.stringify(result ?? null), status: 'ok'});
  } catch (e: unknown) {
    updateHarnessState({
      output: e instanceof Error ? e.message : String(e),
      status: 'error',
    });
  }
}

/**
 * Register the native event listeners and the app lifecycle forwarding once per JavaScript runtime,
 * not per mount, so a callback that fires while Android recreates the activity is still recorded
 * (issue #431). We deliberately do NOT auto-init: the e2e drives the lifecycle via the command box
 * so each test controls it.
 */
let nativeListenersRegistered = false;
function registerNativeListeners(): void {
  if (nativeListenersRegistered) {
    return;
  }
  nativeListenersRegistered = true;
  const emitter = new NativeEventEmitter(NativeModules.BingleJsi);
  emitter.addListener(
    'onLog',
    (e: {timestamp: number; level: string; message: string}) => {
      pushEvent(`onLog ${e.level} ${e.message}`);
    },
  );
  emitter.addListener(
    'onMessage',
    (e: {senderId: string; senderHandle: string; message: unknown}) => {
      pushEvent(`onMessage ${e.senderHandle} ${JSON.stringify(e.message)}`);
    },
  );
  emitter.addListener(
    'onListening',
    (e: {listening: boolean; nat_type: string; nat_type_v6?: string}) => {
      pushEvent(`onListening ${e.listening} ${e.nat_type} v6=${e.nat_type_v6}`);
    },
  );
  emitter.addListener('onMailboxChecked', (e: {read: number}) => {
    pushEvent(`onMailboxChecked ${e.read}`);
  });
  emitter.addListener(
    'onMailboxProgress',
    (e: {stage: string; read: number; retry_in_secs?: number}) => {
      const retry = e.retry_in_secs === undefined ? '' : ` ${e.retry_in_secs}`;
      pushEvent(`onMailboxProgress ${e.stage} ${e.read}${retry}`);
    },
  );

  // Forward app lifecycle to the engine as a host app does (bingle_ux_rn App.tsx), so a test that
  // backgrounds the app (`device.sendToHome()`) drives the real native backgrounding path (#352).
  // The bridge resolves without doing anything before `init`, so no gate is needed.
  let previous = AppState.currentState;
  AppState.addEventListener('change', next => {
    const was = previous;
    previous = next;
    if (next === 'active' && was !== 'active') {
      pushEvent('lifecycle foregrounding');
      BingleJsi.foregrounding().catch((err: unknown) =>
        pushEvent(`foregrounding error: ${String(err)}`),
      );
    } else if (next === 'background') {
      pushEvent('lifecycle backgrounding');
      BingleJsi.backgrounding().catch((err: unknown) =>
        pushEvent(`backgrounding error: ${String(err)}`),
      );
    }
  });
}

function App(): React.JSX.Element {
  const [ready, setReady] = useState(false);
  const {command, status, output, events} = useSyncExternalStore(
    subscribeHarnessState,
    getHarnessState,
  );

  useEffect(() => {
    registerNativeListeners();
    // Route native logs into the event feed (safe before init). It runs again after a remount, which
    // only resets the log level.
    BingleJsi.setLogCallback(defaultConfig.log_level)
      .catch((err: unknown) =>
        pushEvent(`setLogCallback error: ${String(err)}`),
      )
      .finally(() => setReady(true));
  }, []);

  const setCommand = useCallback(
    (text: string) => updateHarnessState({command: text}),
    [],
  );
  const runCommand = useCallback(() => {
    runHarnessCommand(getHarnessState().command);
  }, []);

  return (
    <SafeAreaView style={styles.container}>
      <View style={styles.content}>
        <Text style={styles.title}>Bingle JSI Harness</Text>
        <Text testID="app-ready" style={styles.ready}>
          {ready ? 'ready' : 'starting'}
        </Text>

        <Text style={styles.label}>Command (JSON: {'{'}method, args{'}'})</Text>
        <TextInput
          testID="cmd-input"
          style={styles.input}
          value={command}
          onChangeText={setCommand}
          autoCapitalize="none"
          autoCorrect={false}
          spellCheck={false}
          keyboardType="ascii-capable"
        />
        <TouchableOpacity
          testID="cmd-run"
          style={styles.button}
          onPress={runCommand}>
          <Text style={styles.buttonText}>Run</Text>
        </TouchableOpacity>

        <Text style={styles.label}>Status</Text>
        <Text testID="cmd-status" style={styles.value}>
          {status}
        </Text>

        <Text style={styles.label}>Output</Text>
        <ScrollView style={styles.outputBox}>
          <Text testID="cmd-output" style={styles.mono}>
            {output}
          </Text>
        </ScrollView>

        <Text style={styles.label}>Events</Text>
        <ScrollView style={styles.eventBox}>
          <Text testID="event-feed" style={styles.mono}>
            {events.join('\n')}
          </Text>
        </ScrollView>
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {flex: 1, backgroundColor: '#f5f5f5'},
  content: {flex: 1, padding: 16},
  title: {fontSize: 22, fontWeight: 'bold', color: '#333'},
  ready: {fontSize: 14, color: '#0a7', marginBottom: 12},
  label: {fontSize: 13, color: '#666', marginTop: 12, marginBottom: 4},
  input: {
    borderWidth: 1,
    borderColor: '#ccc',
    borderRadius: 6,
    backgroundColor: '#fff',
    padding: 8,
    minHeight: 60,
    fontFamily: 'Courier',
    fontSize: 13,
  },
  button: {
    marginTop: 10,
    backgroundColor: '#2563eb',
    borderRadius: 6,
    paddingVertical: 10,
    alignItems: 'center',
  },
  buttonText: {color: '#fff', fontWeight: '600', fontSize: 15},
  value: {fontSize: 15, fontWeight: '600', color: '#333'},
  outputBox: {
    maxHeight: 120,
    borderWidth: 1,
    borderColor: '#eee',
    borderRadius: 6,
    backgroundColor: '#fff',
    padding: 8,
  },
  eventBox: {
    flex: 1,
    borderWidth: 1,
    borderColor: '#eee',
    borderRadius: 6,
    backgroundColor: '#fff',
    padding: 8,
  },
  mono: {fontFamily: 'Courier', fontSize: 12, color: '#333'},
});

export default App;
