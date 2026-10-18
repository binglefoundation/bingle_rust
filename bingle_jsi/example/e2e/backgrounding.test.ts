/// <reference types="detox" />
/**
 * Backgrounding hand-off e2e for the bingle_jsi Detox harness (issue #352).
 *
 * A message that has not been delivered when the app goes to the background must reach the
 * recipient's Mailbox before the app is suspended or killed, and the saved state must say so. The
 * suite queues a message to the offline fixture before the engine is listening (so its first send
 * has not been tried), backgrounds the app for real with `device.sendToHome()` (the harness forwards
 * the AppState change to `backgrounding`, as a host app does), waits, then kills it with
 * `device.terminateApp()`, as iOS does to a suspended app. It then checks:
 *   1. the state file the app saved records the hand-off (the message is in `forwarded_messages`,
 *      complete, with the StoreAndForward route);
 *   2. the offline recipient, launched fresh, reads the message from its Mailbox.
 *
 * Needs the localnet backend with its Sidewinder node, and skips cleanly without them:
 *   BINGLE_E2E_BACKEND=localnet, BINGLE_E2E_NODE_FILE, BINGLE_E2E_STUN_FILE, BINGLE_E2E_PASSPHRASE,
 *   BINGLE_E2E_HANDLE                        as for messaging.test.ts
 *   BINGLE_E2E_OFFLINE_HANDLE / _PASSPHRASE  the registered-but-offline fixture (the provisioner
 *                                            writes both)
 *   BINGLE_E2E_STORE_FORWARD=1, BINGLE_E2E_SIDEWINDER_URL / _TOKEN   see harness.storeForwardConfig
 *   BINGLE_E2E_SEND_PENDING_MESSAGES=1       (optional) run on the shared pending-message sender
 */
import {describe, it} from '@jest/globals';
import assert from 'assert';
import {
  call,
  sleep,
  resolveNetworkInputs,
  localStatePath,
  readLocalState,
  storeForwardConfig,
  withSenderSelection,
} from './harness';

const backend = process.env.BINGLE_E2E_BACKEND || 'testnet';
const passphrase = process.env.BINGLE_E2E_PASSPHRASE || '';
const handle = process.env.BINGLE_E2E_HANDLE || '';
const nodeFile = process.env.BINGLE_E2E_NODE_FILE || '';
const stunFile = process.env.BINGLE_E2E_STUN_FILE || '';
const offlineHandle = process.env.BINGLE_E2E_OFFLINE_HANDLE || '';
const offlinePassphrase = process.env.BINGLE_E2E_OFFLINE_PASSPHRASE || '';
const storeForwardInit = storeForwardConfig();

const haveFixtures =
  backend === 'localnet' &&
  passphrase &&
  handle &&
  nodeFile &&
  offlineHandle &&
  offlinePassphrase &&
  storeForwardInit?.sidewinder_node_url;
const describeOrSkip = haveFixtures ? describe : describe.skip;

// How long the app stays in the background before it is killed: longer than iOS lets a suspended
// app run without a background task, so a hand-off that relies on borrowed time is not credited.
const BACKGROUND_MS = 20000;
const READ_TIMEOUT = 60000;
// Two launches, the background wait and the Mailbox read can outlast Jest's default 120s; under
// `it.failing` a timeout would count as the expected failure, so allow well over the worst case.
const TEST_TIMEOUT = 300000;

async function launchReady(): Promise<void> {
  await device.launchApp({newInstance: true});
  await device.disableSynchronization();
  await waitFor(element(by.id('app-ready')))
    .toHaveText('ready')
    .withTimeout(30000);
}

describeOrSkip(`bingle_jsi backgrounding hand-off (${backend})`, () => {
  // Known failure (#352): `it.failing` passes while the hand-off is broken and fails once it works,
  // so switch it back to `it` with the fix. On the simulator the Mailbox post lands but the saved
  // state still shows the message unsent.
  it.failing('hands an unsent message to the Mailbox when the app backgrounds and is killed', async () => {
    const net = await resolveNetworkInputs(nodeFile, stunFile);
    const text = `e2e-background ${Date.now()}`;
    const senderState = localStatePath(`bingle_e2e_background_sender_${Date.now()}.json`);

    // Sender: queue before starting, so the first send is still untried when the app backgrounds.
    await launchReady();
    await call({
      method: 'init',
      args: [
        withSenderSelection({
          handle,
          passphrase,
          node_file: net.node_file,
          stun_servers: net.stun_servers,
          local: senderState,
          ...(storeForwardInit ?? {}),
        }),
      ],
    });
    await call({method: 'importKeypair', args: [passphrase]});
    await call({method: 'queueMessage', args: [[offlineHandle], text]});
    await call({method: 'start', args: []});
    await device.sendToHome();
    await sleep(BACKGROUND_MS);
    await device.terminateApp();

    // 1. The saved state records the hand-off. Collected rather than asserted, so the Mailbox check
    // below still runs and tells "never posted" apart from "posted but not saved".
    const problems: string[] = [];
    const state = readLocalState(senderState);
    const saved = (state.messages ?? []).find((m: any) => m.text === text);
    assert.ok(saved, 'the queued message is in the saved state');
    const record = JSON.stringify({...saved, signature: undefined});
    const forwarded = (state.forwarded_messages ?? []).some(
      (f: any) => f.message_id === saved.id && f.handle === offlineHandle,
    );
    if (!forwarded) {
      problems.push('the saved state does not record the Mailbox post');
    }
    if (saved.progress !== 1.0 || saved.delivery_route !== 'StoreAndForward') {
      problems.push('the saved message is not complete by store-and-forward');
    }

    // 2. The offline recipient reads it from its Mailbox. A fresh state file and no `start`, so the
    // fixture stays unadvertised for the other suites.
    await launchReady();
    await call({
      method: 'init',
      args: [
        {
          handle: offlineHandle,
          passphrase: offlinePassphrase,
          node_file: net.node_file,
          stun_servers: net.stun_servers,
          local: localStatePath(`bingle_e2e_background_recipient_${Date.now()}.json`),
          ...(storeForwardInit ?? {}),
        },
      ],
    });
    await call({method: 'importKeypair', args: [offlinePassphrase]});
    const start = Date.now();
    let received: any = null;
    while (!received && Date.now() - start < READ_TIMEOUT) {
      await call({method: 'foregrounding', args: []});
      const messages = await call({method: 'getMessages', args: []});
      received = messages.find((m: any) => m.text === text);
      if (!received) {
        await sleep(2000);
      }
    }
    await call({method: 'backgrounding', args: []});
    if (!received) {
      problems.push(`the recipient did not read the message from its Mailbox within ${READ_TIMEOUT}ms`);
    } else if (received.delivery_route !== 'StoreAndForward') {
      problems.push(`the recipient's copy has route ${received.delivery_route}`);
    }
    assert.deepStrictEqual(problems, [], `saved sender record: ${record}`);
  }, TEST_TIMEOUT);
});
