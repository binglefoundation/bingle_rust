# DApp Method Rights & Requirements

Per-method requirements for the Bingle DApp: authorized signer, on-chain auth
check, foreign references, transaction group the client assembles, and state
read/written.

Sources:
- Contract: `dapp/projects/dapp/smart_contracts/bingle_dapp/contract.py`
- Rust client: `src/blockchain/algo_bingle.rs`, `src/blockchain/algo_ops.rs`

## Roles

Signing accounts collapse to three roles. Only the **creator** (the deploying
key, `Global.creator_address`) can update/delete, set roles, set the
predecessor, and run global/reserve migration. **AppAdmin** and **AppWithdrawer**
are held in global state and are reassignable by the creator.

| Role | Held in | Set by |
|---|---|---|
| Creator | `Global.creator_address` (chain) | Deployment |
| AppAdmin | Global `AppAdmin` | `create`, `set_app_admin`, `migrate_global` |
| AppWithdrawer | Global `AppWithdrawer` | `create`, `set_app_withdrawer`, `migrate_global` |

## Global state keys

`BinglePrice`, `LastHandleTime`, `AppAdmin`, `AppWithdrawer`, `PredecessorApp`.

## Local state keys (per opted-in account)

`Handle`, `HandleTime`, `allow_static`, `allow_relay`, `static_endpoint`,
`static_endpoint_x`.

## Client-side conventions (confirmed in `algo_ops::build_call_app_tx_inner`)

- Every app call automatically includes the **app creator** in `accounts[0]`.
- `set_allow_static` / `set_allow_relay` additionally insert the **target**
  address into `accounts[0]`.
- Fees are auto-estimated so the outer transaction covers any inner transactions.

---

## Lifecycle / bare methods

| Method | Signer | On-chain auth | Notes | State written |
|---|---|---|---|---|
| `create(app_admin, app_withdrawer)` | Deployer → creator | `create="require"` | ARC-4 create | `AppAdmin`, `AppWithdrawer` |
| `update_application` (bare) | Creator | `sender == Global.creator_address` | UpdateApplication | — |
| `delete_application` (bare) | Creator | `sender == Global.creator_address` | DeleteApplication | — |
| `optin` (bare) | Any user | none | OptIn; allocates the caller's local state | — |

## Trading methods

| Method | Signer | On-chain auth | Foreign refs | Group the client builds | State read | Inner txns |
|---|---|---|---|---|---|---|
| `opt_in_to_bingle(asset_id)` | **AppAdmin** | `sender == AppAdmin` | asset `asset_id` | single app call | `AppAdmin` | axfer 0 → app (opts app into ASA; app needs MBR) |
| `buy_bingle()` | User (buyer) | none (open) | `assets[0]` = Bingle$ ASA | client opts sender into ASA, then group **[Pay sender→app = price] + [app call]** | `BinglePrice` | clawback axfer 1 unit `asset_sender=app` → buyer (**requires ASA clawback addr = app**) |
| `sell_bingle(amount)` | User (seller) | none | `assets[0]` = Bingle$ ASA | client opts app into ASA, then group **[axfer amount sender→app] + [Pay payout — built as self-payment sender→sender] + [app call]** | `BinglePrice` | none (validation only) |
| `withdraw(address, amount, asset_id, asset_amount)` | **AppWithdrawer** | `sender == AppWithdrawer` | asset `asset_id` (only when `asset_amount>0 && asset_id>0`) | single app call | `AppWithdrawer`; app balance & min_balance | Payment (capped to withdrawable) and/or asset transfer from app → `address` |

Notes:
- `sell_bingle`: the contract only checks that *some* group txn pays the seller
  `price × amount`; the client satisfies this with a self-payment, so no
  external funder is needed. Any account *may* fund it per the contract.
- The reserve (`TOTAL − hot`, per `dapp_endpoints.md`) is held by
  `ASSET_RESERVE`; `buy_bingle`'s clawback pulls from the app-held balance.

