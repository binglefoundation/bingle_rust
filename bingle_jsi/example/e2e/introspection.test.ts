/// <reference types="detox" />
/**
 * Introspection + local-store e2e for the bingle_jsi Detox harness (issue #139, group A).
 *
 * Covers the read-only getters and local-store writes that need no network (only a state-file via
 * `init`): `getVersions` (issue #144), `isStarted`/`networkAvailable` before the engine is
 * started, and an `addMessage` -> `getMessages` -> `updateMessageStatusById` round-trip. Always
 * runs. (The single-module `version()` is covered by smoke.test.ts.)
 *
 * Note: Detox replaces the global `expect`, so value assertions use Node's `assert` (see harness.ts).
 */
import {describe, it, beforeAll} from '@jest/globals';
import assert from 'assert';
import {assertNoStoreAndForwardFields, call, localStatePath} from './harness';

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

  it('lists the version of every library module via getVersions', async () => {
    const versions = await call({method: 'getVersions', args: []});
    assert.ok(versions && typeof versions === 'object', 'getVersions should resolve to a map');
    // The core library and the JSI binding each report their own version.
    for (const module of ['Base', 'JSI']) {
      const info = versions[module];
      assert.ok(info, `getVersions should include ${module}, got ${Object.keys(versions).join(',')}`);
      assert.strictEqual(typeof info.version, 'string');
      assert.ok(info.version.length > 0, `${module} version should be non-empty`);
      assert.strictEqual(typeof info.build_timestamp, 'string');
      assert.strictEqual(typeof info.build_number, 'string');
      assert.ok(info.git_sha === null || typeof info.git_sha === 'string', `${module} git_sha should be a string or null`);
    }
    // The single-module version() reports the core library, so it matches the Base entry.
    const single = await call({method: 'version', args: []});
    assert.strictEqual(versions.Base.version, single.version);
  });

  it('isStarted is false before the engine is started', async () => {
    assert.strictEqual(await call({method: 'isStarted', args: []}), false);
  });

  it('networkAvailable is false before the engine is started', async () => {
    // Reflects the P2P transport only; with the engine stopped there is no usable route.
    assert.strictEqual(await call({method: 'networkAvailable', args: [false]}), false);
  });

  it('round-trips a message through addMessage / getMessages / updateMessageStatusById', async () => {
    const ts = Date.now();
    const text = `introspection-${ts}`;
    await call({method: 'addMessage', args: ['alice', ['me'], ts, text, null]});

    let messages = await call({method: 'getMessages', args: []});
    const added = messages.find((m: any) => m.text === text);
    assert.ok(added, 'the added message should be listed by getMessages');
    assert.strictEqual(added.sender_handle, 'alice');
    // Every message carries a stable id (issue #209).
    assert.strictEqual(typeof added.id, 'string');
    assert.ok(added.id.length > 0, 'the message should have an id');
    // Not from a Mailbox, so the store-and-forward fields are present and null (issue #210).
    assertNoStoreAndForwardFields(added, 'a message stored with addMessage');

    // Progress updates by id are reflected on the stored message.
    await call({method: 'updateMessageStatusById', args: [added.id, 0.5, 'retrying']});
    messages = await call({method: 'getMessages', args: []});
    const updated = messages.find((m: any) => m.id === added.id);
    assert.ok(updated, 'the message should still be listed after updateMessageStatusById');
    assert.strictEqual(updated.progress, 0.5, 'progress should be updated to 0.5');
    assert.strictEqual(updated.failure_reason, 'retrying');

    // The deprecated timestamp form still works for apps that have not migrated (issue #209).
    await call({method: 'updateMessageStatus', args: [ts, 1.0, null]});
    messages = await call({method: 'getMessages', args: []});
    const viaTimestamp = messages.find((m: any) => m.id === added.id);
    assert.strictEqual(viaTimestamp.progress, 1.0, 'the timestamp form should still update progress');
  });
});
