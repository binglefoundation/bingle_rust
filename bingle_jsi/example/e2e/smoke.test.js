/**
 * Smoke e2e for the bingle_jsi Detox harness (issue #109).
 *
 * Proves the full TypeScript -> native -> Rust path works through the command dispatcher: the app
 * launches and becomes ready, `version()` works before init, and after `init` a generated keypair
 * round-trips through `keypairStatus`. This is the foundation the messaging / failure-cause suites
 * (issues #111, #112, ...) build on.
 */
const {call, expectStatus, runCommand} = require('./harness');

describe('bingle_jsi harness smoke', () => {
  beforeAll(async () => {
    await device.launchApp({newInstance: true});
  });

  it('launches and becomes ready', async () => {
    await waitFor(element(by.id('app-ready')))
      .toHaveText('ready')
      .withTimeout(30000);
  });

  it('returns version info before init', async () => {
    const version = await call({method: 'version', args: []});
    expect(version).toBeTruthy();
    expect(typeof version.version).toBe('string');
    expect(version.version.length).toBeGreaterThan(0);
  });

  it('inits, generates a keypair, and reads it back', async () => {
    await runCommand({method: 'init', args: []});
    await expectStatus('ok');

    const keypair = await call({method: 'generateKeypair', args: []});
    expect(typeof keypair.id).toBe('string');
    expect(keypair.id.length).toBeGreaterThan(0);

    const status = await call({method: 'keypairStatus', args: []});
    expect(status.id).toBe(keypair.id);
  });
});
