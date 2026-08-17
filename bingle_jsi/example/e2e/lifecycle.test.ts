/// <reference types="detox" />
/**
 * Account-lifecycle + contacts e2e for the bingle_jsi Detox harness (issue #113).
 *
 * Two contexts, driven through the command dispatcher:
 *
 *  1. "local" — keypair generation/import, keypair-status transitions (None -> Unfunded), the
 *     contacts CRUD/block surface, and a save/load round-trip. These need only the bingle_local API
 *     (enabled via `init`'s `local` state file) plus a reachable localnet for the balance read that
 *     keypairStatus does; they always run.
 *  2. "network" — the funded/active keypair-status path and `handleLookupPartial`, which need a
 *     funded, on-chain-registered fixture account. Credentials come from the environment (staged by
 *     run_e2e_ios.sh / run_e2e_android.sh) exactly as the messaging/failure suites; skips cleanly
 *     when unset.
 *
 * Each context uses a per-run local state file so a keypair or contact left by a prior run cannot
 * bleed in (the on-device state file persists across app instances). Contact ids are per-run unique
 * for the same reason (add_contact rejects a duplicate id).
 *
 * Note: Detox replaces the global `expect` with its element-matcher expect, so value assertions use
 * Node's built-in `assert` (see harness.ts / smoke.test.ts).
 */
import {describe, it, beforeAll} from '@jest/globals';
import assert from 'assert';
import {call, resolveNetworkInputs, localStatePath} from './harness';

const backend = process.env.BINGLE_E2E_BACKEND || 'testnet';
const passphrase = process.env.BINGLE_E2E_PASSPHRASE || '';
const handle = process.env.BINGLE_E2E_HANDLE || '';
const nodeFile = process.env.BINGLE_E2E_NODE_FILE || '';
const stunFile = process.env.BINGLE_E2E_STUN_FILE || '';

const haveCreds = passphrase && handle && nodeFile;
const describeOrSkipNetwork = haveCreds ? describe : describe.skip;

async function launchReady(): Promise<void> {
  await device.launchApp({newInstance: true});
  // bingle_jsi runs a P2P engine (background threads/timers) and RN dev mode keeps work pending, so
  // the app never reaches the fully-idle state Detox waits for. Disable auto-sync and drive timing
  // via the harness helpers (see smoke.test.ts).
  await device.disableSynchronization();
  await waitFor(element(by.id('app-ready')))
    .toHaveText('ready')
    .withTimeout(30000);
}

