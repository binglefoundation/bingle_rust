# Reserved State Schema Slots

Status: implemented. Contract: `dapp_projects/smart_contracts/bingle_dapp/contract.py`.

## Context

An Algorand application's **state schema** — the counts of global and local `uint` and
`byte-slice` slots — is fixed at **creation** and is immutable thereafter. `UpdateApplication`
can replace the approval/clear programs of an existing app **in place** (keeping the same app id
and all existing state), but it **cannot grow the schema**. Deploying a contract version that
declares more state than the live app therefore triggers a *schema break*: the deploy tooling
(`factory.deploy(..., on_schema_break=AppendApp)`) is forced to create a **brand-new app** with a
new app id, which in turn requires every user to re-opt-in and run the local-state migration
described in [migrate_local_storage.md](migrate_local_storage.md).

To keep future field additions cheap — an in-place update rather than a full app replacement — we
**pre-claim spare schema capacity** on the current app.

## What is reserved

Declared in `contract.py.__init__` as dummy state proxies:

| Scope  | uint slots | byte-slice slots |
|--------|-----------|------------------|
| global | 4 (`rsvd_g_i0..3`) | 4 (`rsvd_g_b0..3`) |
| local  | 2 (`rsvd_l_i0..1`) | 2 (`rsvd_l_b0..1`) |

Resulting compiled schema (see `BingleDapp.arc56.json`):

| Scope  | uint | byte-slice | total | protocol limit |
|--------|------|-----------|-------|----------------|
| global | 6    | 8         | 14    | 64 |
| local  | 5    | 5         | 10    | 16 |

## Mechanism — the contract is the source of truth

The reserved slots are ordinary `GlobalState`/`LocalState` proxies that are **never read or
written**. puyapy counts declared state in the compiled schema regardless of whether it is
accessed (verified empirically against puyapy 5.4.0 at the default optimization level), so simply
declaring the proxies is sufficient to reserve the slots. No dummy writes, `create`-time
initialisation, or opt-in touching is required.

Because the schema is derived from the contract:

- The compiled `BingleDapp.arc56.json` reports the reserved counts, and `factory.deploy` creates
  the app with that schema — nothing hardcodes the counts at deploy time.
- The Rust client sizes opt-in funding from the **live on-chain schema**
  (`AlgoOps::get_app_local_schema` → the MBR constants in `algo_bingle.rs`), so per-user funding
  automatically accounts for the reserved local slots with no client change.

This makes the contract the single source of truth for the reserved capacity.

## Cost

Reserved slots raise the minimum balance requirement (MBR) — locked, recoverable reserve, not a
spent fee — for as long as they exist. Per Algorand mainnet MBR (uint = 28,500 µAlgo, byte-slice =
50,000 µAlgo):

- **App owner (creator)** — global schema, charged once to the creator account:
  `4 × 0.0285 + 4 × 0.05 = 0.314 ALGO`.
- **Each opted-in user** — local schema, charged to every account that opts in:
  `2 × 0.0285 + 2 × 0.05 = 0.157 ALGO`.

## Repurposing a reserved slot in a future version

1. In the new contract version, replace a `rsvd_*` proxy with the real field, **keeping the same
   slot type** (`uint` stays `uint`, byte-slice stays byte-slice) so the compiled schema counts are
   unchanged. The state key may change; only the per-scope `(uint, byte-slice)` counts must match.
2. Rebuild (`python -m smart_contracts build bingle_dapp`) and confirm the `arc56` schema counts
   are identical to the live app's, so `factory.deploy` performs an in-place `UpdateApplication`
   rather than an `AppendApp` schema break.
3. Populate the field in whatever method owns it; existing users already carry the MBR for the
   slot, so no additional funding step is needed.

Do **not** change a reserved slot's type or exceed the reserved counts — either reintroduces a
schema break and the full app-replacement migration this reservation exists to avoid.
