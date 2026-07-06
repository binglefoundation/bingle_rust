# Local Storage Migration Across Dapp Upgrades

Status: design (interim solution). Branch: `migrate_local_storage`.

## Context

When a new Bingle dapp is deployed to replace an older one, each user's per-account **local
state** must be carried over: `Handle`/`HandleTime`, `allow_static`, `allow_relay`, and
`static_endpoint`/`static_endpoint_x`.

An **admin-driven** batch migration was investigated and rejected. On Algorand an application
can only write **local** state for accounts that have opted into *that* application, so an
admin can never pre-populate a new user's local state. Migration is therefore inherently
**user-signed**.

A user-driven `migrate_local(old_app)` method already exists in the contract, but today it is:

1. never invoked automatically, so it relies on the user taking a manual action; and
2. limited to the single **immediate** predecessor — a user two versions behind (data on v1,
   whose successor v2 was skipped, now facing v3) is stranded even though their data still
   exists on-chain.

This design makes migration **automatic and transparent on start**, and generalises the
accepted source app from a single predecessor to a **creator-blessed ancestor set**, so a
user any number of versions behind migrates directly in one hop.

## Design overview

1. **Contract**: replace the single `PredecessorApp` global with an accepted-ancestor
   **lineage set**; `migrate_local` accepts any app in that set. The lineage accumulates
   automatically when the creator points a new app at its immediate predecessor.
2. **Client**: on start, if the account is not yet registered on the new app but holds local
   state on a blessed ancestor, opt into the new app and call `migrate_local(ancestor)` once.
   Otherwise fall through to the normal fresh-registration UX.

Everything is user-signed, so the opt-in write constraint is satisfied. There is no admin path.

## Part A — Contract (`dapp/projects/dapp/smart_contracts/bingle_dapp/contract.py`)

- Replace `self.predecessor_app = GlobalState(UInt64, key="PredecessorApp")` with a packed
  ancestor list held in a single global `Bytes` value (key `AncestorApps`): concatenated
  8-byte big-endian app ids. ~15 ids fit within the 128-byte global value limit — ample for an
  interim solution; the bound is documented in the contract.
- `set_predecessor_app(predecessor: Application)` (creator-only): keep the simple "point at the
  immediate predecessor" API, but **accumulate** the lineage — store `predecessor.id` plus the
  predecessor's own `AncestorApps` value (read via
  `op.AppGlobal.get_ex_bytes(predecessor, b"AncestorApps")`), de-duplicated. Mirrors the
  existing cross-app read pattern in `migrate_global`.
- `migrate_local(old_app: Application)`: change the guard from `old_app.id == predecessor_id`
  to **membership** — walk `AncestorApps` in 8-byte chunks (`urange(length // 8)`) and assert
  `old_app.id` is present. The per-field copy logic is unchanged: first-write-wins on handle;
  `allow_static`/`allow_relay` copied as-is; `static_endpoint*` copied only when
  `allow_static == 1`. This preserves the security property — only creator-blessed ancestors
  are accepted, so a user cannot supply a fake app they control to forge admin-granted
  permissions.
- Rebuild artifacts (`.teal` + `bingle_dapp_client.py`) under
  `smart_contracts/artifacts/bingle_dapp/`.

## Part B — Client auto-migration on start (`src/blockchain/algo_bingle.rs`)

New helpers, reusing existing primitives:

- `ancestor_apps(new_app_id) -> Vec<u64>`: read the new app's lineage by decoding the
  `AncestorApps` global via `AlgoOps::global_state` (algo_ops.rs:452).
- `find_migratable_ancestor(new_app_id, my_addr) -> Option<u64>`: for each ancestor (nearest
  first) call `AlgoOps::local_state_for_account(ancestor, my_addr)` (algo_ops.rs:485) and
  return the first that returns `Some` (the account has data there).
- `ensure_local_migrated(new_app_id, asset_id) -> Result<Option<String>>`:
  1. If `local_state_for_account(new_app_id, me)` already shows a `Handle`, return `Ok(None)`
     (already migrated/registered — idempotency gate).
  2. Else find a migratable ancestor; if none, return `Ok(None)` (fresh install).
  3. Else `AlgoOps::opt_in_app(new_app_id)` (already idempotent, algo_ops.rs:1325), then call
     the existing `migrate_local(new_app_id, ancestor)` wrapper (algo_bingle.rs:324). Return
     the txid.

Wiring: call `ensure_local_migrated` on the start/activation path, before the normal
"register a handle" UX, so an upgrading user is migrated instead of prompted. The natural call
site is the flow that already performs `opt_in_app` during registration (`register_unchecked`,
algo_bingle.rs:1017); the check is factored out so start can invoke it. The exact call site
(`BingleApiImpl` start vs the local/JSI activation path) is confirmed during implementation by
tracing from `App.tsx` "Checking activation status" → JSI start → Rust.

## Part C — Rust wrapper / AlgoOps

- `migrate_local(app_id, old_app_id)` (algo_bingle.rs:324) is unchanged (still
  `migrate_local(uint64)void`, old app passed as arg + foreign app).
- No new foreign-accounts call builder is needed (the admin/4-accounts design was dropped).
- `set_predecessor_app` wrapper (algo_bingle.rs:236): signature unchanged; doc updated to note
  lineage accumulation.

## Verification

- **Contract** (`test_bingle_dapp.py`, algokit localnet): deploy v1 → v2 → v3 with accumulating
  lineage; assert a user with data only on v1 can `migrate_local(v1)` against v3, and that a
  non-blessed app id is rejected.
- **Rust** (localnet, gated integration test under `tests/blockchain/`): register an account on
  an old app, deploy a new app pointing at it, run `ensure_local_migrated`, assert the new
  app's local state matches; plus the fresh-install case (no ancestor data → no migration, no
  error) and the idempotency case (second call is a no-op).
- Run the `unit` target and confirm `tests`, `bingle_jsi`, `bingle_local`, `bingle_webserver`
  compile.

## Notes / limitations

- Lineage size is bounded by the 128-byte global value (~15 ancestors); adequate for an interim
  solution.
- Migration does **not** clear the old app's local state (an app cannot delete another
  account's local state on a different app). Stale old-app data remains until that account
  clears it or the old app is deleted.
- Only accounts that run the new app are migrated; this is inherent to Algorand's local-state
  opt-in rule and is why the admin batch approach was dropped.
