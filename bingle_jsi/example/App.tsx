/**
 * BingleJsi Example App — Detox command-dispatcher harness (issue #109).
 *
 * Instead of one screen per API method (churn) or a JavaScript `eval` console (Hermes restricts
 * `eval`, and it is unreliable on iOS), this harness exposes a single command box. You type a JSON
 * command `{ "method": "...", "args": [...] }`; a dispatch table calls the matching method on the
 * `BingleJsi` proxy (plus a couple of extras), awaits it, and renders the JSON result. Native
 * callbacks (onLog / onMessage / onListening) are appended to an event feed. Every element a test
 * drives or reads carries a stable `testID` so Detox can operate the whole API surface without any
 * per-method UI.
 */
import React, {useCallback, useEffect, useRef, useState} from 'react';
import {
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
  failureKindIsRetryable,
} from 'react-native-bingle-jsi';
import type {BingleJsiConfig, FailureKind} from 'react-native-bingle-jsi';

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
};

type CommandStatus = 'idle' | 'running' | 'ok' | 'error';

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
};

/** How many event-feed lines to keep in view. */
const MAX_EVENT_LINES = 300;

function App(): React.JSX.Element {
  const [ready, setReady] = useState(false);
  const [command, setCommand] = useState('{"method":"version","args":[]}');
  const [status, setStatus] = useState<CommandStatus>('idle');
  const [output, setOutput] = useState('');
  const [events, setEvents] = useState<string[]>([]);
  const eventsRef = useRef<string[]>([]);

  const pushEvent = useCallback((line: string) => {
    const next = [...eventsRef.current, line].slice(-MAX_EVENT_LINES);
    eventsRef.current = next;
    setEvents(next);
  }, []);

  // Register native event listeners once, before any command runs, so callbacks emitted during init
  // or delivery are captured. We deliberately do NOT auto-init: the e2e drives the lifecycle via the
  // command box so each test controls it.
  useEffect(() => {
    const emitter = new NativeEventEmitter(NativeModules.BingleJsi);
    const subs = [
      emitter.addListener(
        'onLog',
        (e: {timestamp: number; level: string; message: string}) => {
          pushEvent(`onLog ${e.level} ${e.message}`);
        },
      ),
      emitter.addListener(
        'onMessage',
        (e: {senderId: string; senderHandle: string; message: unknown}) => {
          pushEvent(`onMessage ${e.senderHandle} ${JSON.stringify(e.message)}`);
        },
      ),
      emitter.addListener(
        'onListening',
        (e: {listening: boolean; nat_type: string}) => {
          pushEvent(`onListening ${e.listening} ${e.nat_type}`);
        },
      ),
    ];
    // Route native logs into the event feed from the start (safe before init).
    BingleJsi.setLogCallback(defaultConfig.log_level)
      .catch((err: unknown) =>
        pushEvent(`setLogCallback error: ${String(err)}`),
      )
      .finally(() => setReady(true));
    return () => subs.forEach(s => s.remove());
  }, [pushEvent]);

  const runCommand = useCallback(async () => {
    setStatus('running');
    setOutput('');
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
      setOutput(JSON.stringify(result ?? null));
      setStatus('ok');
    } catch (e: unknown) {
      setOutput(e instanceof Error ? e.message : String(e));
      setStatus('error');
    }
  }, [command]);

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
