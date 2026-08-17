/// <reference types="detox" />
/**
 * Introspection + local-store e2e for the bingle_jsi Detox harness (issue #139, group A).
 *
 * Covers the read-only getters and local-store writes that need no network (only a state-file via
 * `init`): `getVersions`, `isStarted`/`networkAvailable` before the engine is started, and an
 * `addMessage` -> `getMessages` -> `updateMessageStatus` round-trip. Always runs.
 *
 * Note: Detox replaces the global `expect`, so value assertions use Node's `assert` (see harness.ts).
 */
import {describe, it, beforeAll} from '@jest/globals';
import assert from 'assert';
import {call, localStatePath} from './harness';

const backend = process.env.BINGLE_E2E_BACKEND || 'testnet';

describe(`bingle_jsi introspection + local store (${backend})`, () => {
  beforeAll(async () => {
    await device.launchApp({newInstance: true});
    await device.disableSynchronization();
    await waitFor(element(by.id('app-ready')))
      .toHaveText('ready')
      .withTimeout(30000);
    await call({
      method: 'init',
      args: [{local: localStatePath(`bingle_e2e_introspection_${Date.now()}.json`)}],
    });
  });

  it('getVersions returns a version per module', async () => {
    const versions = await call({method: 'getVersions', args: []});
    const modules = Object.keys(versions);
    assert.ok(modules.length > 0, 'getVersions should report at least one module');
    for (const name of modules) {
      assert.strictEqual(
        typeof versions[name].version,
        'string',
        `module ${name} should have a version string`,
      );
      assert.ok(versions[name].version.length > 0, `module ${name} version should be non-empty`);
    }
  });

  it('isStarted is false before the engine is started', async () => {
    assert.strictEqual(await call({method: 'isStarted', args: []}), false);
  });

  it('networkAvailable is false before the engine is started', async () => {
    // Reflects the P2P transport only; with the engine stopped there is no usable route.
    assert.strictEqual(await call({method: 'networkAvailable', args: [false]}), false);
  });

  it('round-trips a message through addMessage / getMessages / updateMessageStatus', async () => {
    const ts = Date.now();
    const text = `introspection-${ts}`;
    await call({method: 'addMessage', args: ['alice', ['me'], ts, text, null]});

    let messages = await call({method: 'getMessages', args: []});
    const added = messages.find((m: any) => m.timestamp === ts);
    assert.ok(added, 'the added message should be listed by getMessages');
    assert.strictEqual(added.text, text);
    assert.strictEqual(added.sender_handle, 'alice');

    // Progress updates are reflected on the stored message.
    await call({method: 'updateMessageStatus', args: [ts, 1.0, null]});
    messages = await call({method: 'getMessages', args: []});
    const updated = messages.find((m: any) => m.timestamp === ts);
    assert.ok(updated, 'the message should still be listed after updateMessageStatus');
    assert.strictEqual(updated.progress, 1.0, 'progress should be updated to 1.0');
  });
});
