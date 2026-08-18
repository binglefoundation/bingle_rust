# AlgoOps API surface — rationalization

Status: design. Tracks issue #158, step 1. Source: `bingle_core/src/blockchain/algo_ops.rs`.

## Goal

Extract `AlgoOps` into an **independent, reusable crate** so it can be shared by `bingle_rust`
and sidewinder instead of being duplicated. The intended shape:

- a `blockchain_ops` crate holding a `BlockChainOps` trait, and
- an `algo_ops` crate implementing that trait for Algorand,
- both living in a new `blockchain_ops_rust` repo.

**The target is not a universal abstraction.** It is a crate consumed by an application that
*knows which blockchain it uses*. That reframing (from PR review) has three consequences that
shape the categorization below:

- The trait can — and should — expose a **public escape hatch** to the underlying native
  handles (the `algonaut` clients, or the equivalent on another chain). Callers that need
  chain-specific power reach through it deliberately rather than being blocked by the
  abstraction.
- The next layer up is **`AlgoBingle`** (`bingle_core/src/blockchain/algo_bingle.rs`), which
  wraps an `AlgoOps` (`pub ops: AlgoOps`) and drives the Bingle Algorand dApp and asset through
  it — calling `call_app`, `get_app_local_schema`, `arc4_selector`, `unique_note`, etc. Those
  are therefore **public API consumed across the crate boundary**, not internal plumbing.
- "Generic" here means "would read naturally against another chain (e.g. Ethereum)", not "hides
  all chain detail". Assets, for example, are generic to *any chain that has assets* — a
  capability tier, not an Algorand-only concept.

This document is **step 1**: rationalize the current API surface so the trait boundary can be
drawn deliberately. No behavior changes are proposed here — this is the map, not the move.

## Categories

1. **Generic blockchain** — belongs on `BlockChainOps`; reads naturally on any chain.
2. **Generic-with-assets / Algorand-specific parameters** — universal *shape* but tied to
   assets (a capability of chains that have them) or carrying an Algorand unit/identifier.
3. **Algorand specific** — TEAL apps, ARC-4/56, ASA configuration; lives on the Algorand impl
   and is used directly by the app layer (`AlgoBingle`).
4. **Internal (should be private)** — plumbing behind the escape hatch.

### 1. Generic blockchain (→ `BlockChainOps`)

| Item | Notes / proposed generic form |
|------|-------------------------------|
| `sign(&self, text) -> Result<String>` | ed25519 signature over arbitrary text. |
| `verify(&self, text, sig_b64) -> Result<bool>` | ed25519 verification. |
| `address_str(&self) -> Result<String>` | The account's own address. |
| `public_key_bytes(&self) -> Result<[u8; 32]>` | Account public key. |
| `generate_keypair() -> (String, String)` | An address + private-key mnemonic — describable for **any** blockchain, not Algorand-only. Belongs on the generic trait. |
| `create_address(&mut self)` | Creates a generic address on the interface object. Drop the unused `save`/`always_new_address` params (see Observations). |
| `send_algo` → **`send_payment(&self, to, payment_amount)`** | Native-token payment; rename off `algo`, take a generic `payment_amount`. |
| `account_balance(&self) -> Result<Option<f64>>` | Balance of the account, as `f64` in units of the chain's default asset. |
| `microalgos_at` → **`account_balance_at(&self, account) -> Result<f64>`** | Balance of an arbitrary address; return `f64` in default-asset units (not µALGO `u64`). |
| `wait_for_confirmation(&self, tx_id, timeout)` | Universal; `timeout_rounds` is the only Algorand-flavoured parameter — express as a generic timeout. |

### 2. Generic-with-assets / Algorand-specific parameters

Asset transfer and holding generalize to any chain that has assets (Algorand ASA, Ethereum
tokens), so they form an **asset capability tier** rather than sitting in either extreme. The
`asset_id: u64` is the chain-specific identifier a caller supplies.

| Item | Note |
|------|------|
| `send_asset(&self, asset_id, amount, to)` | Applies to any blockchain with assets. |
| `asset_holding(&self, account, asset_id) -> Result<u64>` | Asset balance; same capability tier. |
| `private_key_bytes(&self) -> Result<Vec<u8>>` | Stays **public** — we need it externally to export the private key. Generic (private key of the account). |
| `new(passphrase, address, config)` / `new_indexer(config)` | Replace with a **factory method on the Algorand-specific trait**; reduce `new`'s visibility (constructed via the factory, not called directly by consumers). |

### 3. Algorand specific

TEAL/app lifecycle, ASA configuration, opt-in, ARC-4/56, contract addressing. These live on
the Algorand impl and are used directly by `AlgoBingle` (so they are public, not internal):

