# Bingle

Bingle is a peer-to-peer messaging project. Identities are **handles registered on the Algorand
blockchain**, and messages travel **directly between peers** over an encrypted (DTLS) transport,
using STUN for NAT traversal and relays only when a direct path is not available. This repository
is the Rust core: the comms engine, the Algorand integration, a command-line client, a relay/web
server, and the mobile bindings.

This guide is for people with app-development and build skills who want to explore what Bingle can
do. Pick the path that matches your goal.

## Which path is for you?

- **Run a relay node, integrate Bingle messaging into your own app, or explore Bingle from the
  desktop.** Install the CLI and libraries from crates.io — no clone needed. Start with the
  [Quick start](#quick-start) below.
- **Just try the Bingle app as an end user, or build your own iOS / React Native app on top of
  Bingle.** Head to the app project instead: **[bingle_ux](https://github.com/bingle-foundation/bingle_ux)**.
- **Dig into the internals and contribute.** Clone and build the repository — see
  [Going deeper](#going-deeper-development) and the [Developer Guide](DEVELOPER.md).

## Quick start

The quickest way to see Bingle work is the **Bingle CLI**: register a handle on Algorand and then
run a client that listens for and sends messages. The CLI targets the Bingle deployment on
**Algorand mainnet** by default. (Developing against testnet or a local Algorand network is
covered in the [Developer Guide](DEVELOPER.md).)

### Prerequisites

- **Rust** (stable, 2024 edition — Rust 1.85 or newer) via [rustup](https://rustup.rs).
- An **Algorand account with a little ALGO** — see
  [Set up a wallet and add ALGO](#set-up-a-wallet-and-add-algo). You will use its 25-word
  passphrase with the CLI.

### Install the CLI

Install the `bingle_cli` binary from [crates.io](https://crates.io/):

```bash
cargo install bingle_core
```

This puts the `bingle_cli` command on your `PATH`. Alternatively, build it from a clone of this
repository:

```bash
git clone https://github.com/bingle-foundation/bingle_rust.git
cd bingle_rust
cargo install --path bingle_core --bin bingle_cli
```

### Set up a wallet and add ALGO

Your Bingle identity is an Algorand account, so you need a wallet holding a little ALGO.

> **Free as in beer (BYO glassware).** For a limited time Bingle itself is free — there is no
> per-message or subscription charge. You only ever cover Algorand's on-chain fees, so a tiny ALGO
> balance (well under 1 USD) is all you need to register and use a handle.

The simplest option is the [Pera Wallet](https://perawallet.app/):

1. Install Pera (iOS, Android, or web) and **create a new account**.
2. Copy your **account address** — the long string beginning with an uppercase letter, e.g.
   `JWPYTCFOAS23MXVV…`.
3. Add a few ALGO (under 1 USD is plenty):
   - **buy with fiat** on a KYC exchange that lists ALGO — e.g. [Coinbase](https://www.coinbase.com/),
     [Kraken](https://www.kraken.com/) or [Binance](https://www.binance.com/) — and **withdraw to
     your address**, or
   - **swap crypto you already hold** on a decentralized exchange — e.g. the Algorand DEXs
     [Tinyman](https://tinyman.org/) and [Pact](https://www.pact.fi/) (aggregated by
     [Vestige](https://vestige.fi/)) — into ALGO in your account, or
   - **swap crypto with no account** via an instant-exchange service such as
     [SimpleSwap](https://simpleswap.io/) — send a coin you already hold and receive ALGO at your
     address.
4. In Pera, reveal your **25-word passphrase** (Settings → the account → *Show passphrase* /
   recovery phrase). This is the value you pass to the CLI as `--passphrase` below.

> **We don't endorse any of these services.** The exchanges and swap providers above are listed
> only as examples, not recommendations. Do your own due diligence before using any of them, and
> when trying one for the first time, move only a small amount.

> **Keep the passphrase secret.** Anyone with the 25 words controls the account and its funds. For
> exploring Bingle, prefer a fresh account you use only for this.

### Register a handle

```bash
bingle_cli register \
  --handle alice \
  --passphrase "word1 word2 ... word25" \
  --price-units 1
```

This checks the account is funded, then registers `alice` to your account on-chain. If the handle
is already taken it fails fast without spending, so just pick another.

### Run a client

Once registered, start a client that connects to the Bingle network and can send and receive
messages for your handle:

```bash
bingle_cli run \
  --handle alice \
  --passphrase "word1 word2 ... word25"
```

Run `bingle_cli` with no arguments to see the full usage for every command (`run`, `register`,
`buybingle`, `sellbingle`, `checkrelays`).

### Running a relay

Relays help peers connect when a direct peer-to-peer path is blocked by NAT or firewalls:

```bash
bingle_cli run --handle my-relay --passphrase "word1 ... word25" --relay --static-ip <ip:port>
```

**Relay nodes are currently permissioned on mainnet.** While the network is stabilising, only
approved accounts are accepted as relays — for security and stability reasons — and this
restriction is expected to be lifted as Bingle matures. Relay providers earn tangible rewards in
**Bingle$** assets, which can be converted to ALGO.

To have your relay admitted to the network, contact **[tbd — relay-admissions email]**.

## Going deeper (development)

To explore Bingle at a deeper level — modifying the engine, the Algorand integration, or the
mobile bindings — clone and build the repository:

```bash
git clone https://github.com/bingle-foundation/bingle_rust.git
cd bingle_rust
cargo build
cargo test --test unit      # unit tests, no external services required
```

Prerequisites are just the Rust toolchain; localnet integration tests additionally need
[AlgoKit](https://github.com/algorandfoundation/algokit-cli) (`algokit localnet start`). The full
build/test workflow, Docker images, testnet/localnet integration runs, relay deployment to AWS,
and the mobile (JSI) bridge details are all in the **[Developer Guide](DEVELOPER.md)**.

Before publishing, `scripts/scan_native_leaks.sh` checks that no build-machine paths leak into
shipped native libraries or the cargo/npm packages — see
[Release hygiene](DEVELOPER.md#release-hygiene-scanning-for-leaked-build-paths) in the Developer
Guide.

## What's in this repository

Bingle is a Cargo workspace of five crates:

| Crate | Purpose |
|---|---|
| **`bingle_core`** | The heart of Bingle: the peer-to-peer comms engine (STUN NAT traversal, DTLS transport, relay discovery and routing) and the Algorand integration (`AlgoOps` generic helpers, `AlgoBingle` app/asset operations such as handle registration and lookup). Also builds the `bingle_cli` binary used in the quick start. |
| **`bingle_local`** | A thin local-state layer over `bingle_core`: keypair status, the message queue, and the contact store. Kept deliberately small so it can be re-implemented natively (iOS/Android) in future. |
| **`bingle_jsi`** | The React Native JSI bridge. Generates iOS/Android bindings from the Rust API via [uniffi](https://mozilla.github.io/uniffi-rs/) so apps like [bingle_ux](https://github.com/bingle-foundation/bingle_ux) can call Bingle. |
| **`bingle_webserver`** | An HTTP server exposing the Bingle API — used to run relays and to drive Bingle from non-Rust environments. |
| **`bingle_test`** | Shared test fixtures and helpers used across the other crates' test suites. |

## API documentation

Generate the Rust API docs locally with `cargo doc --open`. Hosted references (stubs — to be
published):

- `bingle_core` — _[API docs — coming soon]_
- `bingle_local` — _[API docs — coming soon]_
- `bingle_jsi` — _[API docs — coming soon]_

## License

See the repository for license details.
