/// <reference types="detox" />
/**
 * Prompt Mailbox read e2e for the bingle_jsi Detox harness (issue #358).
 *
 * Opening the app must read the device's Sidewinder Mailbox at once, not at the poller's next
 * 120s cycle, and each finished check must reach JavaScript as an `onMailboxChecked` event (with the
 * number of messages read, 0 included) so a host can clear a "Checking for messages" banner. The
 * device sends `mailbox-echo <text>` to the localnet echo peer, which posts `<text>` to the device's
 * Mailbox and replies live once it is there. The suite then opens the app — by calling
 * `foregrounding` while the poller is already running, and by a real resume — and checks the
 * message is read, and the check reported, well inside the poll interval.
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
// A prompt read: well inside the poller's 120s interval, so only an immediate poll passes.
const PROMPT_TIMEOUT = 20000;
// A reply wait plus a prompt read can outlast Jest's default 120s.
const TEST_TIMEOUT = 240000;
// The launch wait plus the listening wait can outlast Jest's default 120s.
const SETUP_TIMEOUT = 180000;

/** The `read` counts of the `onMailboxChecked` events in the feed, oldest first. */
async function mailboxChecks(): Promise<number[]> {
  const feed = await textOf('event-feed');
  return feed
    .split('\n')
    .map(line => /^onMailboxChecked (\d+)$/.exec(line))
    .filter((m): m is RegExpExecArray => m !== null)
    .map(m => Number(m[1]));
}

/** Wait for an `onMailboxChecked` event beyond the first `seen`, and return its `read` count. */
async function waitForCheck(seen: number, timeoutMs: number, what: string): Promise<number> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const checks = await mailboxChecks();
    if (checks.length > seen) {
      return checks[checks.length - 1];
    }
    await sleep(500);
  }
  throw new Error(`no onMailboxChecked event ${what} within ${timeoutMs}ms`);
}

/** Poll getMessages (without polling the Mailbox) until a message matches `found`. */
async function waitForStored(found: (m: any) => boolean, timeoutMs: number, what: string): Promise<any> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const messages = await call({method: 'getMessages', args: []});
    const m = messages.find(found);
    if (m) {
      return m;
    }
    await sleep(500);
  }
  throw new Error(`${what} was not stored within ${timeoutMs}ms`);
}

/**
 * Have the echo peer post `text` to our Mailbox and wait until it says the post is done. Asserts the
 * message has not been read yet, so the open that follows is what reads it.
 */
async function postToOwnMailbox(text: string): Promise<void> {
  const before = await call({method: 'getMessages', args: []});
  const earlierReplies = new Set(
    before.filter((m: any) => isMailboxReply(m)).map((m: any) => m.id),
  );
  await call({method: 'queueMessage', args: [[echoTo], `mailbox-echo ${text}`]});
  await waitForStored(
    (m: any) => isMailboxReply(m) && !earlierReplies.has(m.id),
    REPLY_TIMEOUT,
    "the echo peer's Mailbox reply",
  );
  const now = await call({method: 'getMessages', args: []});
  assert.ok(
    !now.some((m: any) => m.text === text),
    'the message was read before the app was opened (the poller cycle ran first), so this run proves nothing',
  );
}

function isMailboxReply(m: any): boolean {
  return typeof m.text === 'string' && m.text.startsWith('Mailboxed ');
}

describeOrSkip(`bingle_jsi prompt Mailbox read (${backend})`, () => {
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
          local: localStatePath(`bingle_e2e_mailbox_check_${Date.now()}.json`),
          ...(storeForwardInit ?? {}),
        },
      ],
    });
    await call({method: 'importKeypair', args: [passphrase]});
    await call({method: 'setMessageCallback', args: []});
    await call({method: 'setListeningCallback', args: []});
    await call({method: 'setMailboxCheckCallback', args: []});
    await call({method: 'start', args: []});
    const start = Date.now();
    while (!(await textOf('event-feed')).includes('onListening true')) {
      if (Date.now() - start > LISTEN_TIMEOUT) {
        throw new Error(`not listening within ${LISTEN_TIMEOUT}ms`);
      }
      await sleep(1000);
    }
  }, SETUP_TIMEOUT);

  afterAll(async () => {
    try {
      await call({method: 'stop', args: []});
    } catch (_e) {
      // ignore
    }
  });

  it('reports a finished Mailbox check to JavaScript', async () => {
    // Only `foregrounding` starts the poller, so no check has run yet.
    assert.deepStrictEqual(await mailboxChecks(), [], 'no check before the poller starts');
    await call({method: 'foregrounding', args: []});
    const read = await waitForCheck(0, PROMPT_TIMEOUT, 'after the poller starts');
    assert.ok(read >= 0, `read count ${read}`);
  });

  it(
    'reads a waiting message at once when foregrounding finds the poller running',
    async () => {
      const text = `e2e-mailbox-check-wake ${Date.now()}`;
      await postToOwnMailbox(text);

      const seen = (await mailboxChecks()).length;
      await call({method: 'foregrounding', args: []});
      await waitForStored((m: any) => m.text === text, PROMPT_TIMEOUT, `the Mailbox message "${text}"`);
      const read = await waitForCheck(seen, PROMPT_TIMEOUT, 'after foregrounding');
      assert.ok(read >= 1, `the check should report the message it read, got ${read}`);
    },
    TEST_TIMEOUT,
  );

  it(
    'reads a waiting message at once when the app resumes',
    async () => {
      const text = `e2e-mailbox-check-resume ${Date.now()}`;
      await postToOwnMailbox(text);

      const seen = (await mailboxChecks()).length;
      // The harness forwards the AppState changes to backgrounding and foregrounding, as a host does.
      await device.sendToHome();
      await sleep(2000);
      await device.launchApp({newInstance: false});
      await device.disableSynchronization();
      await waitForStored((m: any) => m.text === text, PROMPT_TIMEOUT, `the Mailbox message "${text}"`);
      const read = await waitForCheck(seen, PROMPT_TIMEOUT, 'after the resume');
      assert.ok(read >= 1, `the check should report the message it read, got ${read}`);
    },
    TEST_TIMEOUT,
  );
});
