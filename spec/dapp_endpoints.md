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
