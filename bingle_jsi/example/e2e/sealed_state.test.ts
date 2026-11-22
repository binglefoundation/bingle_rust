/// <reference types="detox" />
/**
 * Sealed state file e2e for the bingle_jsi Detox harness (issue #478).
 *
 * The sealed state file (#459) is unit-tested in bingle_local over an in-memory Keychain. This suite
 * runs it through the real iOS Keychain store, the Swift bridge and the engine together:
 *   1. the file on disk holds no readable handle, address, phrase, message or app record;
 *   2. a file whose device secret is gone, as after a restore to another device, opens only with the
 *      account's recovery phrase, and a wrong phrase leaves it byte-for-byte intact;
 *   3. a plain file is sealed on first load without losing anything;
 *   4. after a reinstall (the app's files gone, the Keychain kept) the stored key brings the account
 *      back;
 *   5. removing the key from the device deletes the device secret too, so a copy of the file no
 *      longer opens without the phrase.
 *
 * `device.clearKeychain()` stands in for a restore to another device: the device secret is a
 * `ThisDeviceOnly` Keychain item, which iOS never restores elsewhere. Each case starts from a cleared
 * Keychain and its own state directory (the reinstall marker lives beside the state file), so the
 * cases run in any order.
 *
 * iOS simulator only, and local only like the other iOS suites. Needs no network: it uses only the
 * local API. What a simulator can't show (Data Protection while locked, iCloud Keychain sync, Apple's
 * backup restore) stays a manual device check; see CI-E2E.md.
 */
import {describe, it, beforeEach} from '@jest/globals';
import assert from 'assert';
import fs from 'fs';
import {call, callExpectingError} from './harness';

const describeOrSkip = device.getPlatform() === 'ios' ? describe : describe.skip;

const CONTACT_HANDLE = 'sealed-e2e-contact';
const CONTACT_ID = 'SEALEDE2ECONTACTADDRESS';
const MESSAGE_TEXT = 'sealed e2e message text';
const RECORD_NAME = 'read_state';
const RECORD_VALUE = '{"since":1,"lastRead":{"sealed-e2e-contact":2}}';
// Up to three launches per case outlast Jest's default 120s, and a newly created simulator is slow
// for its first few launches.
const TEST_TIMEOUT = 600000;

/** A fresh state directory for one case, and its state file. */
function stateFile(name: string): {dir: string; path: string} {
  const dir = `/tmp/bingle_e2e_sealed_${name}_${Date.now()}`;
  fs.mkdirSync(dir, {recursive: true});
  return {dir, path: `${dir}/bingle_state_testnet.json`};
}

async function launchReady(): Promise<void> {
  await device.launchApp({newInstance: true});
  await device.disableSynchronization();
  await waitFor(element(by.id('app-ready')))
    .toHaveText('ready')
    .withTimeout(30000);
}

/** Launch and init over `path` with the Keychain store, sealing unless `encrypt` is false. */
async function launchOver(path: string, encrypt: boolean = true): Promise<void> {
  await launchReady();
  await call({
    method: 'init',
    args: [{local: path, key_storage: 'keychain', encrypt_state: encrypt}],
  });
}

/** Create an account with a contact, a message and an app record, and save it. */
async function createAccount(path: string): Promise<{id: string; passphrase: string}> {
  const kp = await call({method: 'generateKeypair', args: []});
  await call({method: 'addContact', args: [CONTACT_HANDLE, CONTACT_ID, 'Manual']});
  await call({
    method: 'addMessage',
    args: [CONTACT_HANDLE, [kp.id], Date.now(), MESSAGE_TEXT, null],
  });
  await call({method: 'setAppRecord', args: [RECORD_NAME, RECORD_VALUE]});
  await call({method: 'save', args: [path]});
  return kp;
}

/** Asserts the loaded state has the account's contact, message and app record. */
async function assertAccountData(): Promise<void> {
  const contacts = await call({method: 'getContacts', args: []});
  assert.ok(
    contacts.some((c: any) => c.handle === CONTACT_HANDLE && c.id === CONTACT_ID),
    `contact missing: ${JSON.stringify(contacts)}`,
  );
  const messages = await call({method: 'getMessages', args: []});
  assert.ok(
    messages.some((m: any) => m.text === MESSAGE_TEXT),
    `message missing: ${JSON.stringify(messages)}`,
  );
  assert.strictEqual(await call({method: 'appRecord', args: [RECORD_NAME]}), RECORD_VALUE);
}

/** Asserts none of `secrets` can be read in the file's bytes. */
function assertUnreadable(path: string, secrets: string[]): void {
  const bytes = fs.readFileSync(path);
  for (const secret of secrets) {
    assert.ok(!bytes.includes(Buffer.from(secret)), `"${secret}" is readable in ${path}`);
  }
  assert.throws(() => JSON.parse(bytes.toString('utf8')), 'the file is still plain JSON');
}

