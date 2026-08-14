/**
 * Messaging e2e for the bingle_jsi Detox harness (issue #111).
 *
 * Sends a message to a live echo peer and verifies both delivery and the echoed reply — the full
 * send/receive path over the real P2P transport, driven through the command-dispatcher harness.
 *
 * Backend + credentials come from the environment (staged by run_e2e_ios.sh). The suite skips
 * cleanly when they are unset, so it is a no-op without a funded account:
 *   BINGLE_E2E_BACKEND     testnet|localnet (localnet not yet supported)
 *   BINGLE_E2E_NODE_FILE   staged node-file path (e.g. /tmp/bingle_e2e_node.json)
 *   BINGLE_E2E_STUN_FILE   staged STUN list path
 *   BINGLE_E2E_PASSPHRASE  mnemonic of a funded, already-registered sender account
 *   BINGLE_E2E_HANDLE      that account's registered handle
 *   BINGLE_E2E_ECHO_TO     a live echo peer/relay handle that replies "Echo: <text>"
 */
const assert = require('assert');
const {call, textOf, sleep} = require('./harness');

const backend = process.env.BINGLE_E2E_BACKEND || 'testnet';
const passphrase = process.env.BINGLE_E2E_PASSPHRASE || '';
const handle = process.env.BINGLE_E2E_HANDLE || '';
const echoTo = process.env.BINGLE_E2E_ECHO_TO || '';
const nodeFile = process.env.BINGLE_E2E_NODE_FILE || '';
const stunFile = process.env.BINGLE_E2E_STUN_FILE || '';

const haveCreds = passphrase && handle && echoTo && nodeFile;
const describeOrSkip = haveCreds ? describe : describe.skip;

const LISTEN_TIMEOUT = 90000;
const DELIVER_TIMEOUT = 120000;
const ECHO_TIMEOUT = 120000;

/** Poll the event feed until it contains `substring`. */
async function waitForFeed(substring, timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if ((await textOf('event-feed')).includes(substring)) {
      return;
    }
    await sleep(1000);
  }
  throw new Error(`event feed never contained "${substring}" within ${timeoutMs}ms`);
}

/** Poll getMessages until the outbound message with `text` reaches a terminal delivered state. */
async function waitForDelivered(text, timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const messages = await call({method: 'getMessages', args: []});
    const m = messages.find(x => x.text === text && x.sender_handle === handle);
    if (m && (m.progress ?? 0) >= 1.0) {
      return m;
    }
    await sleep(1500);
  }
  throw new Error(`message "${text}" was not delivered within ${timeoutMs}ms`);
}

describeOrSkip(`bingle_jsi messaging (${backend})`, () => {
  beforeAll(async () => {
    await device.launchApp({newInstance: true});
    await device.disableSynchronization();
    await waitFor(element(by.id('app-ready')))
      .toHaveText('ready')
      .withTimeout(30000);

    // Init against the selected network with a messaging-specific state file (so a keypair a prior
    // smoke run left in the shared state file is not loaded instead of ours).
    await call({
      method: 'init',
      args: [
        {
          handle,
          passphrase,
          node_file: nodeFile,
          stun_servers_file: stunFile,
          local: '/tmp/bingle_e2e_messaging_state.json',
        },
      ],
    });
    // init only loads local state + sets the engine passphrase; it does NOT import the passphrase
    // into the local API keypair. Import the funded account explicitly (replaces any loaded keypair)
    // so start() sees FUNDED/ACTIVE rather than the Unfunded default (issue #111).
    await call({method: 'importKeypair', args: [passphrase]});
    // Activate the inbound-message + listening callbacks so the harness event feed receives them.
    await call({method: 'setMessageCallback', args: []});
    await call({method: 'setListeningCallback', args: []});
    // Start the P2P engine and wait until it reports a usable return path.
    await call({method: 'start', args: []});
    try {
      await waitForFeed('onListening true', LISTEN_TIMEOUT);
    } catch (e) {
      // Surface the engine's log output (routed into the event feed) so a listening failure is
      // diagnosable from the test output.
      // eslint-disable-next-line no-console
      console.log('DIAG event-feed at listening timeout:\n' + (await textOf('event-feed')));
      throw e;
    }
  });

  afterAll(async () => {
    // Best-effort: stop the engine so it does not linger between runs.
    try {
      await call({method: 'stop', args: []});
    } catch (_e) {
      // ignore
    }
  });

  it('delivers a message to the echo peer and receives the echo', async () => {
    const text = `bingle-e2e ${Date.now()}`;
    await call({method: 'queueMessage', args: [[echoTo], text]});

    // 1) The queued outbound message delivers (progress 1.0, no typed failure).
    const sent = await waitForDelivered(text, DELIVER_TIMEOUT);
    assert.ok(
      sent.failure_kind == null,
      `delivered message should have no failure_kind, got ${sent.failure_kind}`,
    );

    // 2) The echo peer replies "Echo: <text>", surfaced via onMessage in the event feed.
    await waitForFeed(`Echo: ${text}`, ECHO_TIMEOUT);
  });
});