- `contract_address(app_id)`
- `global_state(maybe_app_id)`
- `app_global_bytes(app_id, key)`
- `get_app_local_schema(app_id)` — used by `AlgoBingle`
- `local_state_for_account(app_id, account)`
- `create_asset(name, units)`
- `create_asset_configured(name, units, manager, reserve, clawback, freeze)`
- `create_asset_with_reserve_app(name, units, app_id)`
- `is_account_opted_in_to_asset(account, asset_id)`
- `opt_in_to_asset(asset_id)`
- `set_asset_clawback_to_app(app_id, asset_id)`
- `change_asset_reserve_address(asset_id, reserve)`
- `recover_reserve_balance(asset_id)`
- `compile_teal(source)`
- `deploy_app(...)`, `update_app(...)`, `delete_app(app_id)`
- `call_app(...)` — used by `AlgoBingle`; `call_app_with_foreign_app(...)`
- `opt_in_app(app_id)`, `clear_state_app(app_id)`, `close_out_app(app_id)`
- `opt_in_app_to_asset(app_id, asset_id, opt_in_method_name)`
- `arc4_selector(sig)`, `unique_note()` — used by `AlgoBingle`; Algorand-specific but public helpers, **not** internal
- `build_call_app_tx(...)`, `build_call_app_tx_with_foreign_apps(...)` — return `algonaut` transactions; part of the escape-hatch surface (see below)
- Free functions `address_to_byte_key`, `byte_key_to_address` (Algorand base32 + SHA512/256 checksum)
- `AppArg` enum + `AppArg::to_bytes` (ARC-4 argument encoding)
- `AlgoChainConfig`, `KeyProvider` trait

### 4. Native escape hatch (public, chain-aware callers)

Rather than sealing these, expose them as the deliberate escape hatch to native power:

| Item | Role |
|------|------|
| `algod_client()`, `indexer_client()` | Public accessors for the underlying `algonaut` handles — the escape hatch a chain-aware app uses when the generic surface is insufficient. |
| `build_call_app_tx*`, and the raw `algonaut` transaction/error types | Reachable through the escape hatch for callers that assemble their own transactions (as `AlgoBingle` does). |

**Open design question (from review):** can the escape hatch return an *opaque* handle that a
caller can *widen* to the concrete `algonaut` type on demand — so `blockchain_ops` need not
depend on `algonaut`, while `algo_ops` consumers can still downcast? To resolve when drafting
the trait.

### 5. Internal (genuinely private)

Already private and should stay so: `rt_block_on`, `parse_address`, `require_address`,
`json_get`, `decode_state_entries`, `estimate_fee_for_programs`, `estimate_fee_for_signed_size`,
`state_schema_from_arc56`, `build_call_app_tx_inner`.

Candidates to tighten to `pub(crate)` (pure JSON parsers, likely `pub` only for tests):
`parse_creator_reserve_from_asset_info_value`, `parse_holding_amount_from_account_value`. Also
the retry/transport internals `algod_call`, `is_retryable`, `RETRYABLE_STATUS_CODES`.

## Observations for the redesign

1. **Trait tiers, not one flat trait.** A core `BlockChainOps` (bucket 1) plus an **asset
   capability** (bucket 2) that only chains with assets implement; the Algorand impl adds
   bucket 3 on top. Validate the core against a hypothetical Ethereum impl: `send_payment` and
   `account_balance_at` read naturally; TEAL/ARC-56 do not and stay Algorand-only.
2. **Escape hatch over sealing.** Because the consumer is chain-aware, `algod_client()` /
   `indexer_client()` should remain a public escape hatch (possibly via an opaque,
   widen-on-demand handle) rather than being hidden.
3. **Single balance type.** Standardize on `f64` in units of the default asset for
   `account_balance`/`account_balance_at`, dropping the `u64` µALGO form from the public
   surface. A decimal type is the alternative but would need to span ~12 places either side of
   the point to hold both large balances and fine fractions.
4. **Drop dead parameters.** `create_address`'s `save`/`always_new_address` are unused; remove
   them (repo convention forbids `_`-prefixed in-use params).
5. **Construction via factory.** Replace direct `new`/`new_indexer` calls with a factory on the
   Algorand-specific trait and lower `new`'s visibility.
6. **Keep `private_key_bytes` public** — exporting the private key is a required capability.

## Next steps (issue #158)

- Draft `BlockChainOps` (core) + an asset-capability trait, deciding the balance/amount types
  and the escape-hatch handle shape; validate against a hypothetical Ethereum impl.
- Define the factory that replaces `new`; lower `new` visibility.
- Tighten only the genuinely-internal items (bucket 5); leave the escape hatch and the
  `AlgoBingle`-consumed helpers public.
- Plan the crate split: `blockchain_ops` (trait) + `algo_ops` (Algorand impl) in a new
  `blockchain_ops_rust` repo, consumed by `bingle_rust` (`AlgoBingle`) and sidewinder.
