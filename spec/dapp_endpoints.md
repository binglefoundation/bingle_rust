# DApp Endpoints

The application account holds the `hot` balance, which can be withdrawn using the `withdraw` endpoint.
The rserver value is held by the ASSET_RESERVE account.

See [dapp_rights.md](dapp_rights.md) for per-method signer, auth, foreign refs, transaction group, and state requirements.

## Asset accounts

| Account        | Description                                |
|----------------|--------------------------------------------|
| ASSET_CREATOR  | Creates the asset                          |
| ASSET_MANAGER  | Standard asset manager access              |
| ASSET_RESERVE  | Asset reserve balance, holds TOTAL - `hot` |
| ASSET_CLAWBACK | Asset clawback access (not used)           |
| ASSET_FREEZE   | Asset freeze access (not used)             |


## DApp lifecycle (left-side pins — bare methods)

| Endpoint | ARC-4 action | Caller |
|---|---|---|
| Create | CreateApplication | APP_CREATOR |
| Update | UpdateApplication | APP_CREATOR |
| Delete | DeleteApplication | APP_CREATOR |
| _(opt-in)_ | OptIn (bare) | USER |

## DApp endpoints (right-side bracket)

| Endpoint | Method | Caller         | Notes                                                                                                      |
|---|---|----------------|------------------------------------------------------------------------------------------------------------|
| `opt_in_to_bingle` | `@abimethod` | APP_CREATOR    | Used after app creation.                                                                                   |
| `buy_bingle` | `@abimethod` | USER           | Requires payment of `BinglePrice` µAlgo in group; inner clawback of 1 Bingle$ to buyer.                    |
| `sell_bingle` | `@abimethod` | USER           | Requires axfer of `amount` Bingle$ to app + payment of `price × amount` to seller in group.                |
| `withdraw` | `@abimethod` | APP_WITHDRAWER | Withdraws Algo (capped to balance − min_balance) and/or Bingle$ from the app account to a target address via inner txns. |
| `register` | `@abimethod` | USER           | Requires opt-in to app + 1 Bingle$ axfer to app in group. Stores handle in local state (first write wins). |

## Admin / permission endpoints

| Endpoint | Method | Caller      | Notes |
|---|---|-------------|---|
| `set_allow_relay` | `@abimethod` | APP_ADMIN   | Sets `allow_relay` local state flag on a target account. |
| `set_allow_static` | `@abimethod` | APP_ADMIN   | Sets `allow_static` local state flag on a target account. Clearing it also deletes `static_endpoint` / `static_endpoint_x`. |
| `register_endpoint` | `@abimethod` | USER_STATIC | Caller must have `allow_static == 1`. Stores endpoint string split across `static_endpoint` (≤64 bytes) and `static_endpoint_x` (overflow). Passing `""` clears both. |
| `set_bingle_price` | `@abimethod` | APP_ADMIN   | Sets global `BinglePrice`. Admin operation                                                                 |

## Migration endpoints

These move users and funds from an older dapp deployment to a newer one. `set_predecessor_app`
records an **accepted-ancestor lineage** in the global `AncestorApps` value (a packed list of
8-byte app ids): the immediate predecessor, the predecessor's own accumulated lineage, and — to
bridge apps deployed with the older single-predecessor contract — the predecessor's legacy
`PredecessorApp` pointer. `migrate_local` copies a caller's local state from **any** app in that
lineage, so a user several versions behind migrates directly in one hop. Because only the creator
extends the lineage, a user cannot supply a fake app they control to forge admin-granted
permissions.

| Endpoint | Method | Caller | Notes |
|---|---|---|---|
| `set_predecessor_app` | `@abimethod` | APP_CREATOR | Blesses `predecessor` (and its accumulated lineage) as migration sources; updates global `AncestorApps`. |
| `migrate_local` | `@abimethod` | USER | Copies the caller's local state (`Handle`/`HandleTime`, `allow_relay`, `allow_static`, and `static_endpoint`/`static_endpoint_x` when `allow_static == 1`) from a blessed ancestor app into this app. Handle is first-write-wins; requires the caller opted in to this app. |
| `migrate_global` | `@abimethod` | APP_CREATOR | Copies global state (`BinglePrice`, `LastHandleTime`, `AppAdmin`, `AppWithdrawer`) from an old app. |
| `migrate_reserve` | `@abimethod` | APP_CREATOR | Transfers the app account's spendable Algo and Bingle$ ASA holdings to a new app. |

## Superseding an app

When a newer dapp replaces this one, the creator marks this app **superseded** by calling
`set_successor_app` **on the app being retired**, passing the replacement as the successor (an
app can only write its own global state, so this is the forward-pointing mirror of
`set_predecessor_app`). This records the successor's id in the global `SuccessorApp` value
(8-byte big-endian; empty means not superseded).

Once superseded, the app **hard-rejects** its user-facing state-changing methods — `register`,
`buy_bingle`, `sell_bingle`, and `register_endpoint` all assert the app is not superseded and
fail. Admin/creator methods (`set_*`, `withdraw`, `migrate_global`, `migrate_reserve`), the
bare `OptIn`, and `migrate_local` stay callable, so the retired app can still be wound down and
users can still migrate their local state out of it. Clients read `SuccessorApp` on start and,
when it is set, report `UPGRADE_REQUIRED` and prompt the user to update rather than transacting
against the dead app.

| Endpoint | Method | Caller | Notes |
|---|---|---|---|
| `set_successor_app` | `@abimethod` | APP_CREATOR | Marks this app superseded by `successor`; records global `SuccessorApp`. Re-pointable. `successor` must be in the transaction's foreign apps. |
