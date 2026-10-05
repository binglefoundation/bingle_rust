/// <reference types="detox" />
/**
 * Store-and-forward receive e2e for the bingle_jsi Detox harness (issue #210).
 *
 * Proves the fields carried by a message read from the device's own Sidewinder Mailbox reach
 * JavaScript through `getMessages`, over the real TypeScript → native → Rust path. The device sends
 * `mailbox-echo <text>` to the localnet echo peer, which seals `<text>` to the device's account, posts
 * it to the device's Mailbox, and then replies live with the `sent_time` and `signature` it sealed.
 * The device polls its Mailbox (`foregrounding`) and the suite checks the stored message against
 * those values.
 *
 * Needs the localnet backend with its Sidewinder node (the provisioner's echo peer answers
 * `mailbox-echo` only when a Mailbox is running), so it skips cleanly without them:
 *   BINGLE_E2E_BACKEND=localnet, BINGLE_E2E_NODE_FILE, BINGLE_E2E_STUN_FILE, BINGLE_E2E_PASSPHRASE,
 *   BINGLE_E2E_HANDLE, BINGLE_E2E_ECHO_TO   as for messaging.test.ts
 *   BINGLE_E2E_STORE_FORWARD=1, BINGLE_E2E_SIDEWINDER_URL / _TOKEN   see harness.storeForwardConfig
 */
import {describe, it, beforeAll, afterAll} from '@jest/globals';
import assert from 'assert';
import {
  call,
  textOf,
  sleep,
  resolveNetworkInputs,
  localStatePath,
  storeForwardConfig,
} from './harness';

const backend = process.env.BINGLE_E2E_BACKEND || 'testnet';
const passphrase = process.env.BINGLE_E2E_PASSPHRASE || '';
const handle = process.env.BINGLE_E2E_HANDLE || '';
const echoTo = process.env.BINGLE_E2E_ECHO_TO || '';
const nodeFile = process.env.BINGLE_E2E_NODE_FILE || '';
const stunFile = process.env.BINGLE_E2E_STUN_FILE || '';
const storeForwardInit = storeForwardConfig();

// The `mailbox-echo` command is answered only by the localnet provisioner's echo peer, and only
// when it runs a Sidewinder node (it then sets the URL).
const haveMailboxEcho =
  backend === 'localnet' &&
  passphrase &&
  handle &&
  echoTo &&
  nodeFile &&
  storeForwardInit?.sidewinder_node_url;
const describeOrSkip = haveMailboxEcho ? describe : describe.skip;

const LISTEN_TIMEOUT = 90000;
// The echo peer's Mailbox post waits for the transaction to finalise before it replies.
const REPLY_TIMEOUT = 120000;
const READ_TIMEOUT = 60000;
// Allowance for the emulator's clock against the host's when checking delivered_time.
const CLOCK_SKEW_MS = 5 * 60 * 1000;
const MAILBOX_SUITE = 'HPKE[DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305]';

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

/** Poll getMessages until a message matches `found`, polling the Mailbox first when `poll` is set. */
async function waitForStored(
  found: (m: any) => boolean,
  timeoutMs: number,
  what: string,
  poll: boolean,
): Promise<any> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (poll) {
      await call({method: 'foregrounding', args: []});
    }
    const messages = await call({method: 'getMessages', args: []});
    const m = messages.find(found);
    if (m) {
      return m;
    }
    await sleep(2000);
  }
  throw new Error(`${what} was not stored within ${timeoutMs}ms`);
}

describeOrSkip(`bingle_jsi store-and-forward receive (${backend})`, () => {
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
          local: localStatePath(`bingle_e2e_store_and_forward_${Date.now()}.json`),
          ...(storeForwardInit ?? {}),
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

  it('surfaces sent_time, delivered_time and signature of a message read from the Mailbox', async () => {
    const testStart = Date.now();
    const text = `e2e-mailbox ${testStart}`;
    await call({method: 'queueMessage', args: [[echoTo], `mailbox-echo ${text}`]});

    // The echo peer posts `text` to our Mailbox, then tells us live what it sealed.
    const reply = await waitForStored(
      (m: any) => typeof m.text === 'string' && m.text.startsWith('Mailbox'),
      REPLY_TIMEOUT,
      "the echo peer's Mailbox reply",
      false,
    );
    const sealed = /^Mailboxed sent_time=(\d+) signature=(\S+)$/.exec(reply.text);
    assert.ok(sealed, `unexpected echo reply: ${reply.text}`);
    const sentTime = Number(sealed[1]);
    const signature = sealed[2];

    // Read our Mailbox and find the stored message.
    const read = await waitForStored(
      (m: any) => m.text === text,
      READ_TIMEOUT,
      `the Mailbox message "${text}"`,
      true,
    );

    assert.strictEqual(read.sent_time, sentTime, 'sent_time should be what the sender sealed');
    assert.strictEqual(read.signature, signature, 'signature should be what the sender sealed');
    assert.strictEqual(typeof read.delivered_time, 'number', 'delivered_time should be a number');
    assert.ok(
      read.delivered_time >= testStart - CLOCK_SKEW_MS && read.delivered_time <= Date.now() + CLOCK_SKEW_MS,
      `delivered_time ${read.delivered_time} should be stamped during the test`,
    );
    assert.strictEqual(read.delivery_route, 'StoreAndForward');
    assert.strictEqual(read.cipher_suite, MAILBOX_SUITE);
  });
});
