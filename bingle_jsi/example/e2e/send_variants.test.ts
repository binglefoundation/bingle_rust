/// <reference types="detox" />
/**
 * Send-variant + transport-introspection e2e for the bingle_jsi Detox harness (issue #139, group B).
 *
 * Drives the transport surface the messaging suite (#111) does not: exact `handleLookup`, the direct
 * (fire-and-forget) `sendMessageToHandle` / `sendMessageToId`, plus `getNatType`, `networkAvailable`
 * (true once listening), `queued`, and `isStarted` across start/stop — all against the localnet echo
 * peer.
 *
 * Backend + credentials come from the environment (staged by run_e2e_ios.sh / run_e2e_android.sh),
 * same as the messaging suite; skips cleanly when unset.
 *
 * Not covered — three variants exist in the uniffi trait but are **not wired into the native JS
 * bridge** (Kotlin BingleJsiModule / Swift), so they are not reachable from the harness at all:
 * `sendMessageToIdWithResponse` / `sendMessageToHandleWithResponse` (synchronous request/response)
 * and `sendMessageToNetwork` (+ its WithResponse form). The network-source-key path would also need
 * a concrete `NetworkSourceKey` the harness cannot obtain. Deferred to an API-surface follow-up (the
 * bridge should expose these) rather than faked here. See the #139 note.
 *
 * Note: Detox replaces the global `expect`, so value assertions use Node's `assert` (see harness.ts).
 */
import {describe, it, beforeAll, afterAll} from '@jest/globals';
import assert from 'assert';
import {call, textOf, sleep, resolveNetworkInputs, localStatePath} from './harness';

const backend = process.env.BINGLE_E2E_BACKEND || 'testnet';
const passphrase = process.env.BINGLE_E2E_PASSPHRASE || '';
const handle = process.env.BINGLE_E2E_HANDLE || '';
const echoTo = process.env.BINGLE_E2E_ECHO_TO || '';
const nodeFile = process.env.BINGLE_E2E_NODE_FILE || '';
const stunFile = process.env.BINGLE_E2E_STUN_FILE || '';

const haveCreds = passphrase && handle && echoTo && nodeFile;
const describeOrSkip = haveCreds ? describe : describe.skip;

const LISTEN_TIMEOUT = 90000;
const ECHO_TIMEOUT = 120000;

/** The full BingleMessage record with only `text` set (nulls for the rest, to be explicit). */
function textMessage(text: string): Record<string, unknown> {
  return {
    app: null,
    type: null,
    tag: null,
    response_tag: null,
    text,
    data: null,
    cipher_suite: null,
  };
}

/** Poll the event feed until it contains `substring`. */
async function waitForFeed(substring: string, timeoutMs: number): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if ((await textOf('event-feed')).includes(substring)) {
      return;
    }
    await sleep(1000);
  }
  throw new Error(`event feed never contained "${substring}" within ${timeoutMs}ms`);
}

describeOrSkip(`bingle_jsi send variants (${backend})`, () => {
  beforeAll(async () => {
    await device.launchApp({newInstance: true});
    await device.disableSynchronization();
    await waitFor(element(by.id('app-ready')))
      .toHaveText('ready')
      .withTimeout(30000);

    const net = await resolveNetworkInputs(nodeFile, stunFile);
    await call({
      method: 'init',
      args: [
        {
          handle,
          passphrase,
          node_file: net.node_file,
          stun_servers: net.stun_servers,
          local: localStatePath(`bingle_e2e_send_variants_${Date.now()}.json`),
        },
      ],
    });
    await call({method: 'importKeypair', args: [passphrase]});
    await call({method: 'setMessageCallback', args: []});
    await call({method: 'setListeningCallback', args: []});
    await call({method: 'start', args: []});
    try {
      await waitForFeed('onListening true', LISTEN_TIMEOUT);
    } catch (e) {
      // eslint-disable-next-line no-console
      console.log('DIAG event-feed at listening timeout:\n' + (await textOf('event-feed')));
      throw e;
    }
  });

  afterAll(async () => {
    try {
      await call({method: 'stop', args: []});
    } catch (_e) {
      // ignore — the last test may already have stopped the engine.
    }
  });

  it('resolves a registered handle to an id via handleLookup', async () => {
    const id = await call({method: 'handleLookup', args: [echoTo]});
    assert.strictEqual(typeof id, 'string');
    assert.ok(id.length > 0, `handleLookup(${echoTo}) should return a non-empty id`);
  });

  it('reports networkAvailable true once listening', async () => {
    assert.strictEqual(await call({method: 'networkAvailable', args: [false]}), true);
  });

  it('reports a known NAT type via getNatType', async () => {
    const res = await call({method: 'getNatType', args: []});
    const known = ['Unknown', 'NoConnection', 'Symmetric', 'Restricted', 'FullCone'];
    assert.ok(
      known.includes(res.nat_type),
      `getNatType should be one of ${known.join('|')}, got ${res.nat_type}`,
    );
  });

  it('delivers via sendMessageToHandle and receives the echo (also in queued)', async () => {
    const text = `e2e-direct-handle ${Date.now()}`;
    const ok = await call({method: 'sendMessageToHandle', args: [echoTo, textMessage(text)]});
    assert.strictEqual(ok, true, 'sendMessageToHandle should report success');

    await waitForFeed(`Echo: ${text}`, ECHO_TIMEOUT);

    // The received echo is captured in the server-side queue too.
    const q = await call({method: 'queued', args: []});
    assert.ok(
      q.some((m: any) => typeof m.text === 'string' && m.text.includes(`Echo: ${text}`)),
      'queued() should contain the received echo',
    );
  });

  it('delivers via sendMessageToId (looked-up id) and receives the echo', async () => {
    const id = await call({method: 'handleLookup', args: [echoTo]});
    const text = `e2e-direct-id ${Date.now()}`;
    const ok = await call({method: 'sendMessageToId', args: [id, textMessage(text)]});
    assert.strictEqual(ok, true, 'sendMessageToId should report success');
    await waitForFeed(`Echo: ${text}`, ECHO_TIMEOUT);
  });

  // Kept last: stopping the engine is terminal for the other cases above.
  it('reports isStarted true while running and false after stop', async () => {
    assert.strictEqual(await call({method: 'isStarted', args: []}), true);
    await call({method: 'stop', args: []});
    assert.strictEqual(await call({method: 'isStarted', args: []}), false);
  });
});
