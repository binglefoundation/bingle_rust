/**
 * Typed send-failure-cause e2e for the bingle_jsi Detox harness (issue #112; validates #99).
 *
 * Drives sends that fail deterministically and asserts the typed cause surfaces through the read
 * model — `failure_kind` on getMessages, and the derived `failureKindIsRetryable` — exactly as a
 * mobile app would see it. This is the first suite to read a non-null `failure_kind` across the
 * bridge, so it also guards the #99 plumbing end to end.
 *
 * Backend + credentials come from the environment (staged by run_e2e_ios.sh); skips cleanly without
 * them. In addition to the messaging vars, an optional:
 *   BINGLE_E2E_OFFLINE_HANDLE  a handle registered on-chain but currently offline (no advert),
 *                              used for the RecipientNotAdvertised case; that case skips if unset.
 */
const assert = require('assert');
const {call, textOf, sleep, resolveNetworkInputs, localStatePath} = require('./harness');

const backend = process.env.BINGLE_E2E_BACKEND || 'testnet';
const passphrase = process.env.BINGLE_E2E_PASSPHRASE || '';
const handle = process.env.BINGLE_E2E_HANDLE || '';
const nodeFile = process.env.BINGLE_E2E_NODE_FILE || '';
const stunFile = process.env.BINGLE_E2E_STUN_FILE || '';
const offlineHandle = process.env.BINGLE_E2E_OFFLINE_HANDLE || '';

const haveCreds = passphrase && handle && nodeFile;
const describeOrSkip = haveCreds ? describe : describe.skip;

const LISTEN_TIMEOUT = 90000;
const FAIL_TIMEOUT = 90000;

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

/** Poll getMessages until the outbound message with `text` gains a typed failure_kind. */
async function waitForFailureKind(text, timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const msgs = await call({method: 'getMessages', args: []});
    const m = msgs.find(x => x.text === text && x.sender_handle === handle);
    if (m && m.failure_kind != null) {
      return m;
    }
    await sleep(1500);
  }
  throw new Error(`message "${text}" did not gain a failure_kind within ${timeoutMs}ms`);
}

describeOrSkip(`bingle_jsi typed failure causes (${backend})`, () => {
  beforeAll(async () => {
    await device.launchApp({newInstance: true});
    await device.disableSynchronization();
    await waitFor(element(by.id('app-ready')))
      .toHaveText('ready')
      .withTimeout(30000);

    // Platform-appropriate network inputs: STUN inline, node_file on-device for Android (#131).
    const net = await resolveNetworkInputs(nodeFile, stunFile);

    await call({
      method: 'init',
      args: [
        {
          handle,
          passphrase,
          node_file: net.node_file,
          stun_servers: net.stun_servers,
          local: localStatePath('bingle_e2e_failure_state.json'),
        },
      ],
    });
    await call({method: 'importKeypair', args: [passphrase]});
    await call({method: 'setMessageCallback', args: []});
    await call({method: 'setListeningCallback', args: []});
    await call({method: 'start', args: []});
    try {
      // Sends are only attempted once the transport is up; wait for a return path.
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

  it('unregistered handle -> HandleNotFound (permanent, not retryable)', async () => {
    const text = `e2e-nohandle ${Date.now()}`;
    // A handle that is guaranteed not registered on chain -> the send resolves to no account.
    const bogus = `e2e-nobody-${Date.now()}`;
    await call({method: 'queueMessage', args: [[bogus], text]});

    const m = await waitForFailureKind(text, FAIL_TIMEOUT);
    assert.strictEqual(
      m.failure_kind,
      'HandleNotFound',
      `expected HandleNotFound, got ${m.failure_kind} (reason: ${m.failure_reason})`,
    );
    const retryable = await call({
      method: 'failureKindIsRetryable',
      args: [m.failure_kind],
    });
    assert.strictEqual(retryable, false, 'HandleNotFound must be permanent (not retryable)');
  });

  // Needs a handle registered on chain but currently offline (no advert record). Skips otherwise.
  (offlineHandle ? it : it.skip)(
    'registered-but-offline recipient -> RecipientNotAdvertised (retryable)',
    async () => {
      const text = `e2e-offline ${Date.now()}`;
      await call({method: 'queueMessage', args: [[offlineHandle], text]});

      const m = await waitForFailureKind(text, FAIL_TIMEOUT);
      assert.strictEqual(
        m.failure_kind,
        'RecipientNotAdvertised',
        `expected RecipientNotAdvertised, got ${m.failure_kind} (reason: ${m.failure_reason})`,
      );
      const retryable = await call({
        method: 'failureKindIsRetryable',
        args: [m.failure_kind],
      });
      assert.strictEqual(retryable, true, 'RecipientNotAdvertised must be retryable');
    },
  );
});
