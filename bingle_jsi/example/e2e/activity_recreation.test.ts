/// <reference types="detox" />
/**
 * The harness survives Android recreating its activity (issue #431).
 *
 * An overlay update during a CI run changed the app's asset paths, and Android recreated the
 * activity (`CONFIG_ASSETS_PATHS`, which `android:configChanges` can't declare). The harness kept
 * its command status in the screen's React state, so the command in flight was lost and the suite
 * timed out waiting for a terminal status. The state now lives outside the React tree
 * (harnessStore.ts).
 *
 * This suite recreates the activity on purpose by changing the font scale, which the manifest's
 * `configChanges` doesn't list either, while a `delay` command is in flight. The command's result
 * must still reach the screen. Android only: iOS has no activity recreation.
 *
 * Note: Detox replaces the global `expect`, so value assertions use Node's `assert`.
 */
import {describe, it, beforeAll, afterAll} from '@jest/globals';
import assert from 'assert';
import {adbShell, runCommand, sleep, textOf, waitForTerminalStatus} from './harness';

const describeOnAndroid =
  device.getPlatform() === 'android' ? describe : describe.skip;

/** How long the in-flight command runs: long enough to span the recreation. */
const DELAY_MS = 8000;

describeOnAndroid('bingle_jsi harness survives activity recreation', () => {
  let originalFontScale = 'null';

  beforeAll(async () => {
    await device.launchApp({newInstance: true});
    // The engine keeps background work pending, so drive timing ourselves (see smoke.test.ts).
    await device.disableSynchronization();
    await waitFor(element(by.id('app-ready')))
      .toHaveText('ready')
      .withTimeout(30000);
    originalFontScale = adbShell('settings get system font_scale');
  });

  afterAll(() => {
    if (originalFontScale === 'null' || originalFontScale === '') {
      adbShell('settings delete system font_scale');
    } else {
      adbShell(`settings put system font_scale ${originalFontScale}`);
    }
  });

  it('reports a command in flight after the activity is recreated', async () => {
    const since = adbShell('date +%s');
    await runCommand({method: 'delay', args: [DELAY_MS]});
    assert.strictEqual(await textOf('cmd-status'), 'running');

    const scale = originalFontScale === '1.15' ? '1.0' : '1.15';
    adbShell(`settings put system font_scale ${scale}`);
    await sleep(3000);

    // The font scale change must really have recreated the activity, or this proves nothing.
    const events = adbShell(`logcat -b events -d -T ${since}.000`);
    assert.match(
      events,
      /wm_(relaunch_resume_activity|destroy_activity)[^\n]*binglejsiexample/,
      'changing the font scale did not recreate the activity',
    );

    const status = await waitForTerminalStatus(DELAY_MS + 30000);
    assert.strictEqual(status, 'ok');
    assert.strictEqual(await textOf('cmd-output'), String(DELAY_MS));
  });
});
