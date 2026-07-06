# Blocking superseded apps and forcing a client upgrade

This is an interim mechanism, delivered alongside the local-storage migration
([migrate_local_storage.md](migrate_local_storage.md)).

## Goal

When the creator replaces the Bingle dapp with a newer one, old clients whose baked-in config
still points at the old `app_id` must be **forced to upgrade**. This is achieved two ways,
together — a **hard on-chain block** plus a **client-side prompt**:

1. **On-chain hard block** — once an app is marked superseded, its user-facing state-changing
   methods reject, so a stale client physically cannot transact against the dead app.
2. **Client prompt** — on start the client detects that its configured app is superseded and
   surfaces an `UPGRADE_REQUIRED` status, so the UI tells the user to update the app instead of
   silently failing.

This is the forward-pointing mirror of the `AncestorApps` migration lineage: the new app knows
its ancestors (for state migration); the old app knows its **successor** (to block + prompt).

## Design

A single global `SuccessorApp` on the contract, stored as an **8-byte big-endian app id in a
`Bytes` value** (the same encoding as `AncestorApps`, so the existing `app_global_bytes` reader
is reused). Zero-length / absent means "not superseded".

Because an app can only write its **own** global state, the creator marks the **old** app
superseded by calling `set_successor_app` **on the old app**, passing the new app as the
successor — the exact flip side of calling `set_predecessor_app` on the new app. Both are manual
creator actions at replacement time.

## Contract (`smart_contracts/bingle_dapp/contract.py`)

- Global `self.successor_app = GlobalState(Bytes, key="SuccessorApp")`.
- `@subroutine _is_superseded(self) -> bool` — true when `SuccessorApp` is non-empty.
- Creator-only `set_successor_app(successor: Application)` — asserts `Txn.sender ==
  Global.creator_address` and stores `op.itob(successor.id)`. Re-pointable.
- **Hard block**: `assert not self._is_superseded()` at the top of `register`, `buy_bingle`,
  `sell_bingle`, `register_endpoint`.
- **Not gated** (so the old app can be wound down and users can still migrate out): all
  admin/creator methods (`set_*`, `withdraw`, `migrate_global`, `migrate_reserve`,
  `set_successor_app`), the `OptIn` baremethod, and `migrate_local` (it runs on the *new* app and
  only *reads* the old app's local state).

## Rust (`src/blockchain/algo_bingle.rs`)

- `AlgoBingle::successor_app(app_id) -> Result<Option<u64>>` — reads `SuccessorApp` via
  `app_global_bytes` and decodes the 8 big-endian bytes; `None` when absent/empty.
- `AlgoBingle::set_successor_app(app_id, successor_app_id) -> Result<String>` — creator call to
  `set_successor_app(uint64)void` with the successor in the foreign apps array.

## Client detection + prompt

A new keypair status `UPGRADE_REQUIRED` is threaded through the status pipeline:

- `bingle_local` `keypair_status`: before the funding/registration checks it reads
  `successor_app(config.app_id)`; if set, it short-circuits and returns `UPGRADE_REQUIRED` (the
  whole app is retired regardless of this account's funding or handle). Best-effort — a read
  error falls through to the normal status rather than blocking the user.
- `bingle_jsi`: `KeypairStatus::UpgradeRequired` enum variant + `"UPGRADE_REQUIRED"` mapping in
  `parse_keypair_status`; the TS `KeypairStatus` enum gains `UpgradeRequired`.
- The JSI start path skips both `ensure_local_migrated` and `api.start()` when the status is
  `UPGRADE_REQUIRED`, so a stale client never runs against the dead app.

The React-Native `App.tsx` (separate app repo) renders `KeypairStatus`; it needs an
`UpgradeRequired` branch showing an "update Bingle" prompt. The new enum variant is additive, so
an un-updated RN app hits its default case harmlessly until then.

## Tests

- Contract (`tests/test_bingle_dapp.py`): `set_successor_app` by creator sets `SuccessorApp`;
  non-creator rejected; `register` / `buy_bingle` / `sell_bingle` / `register_endpoint` fail once
  superseded; an ungated method (`withdraw`) still succeeds when superseded.
- Rust localnet (`tests/integration/blockchain/block_old_app_localnet.rs`): `successor_app` is
  `None` initially and `Some(new)` after `set_successor_app`; `buy_bingle` against the superseded
  old app is rejected; and a client still migrates its local state out of the superseded old app
  into the (un-superseded) successor.

## Notes / limitations

- Only apps **built with this contract** can be superseded — an already-deployed old app has no
  `set_successor_app` method and can never be flagged. Protection applies from the next
  replacement onward.
- Hard-blocking `sell_bingle` means users cannot sell back on the dead app; the intent is that
  trading moves to the new app after upgrade.
- The RN `App.tsx` upgrade-prompt UI is an out-of-repo follow-up.
