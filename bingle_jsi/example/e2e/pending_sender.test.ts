/// <reference types="detox" />
/**
 * Shared pending-message sender e2e for the bingle_jsi Detox harness (issue #283).
 *
 * Always inits with `send_pending_messages: true`, so queued messages go through the bingle_local
 * `PendingSender` rather than the legacy JSI processing loop, over the real TypeScript → native →
 * Rust path. Covers delivery (with the echo), a message to an offline recipient staying pending
 * with a retryable cause, and the local store staying responsive while such a send is in flight
 * (the sender records outcomes without holding the local store's lock).
 *
 * Backend + credentials come from the environment (staged by run_e2e_ios.sh / run_e2e_android.sh);
 * the suite skips cleanly without them:
 *   BINGLE_E2E_BACKEND, BINGLE_E2E_NODE_FILE, BINGLE_E2E_STUN_FILE, BINGLE_E2E_PASSPHRASE,
 *   BINGLE_E2E_HANDLE, BINGLE_E2E_ECHO_TO   as for messaging.test.ts
 *   BINGLE_E2E_OFFLINE_HANDLE               (optional) a handle registered but offline; the
 *                                           offline cases skip when unset
 *   BINGLE_E2E_STORE_FORWARD=1              (optional) also turn on the store-and-forward send
 *                                           gate, so a failed send is posted to the recipient's
 *                                           Mailbox — the case where holding the store's lock would
 *                                           block the app. Uses BINGLE_E2E_SIDEWINDER_URL /
 *                                           BINGLE_E2E_SIDEWINDER_TOKEN when set (bearer override),
 *                                           otherwise on-chain discovery from the node file's app id.
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
const offlineHandle = process.env.BINGLE_E2E_OFFLINE_HANDLE || '';
const storeForward = process.env.BINGLE_E2E_STORE_FORWARD === '1';
const sidewinderUrl = process.env.BINGLE_E2E_SIDEWINDER_URL || null;
const sidewinderToken = process.env.BINGLE_E2E_SIDEWINDER_TOKEN || null;

const haveCreds = passphrase && handle && echoTo && nodeFile;
const describeOrSkip = haveCreds ? describe : describe.skip;
const itWithOffline = offlineHandle ? it : it.skip;

const LISTEN_TIMEOUT = 90000;
const DELIVER_TIMEOUT = 120000;
const ECHO_TIMEOUT = 120000;
const FAIL_TIMEOUT = 90000;
// How much slower than an idle call a local-store call may be while a send is in flight. The
// harness round trip itself takes around a second, so this allows for its jitter only.
const RESPONSIVE_MARGIN_MS = 2000;

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

/** Poll getMessages until our outbound message with `text` satisfies `done`. */
async function waitForMessage(
  text: string,
  timeoutMs: number,
  done: (m: any) => boolean,
  what: string,
): Promise<any> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const messages = await call({method: 'getMessages', args: []});
    const m = messages.find((x: any) => x.text === text && x.sender_handle === handle);
    if (m && done(m)) {
      return m;
    }
    await sleep(1500);
  }
  throw new Error(`message "${text}" did not ${what} within ${timeoutMs}ms`);
}

/** Wall-clock time of one harness call, in milliseconds. */
async function timeCall(method: string): Promise<number> {
  const start = Date.now();
  await call({method, args: []});
  return Date.now() - start;
}

describeOrSkip(`bingle_jsi shared pending-message sender (${backend})`, () => {
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
          local: localStatePath(`bingle_e2e_pending_sender_${Date.now()}.json`),
          send_pending_messages: true,
          store_and_forward_send: storeForward,
          sidewinder_node_url: storeForward ? sidewinderUrl : null,
          sidewinder_token: storeForward ? sidewinderToken : null,
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
      // ignore
    }
  });

  it('delivers a queued message to the echo peer and receives the echo', async () => {
    const text = `e2e-pending-sender ${Date.now()}`;
    await call({method: 'queueMessage', args: [[echoTo], text]});

    const sent = await waitForMessage(
      text,
      DELIVER_TIMEOUT,
      m => (m.progress ?? 0) >= 1.0,
      'deliver',
    );
    assert.ok(sent.failure_kind == null, `delivered message has failure_kind ${sent.failure_kind}`);
    await waitForFeed(`Echo: ${text}`, ECHO_TIMEOUT);
  });

  itWithOffline('keeps a message to an offline recipient pending with a retryable cause', async () => {
    const text = `e2e-pending-offline ${Date.now()}`;
    await call({method: 'queueMessage', args: [[offlineHandle], text]});

    const failed = await waitForMessage(
      text,
      FAIL_TIMEOUT,
      m => m.failure_kind != null || (storeForward && (m.progress ?? 0) >= 1.0),
      'gain a failure_kind or be handed off',
    );
    if (storeForward && failed.failure_kind == null) {
      // Handed off to the recipient's Mailbox: complete with no failure.
      assert.strictEqual(failed.progress, 1.0);
      return;
    }
    const retryable = await call({
      method: 'failureKindIsRetryable',
      args: [failed.failure_kind],
    });
    assert.strictEqual(retryable, true, `expected a retryable cause, got ${failed.failure_kind}`);
    assert.ok((failed.progress ?? 1) < 1.0, 'a retryable failure stays pending');
  });

  itWithOffline('keeps the local store responsive while a send is in flight', async () => {
    // Baseline: local-store calls with nothing in flight.
    const idleMessages = await timeCall('getMessages');
    const idleContacts = await timeCall('getContacts');

    // Queue to the offline recipient: the send waits out connect/relay timeouts (and, with the
    // store-and-forward gate on, posts to the Mailbox) while we call into the local store.
    const text = `e2e-pending-busy ${Date.now()}`;
    await call({method: 'queueMessage', args: [[offlineHandle], text]});
    await sleep(500);
    const busyMessages = await timeCall('getMessages');
    const busyContacts = await timeCall('getContacts');

    assert.ok(
      busyMessages <= idleMessages + RESPONSIVE_MARGIN_MS,
      `getMessages took ${busyMessages}ms during a send (idle ${idleMessages}ms)`,
    );
    assert.ok(
      busyContacts <= idleContacts + RESPONSIVE_MARGIN_MS,
      `getContacts took ${busyContacts}ms during a send (idle ${idleContacts}ms)`,
    );
  });
});