## Registration / endpoint methods

| Method | Signer | On-chain auth / precondition | Foreign refs | Group the client builds | State read | State written |
|---|---|---|---|---|---|---|
| `register(handle)` | User | Must be **opted-in to app** | `assets[0]` = Bingle$ ASA | client opts app into ASA, opts sender into ASA, opts sender into app, then group **[axfer 1 unit sender→app] + [app call]** | `LastHandleTime` | `Handle`, `HandleTime` (local, **first-write-wins**); bumps `LastHandleTime` |
| `register_endpoint(endpoint)` | User | Local `allow_static == 1` (must be opted-in) | — | single app call | `allow_static` (local) | `static_endpoint` + `static_endpoint_x` (split at 64 bytes); empty string deletes both |

Notes:
- `register` (client) also runs an off-chain indexer uniqueness pre-check and a
  post-submit winner verification; the on-chain rule is oldest-`HandleTime`-wins.

## Admin / permission methods

| Method | Signer | On-chain auth | Foreign refs | Target requirement | State written |
|---|---|---|---|---|---|
| `set_bingle_price(price)` | **AppAdmin** | `sender == AppAdmin` | — | — | `BinglePrice` |
| `set_allow_static(target, allow)` | **AppAdmin** | `sender == AppAdmin` | `target` in `accounts[0]` | target **opted-in to app** | `allow_static[target]`; disabling also deletes target's `static_endpoint`/`_x` |
| `set_allow_relay(target, allow)` | **AppAdmin** | `sender == AppAdmin` | `target` in `accounts[0]` | target opted-in | `allow_relay[target]` |
| `set_app_admin(admin)` | **Creator** | `sender == Global.creator_address` | — | — | `AppAdmin` |
| `set_app_withdrawer(withdrawer)` | **Creator** | `sender == Global.creator_address` | — | — | `AppWithdrawer` |
| `set_predecessor_app(predecessor)` | **Creator** | `sender == Global.creator_address` | app `predecessor` (foreign app) | — | `PredecessorApp` |

## Migration methods

| Method | Signer | On-chain auth | Foreign refs | Preconditions | Effect |
|---|---|---|---|---|---|
| `migrate_global(old_app)` | **Creator** | `sender == Global.creator_address` | app `old_app` | — | reads old app's `BinglePrice`, `LastHandleTime`, `AppAdmin`, `AppWithdrawer` → writes same globals |
| `migrate_reserve(new_app, asset_id)` | **Creator** | `sender == Global.creator_address` | app `new_app`; asset `asset_id` (skipped if 0) | — | Payment + axfer of full reserve (balance − min_balance − fee slots, and ASA holding) → `new_app.address` |
| `migrate_local(old_app)` | User | none, but `old_app.id == PredecessorApp` (anti-forgery) | app `old_app` | caller **opted-in to both apps**; `PredecessorApp` must be set | copies caller's local `Handle`/`HandleTime`/`allow_static`/`allow_relay`/`static_endpoint(_x)`; handle first-write-wins; endpoints copied only when `allow_static == 1` |

## Cross-cutting requirements

- **Opt-in dependency:** any method that writes *local* state (`register`,
  `register_endpoint`, `set_allow_*` on a target, `migrate_local`) requires the
  relevant account to have opted into the app first (bare `optin`).
- **`assets[0]` foreign-asset ref** is mandatory for `buy_bingle`,
  `sell_bingle`, and `register` (identifies the Bingle$ ASA).
- **Group-validation pattern:** `buy_bingle`, `sell_bingle`, and `register` take
  no payment args — the contract scans the atomic group
  (`urange(Global.group_size)`) for the required payment/axfer, so the client
  must bundle the sibling transactions (confirmed above).

## See also

- [dapp_endpoints.md](dapp_endpoints.md) — endpoint overview and asset accounts.
