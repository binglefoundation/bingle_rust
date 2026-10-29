// Generates registration_groups.json: unsigned registration and recovery groups built by the
// registration service's own group builder (bingle_registrar service/src/registration/group.ts),
// with fixed test keys, plus the signed client transactions algosdk produces for them. The Rust
// tests check bingle_core's validator accepts these groups and signs them byte for byte the same.
//
// Run it from a bingle_registrar checkout on master, which supplies algosdk, esbuild and
// group.ts, after copying this file to that checkout's tmp/ directory (from this directory):
// cp generate_registration_groups.mjs ~/dev/bingle_registrar/tmp/ && (cd ~/dev/bingle_registrar && npx esbuild tmp/generate_registration_groups.mjs --bundle --platform=node --format=cjs --outfile=tmp/gen.cjs && node tmp/gen.cjs) > registration_groups.json

import algosdk from 'algosdk';
import { buildRecoveryGroup, buildRegistrationGroup } from '../service/src/registration/group.ts';

// Test-only keys from fixed seeds: never fund these on MainNet.
const account = (fill) => {
  const mnemonic = algosdk.mnemonicFromSeed(new Uint8Array(32).fill(fill));
  return { mnemonic, ...algosdk.mnemonicToSecretKey(mnemonic) };
};
const client = account(1);
const recovered = account(2);
const funder = account(3);
const service = account(4);

const bingleAppId = 771828003n;
const bingleAssetId = 768197849n;
const registrarAppId = 774577089n;
// The handle as typed: mixed case and a non-ASCII character, which register(handle) keeps as is.
const handle = 'Zoë_Test';

const suggestedParams = {
  flatFee: true,
  fee: 1000n,
  minFee: 1000n,
  firstValid: 100n,
  lastValid: 1100n,
  genesisID: 'testnet-v1.0',
  genesisHash: new Uint8Array(32).fill(7),
};
const installHash = new Uint8Array(32).fill(9);
const inviteHash = new Uint8Array(32).fill(10);
const b64 = (bytes) => Buffer.from(bytes).toString('base64');

const view = (group, signers) => ({
  transactions: group.transactions.map((txn) => b64(algosdk.encodeUnsignedTransaction(txn))),
  sign_indexes: group.clientIndexes,
  signed_transactions: group.clientIndexes.map((index) =>
    b64(group.transactions[index].signTxn(signers(index).sk)),
  ),
});

const registration = (invite) =>
  buildRegistrationGroup({
    suggestedParams,
    funder: funder.addr.toString(),
    service: service.addr.toString(),
    address: client.addr.toString(),
    handle,
    bingleAppId,
    bingleAssetId,
    registrarAppId,
    installHash,
    claimKind: 'free',
    invite,
    funding: 696500n,
  });

const recovery = (withInvite) =>
  buildRecoveryGroup({
    suggestedParams,
    funder: funder.addr.toString(),
    service: service.addr.toString(),
    oldAddress: client.addr.toString(),
    newAddress: recovered.addr.toString(),
    handle,
    bingleAppId,
    bingleAssetId,
    registrarAppId,
    installHash,
    inviteHash: withInvite ? inviteHash : undefined,
  });

const recoverySigner = (index) => (index <= 2 ? client : recovered);

const fixture = {
  generated_by: 'generate_registration_groups.mjs',
  bingle_app_id: Number(bingleAppId),
  bingle_asset_id: Number(bingleAssetId),
  handle,
  client: { mnemonic: client.mnemonic, address: client.addr.toString() },
  recovered: { mnemonic: recovered.mnemonic, address: recovered.addr.toString() },
  funder: { address: funder.addr.toString() },
  service: { address: service.addr.toString() },
  registration: view(registration(undefined), () => client),
  registration_invite: view(
    registration({ inviteHash, inviter: service.addr.toString() }),
    () => client,
  ),
  recovery: view(recovery(false), recoverySigner),
  recovery_invite: view(recovery(true), recoverySigner),
};

process.stdout.write(`${JSON.stringify(fixture, null, 2)}\n`);