describeOrSkip('bingle_jsi sealed state file (iOS Keychain)', () => {
  beforeEach(async () => {
    await device.clearKeychain();
  }, TEST_TIMEOUT);

  it('holds no readable account data in the file', async () => {
    const {path} = stateFile('raw');
    await launchOver(path);
    const kp = await createAccount(path);

    assert.strictEqual(await call({method: 'stateFileStatus', args: []}), 'sealed');
    assertUnreadable(path, [kp.id, kp.passphrase, CONTACT_HANDLE, CONTACT_ID, MESSAGE_TEXT, RECORD_VALUE]);
  }, TEST_TIMEOUT);

  it('opens a file without its device secret only with the recovery phrase', async () => {
    // Another account's valid phrase, from a plain file the suite throws away.
    const other = stateFile('other');
    await launchReady();
    await call({method: 'init', args: [{local: other.path, key_storage: 'file', encrypt_state: false}]});
    const otherPhrase = (await call({method: 'generateKeypair', args: []})).passphrase;

    const {path} = stateFile('restore');
    await launchOver(path);
    const kp = await createAccount(path);

    // A restore to another device: the file without this device's Keychain.
    await device.clearKeychain();
    await launchOver(path);
    assert.strictEqual(await call({method: 'stateFileStatus', args: []}), 'recovery_phrase_required');
    const restored = fs.readFileSync(path);

    for (const wrong of [otherPhrase, 'not a recovery phrase']) {
      const err = await callExpectingError({method: 'openStateWithRecoveryPhrase', args: [wrong]});
      assert.ok(err.length > 0);
      assert.ok(fs.readFileSync(path).equals(restored), 'a wrong phrase changed the file');
    }

    const opened = await call({method: 'openStateWithRecoveryPhrase', args: [kp.passphrase]});
    assert.strictEqual(opened.id, kp.id);
    await assertAccountData();
    assert.strictEqual(await call({method: 'stateFileStatus', args: []}), 'sealed');

    // From now on it opens on this device without the phrase.
    await launchOver(path);
    assert.strictEqual(await call({method: 'stateFileStatus', args: []}), 'sealed');
    assert.strictEqual((await call({method: 'exportKeypair', args: []})).id, kp.id);
    await assertAccountData();
  }, TEST_TIMEOUT);

  it('seals a plain file on first load without losing anything', async () => {
    const {path} = stateFile('migrate');
    await launchOver(path, false);
    const kp = await createAccount(path);
    // stateFileStatus reports what the last load found, and this file hasn't been loaded yet.
    const plain = JSON.parse(fs.readFileSync(path, 'utf8'));
    assert.ok(Array.isArray(plain.messages), 'the plain file is JSON with messages');

    await launchOver(path);
    assert.strictEqual(await call({method: 'stateFileStatus', args: []}), 'sealed');
    assertUnreadable(path, [kp.id, CONTACT_HANDLE, MESSAGE_TEXT, RECORD_VALUE]);
    assert.strictEqual((await call({method: 'exportKeypair', args: []})).id, kp.id);
    await assertAccountData();
  }, TEST_TIMEOUT);

  it('brings the account back from the Keychain after a reinstall', async () => {
    const {dir, path} = stateFile('reinstall');
    await launchOver(path);
    const kp = await createAccount(path);

    // A reinstall deletes the app's files, the install marker with them, but keeps the Keychain.
    fs.rmSync(dir, {recursive: true, force: true});
    fs.mkdirSync(dir, {recursive: true});
    await launchOver(path);
    const status = await call({method: 'keyStorageStatus', args: []});
    assert.strictEqual(status.reinstall_detected, true, JSON.stringify(status));
    assert.strictEqual(status.stored_address, kp.id, JSON.stringify(status));

    const restored = await call({method: 'restoreStoredKey', args: []});
    assert.strictEqual(restored.id, kp.id);
    assert.strictEqual(restored.passphrase, kp.passphrase);
    assert.strictEqual(await call({method: 'stateFileStatus', args: []}), 'sealed');
    assertUnreadable(path, [kp.id, kp.passphrase]);
  }, TEST_TIMEOUT);

  it('deletes the device secret with the key, so a copy of the file needs the phrase', async () => {
    const {dir, path} = stateFile('remove');
    await launchOver(path);
    await createAccount(path);
    const copy = `${dir}/copy.bin`;
    fs.copyFileSync(path, copy);

    await call({method: 'removeKeyFromDevice', args: []});
    assert.ok(!fs.existsSync(path), 'the state file was deleted');

    fs.copyFileSync(copy, path);
    await launchOver(path);
    assert.strictEqual(await call({method: 'stateFileStatus', args: []}), 'recovery_phrase_required');
    const status = await call({method: 'keyStorageStatus', args: []});
    assert.ok(status.stored_address == null, `a key is still stored: ${JSON.stringify(status)}`);
  }, TEST_TIMEOUT);
});
