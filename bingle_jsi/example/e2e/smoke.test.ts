/// <reference types="detox" />
/**
 * Smoke e2e for the bingle_jsi Detox harness (issue #109).
 *
 * Proves the full TypeScript -> native -> Rust path works through the command dispatcher: the app
 * launches and becomes ready, `version()` works before init, and after `init` (which enables the
 * bingle_local API via a state-file path) a generated keypair round-trips through `keypairStatus`.
 * This is the foundation the messaging / failure-cause suites (issues #111, #112, ...) build on.
 *
 * Note: Detox replaces the global `expect` with its own element-matcher expect, so value
 * assertions use Node's built-in `assert` rather than Jest's `expect`.
 */
import {describe, it, beforeAll} from '@jest/globals';
import assert from 'assert';
import {call, textOf} from './harness';

describe('bingle_jsi harness smoke', () => {
  beforeAll(async () => {
    await device.launchApp({newInstance: true});
    // bingle_jsi runs a P2P engine (background threads/timers), and RN dev mode keeps its own work
    // pending, so the app never reaches the fully-idle state Detox waits for by default. Disable
    // automatic synchronization and drive timing explicitly via the harness helpers (issue #116).
    await device.disableSynchronization();
    await waitFor(element(by.id('app-ready')))
      .toHaveText('ready')
      .withTimeout(30000);
  });

  it('launches and becomes ready', async () => {
    assert.strictEqual(await textOf('app-ready'), 'ready');
  });

  it('returns version info before init', async () => {
    const version = await call({method: 'version', args: []});
    assert.ok(version, 'version result should be present');
    assert.strictEqual(typeof version.version, 'string');
    assert.ok(version.version.length > 0, 'version string should be non-empty');
  });

  it('inits, generates a keypair, and reads it back', async () => {
    await call({method: 'init', args: []});

    const keypair = await call({method: 'generateKeypair', args: []});
    assert.strictEqual(typeof keypair.id, 'string');
    assert.ok(keypair.id.length > 0, 'keypair id should be non-empty');

    const status = await call({method: 'keypairStatus', args: []});
    assert.strictEqual(
      status.id,
      keypair.id,
      'keypairStatus should return the generated keypair id',
    );
  });
});