describe(`bingle_jsi account lifecycle (${backend})`, () => {
  describe('keypair, status, and contacts (local)', () => {
    beforeAll(async () => {
      await launchReady();
      // Per-run state file: a keypair/contact from an earlier run must not be loaded instead.
      await call({
        method: 'init',
        args: [{local: localStatePath(`bingle_e2e_lifecycle_${Date.now()}.json`)}],
      });
    });

    it('reports keypairStatus None before any keypair is set', async () => {
      const status = await call({method: 'keypairStatus', args: []});
      assert.strictEqual(
        status.status,
        'None',
        `fresh state should have no keypair, got ${JSON.stringify(status)}`,
      );
    });

    it('generates a keypair that reads back as Unfunded', async () => {
      const keypair = await call({method: 'generateKeypair', args: []});
      assert.strictEqual(typeof keypair.id, 'string');
      assert.ok(keypair.id.length > 0, 'keypair id should be non-empty');

      const status = await call({method: 'keypairStatus', args: []});
      assert.strictEqual(status.id, keypair.id, 'status id should match the generated keypair');
      assert.strictEqual(
        status.status,
        'Unfunded',
        `a freshly generated (0-balance) account should be Unfunded, got ${status.status}`,
      );
    });

    it('imports a keypair by its passphrase and derives the same id', async () => {
      // Generate one, then import its passphrase back: a deterministic round-trip needing no fixture.
      const generated = await call({method: 'generateKeypair', args: []});
      const imported = await call({method: 'importKeypair', args: [generated.passphrase]});
      assert.strictEqual(
        imported.id,
        generated.id,
        'importing the same mnemonic must derive the same address',
      );
    });

    it('reflects contact add/block/remove in getContacts', async () => {
      // Per-run unique ids (add_contact rejects a duplicate id, and the state file persists).
      const stamp = Date.now();
      const idA = `lifecycle-a-${stamp}`;
      const idB = `lifecycle-b-${stamp}`;

      await call({method: 'addContact', args: ['alice', idA, 'Manual']});
      await call({method: 'addContact', args: ['bob', idB, 'Manual']});

      let contacts = await call({method: 'getContacts', args: []});
      assert.ok(
        contacts.some((c: any) => c.id === idA && c.handle === 'alice'),
        'alice should be listed after add',
      );
      assert.ok(
        contacts.some((c: any) => c.id === idB && c.handle === 'bob'),
        'bob should be listed after add',
      );

      // Block hides the contact from getContacts (which returns unblocked only) and flips isBlocked.
      await call({method: 'blockContact', args: [idA]});
      assert.strictEqual(await call({method: 'isBlocked', args: [idA]}), true);
      contacts = await call({method: 'getContacts', args: []});
      assert.ok(!contacts.some((c: any) => c.id === idA), 'blocked alice should be hidden');
      assert.ok(contacts.some((c: any) => c.id === idB), 'bob should still be listed');

      // Remove drops the entry entirely; a removed (never-blocked) id reads back as not blocked.
      await call({method: 'removeContact', args: [idB]});
      contacts = await call({method: 'getContacts', args: []});
      assert.ok(!contacts.some((c: any) => c.id === idB), 'removed bob should be gone');

      await call({method: 'removeContact', args: [idA]});
      assert.strictEqual(
        await call({method: 'isBlocked', args: [idA]}),
        false,
        'a removed contact id should no longer be blocked',
      );
    });

    it('round-trips local state through save/load', async () => {
      const stamp = Date.now();
      const id = `lifecycle-persist-${stamp}`;
      const path = localStatePath(`bingle_e2e_lifecycle_save_${stamp}.json`);

      await call({method: 'addContact', args: ['carol', id, 'Manual']});
      await call({method: 'save', args: [path]});

      // Mutate away, then load the snapshot back and confirm the contact returns.
      await call({method: 'removeContact', args: [id]});
      let contacts = await call({method: 'getContacts', args: []});
      assert.ok(!contacts.some((c: any) => c.id === id), 'carol should be gone after remove');

      await call({method: 'load', args: [path]});
      contacts = await call({method: 'getContacts', args: []});
      assert.ok(
        contacts.some((c: any) => c.id === id && c.handle === 'carol'),
        'carol should be restored after load',
      );
    });
  });

  describeOrSkipNetwork('funded status and handle lookup (network)', () => {
    beforeAll(async () => {
      await launchReady();
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
            local: localStatePath(`bingle_e2e_lifecycle_net_${Date.now()}.json`),
          },
        ],
      });
      // init sets the engine passphrase but does not import it into the local API keypair; import the
      // funded, registered fixture explicitly so keypairStatus reads the funded/active path (#111).
      await call({method: 'importKeypair', args: [passphrase]});
    });

    it('reports the funded fixture as Funded or Active', async () => {
      const status = await call({method: 'keypairStatus', args: []});
      assert.ok(
        status.status === 'Funded' || status.status === 'Active',
        `funded fixture should be Funded/Active, got ${JSON.stringify(status)}`,
      );
      assert.ok(status.id && status.id.length > 0, 'funded status should carry the account id');
    });

    it('maps a handle prefix to its canonical handle via handleLookupPartial', async () => {
      const status = await call({method: 'keypairStatus', args: []});
      // Drop the last 2 chars to exercise prefix matching (the fixture handle is registered on chain
      // by the localnet provisioner / already registered on testnet). Guard very short handles.
      const prefixLen = Math.max(3, handle.length - 2);
      const prefix = handle.slice(0, prefixLen);

      const result = await call({method: 'handleLookupPartial', args: [prefix]});
      assert.strictEqual(
        result.canonical_handle,
        handle,
        `prefix "${prefix}" should resolve to canonical handle "${handle}"`,
      );
      assert.ok(result.id && result.id.length > 0, 'lookup should return an account id');
      // We are that handle, so the resolved id must be our own account id.
      assert.strictEqual(result.id, status.id, 'resolved id should be the fixture account id');
    });
  });
});
