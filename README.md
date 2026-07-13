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
  desktop.** Clone and build this repository — start with the [Quick start](#quick-start) below.
- **Just try the Bingle app as an end user, or build your own iOS / React Native app on top of
  Bingle.** Head to the app project instead: **[bingle_ux](https://github.com/bingle-foundation/bingle_ux)**.
- **Dig into the internals and contribute.** See [Going deeper](#going-deeper-development) and the
  [Developer Guide](DEVELOPER.md).

## Quick start

The quickest way to see Bingle work is the **Bingle CLI**: register a handle on Algorand and then
run a client that listens for and sends messages. The CLI targets the Bingle deployment on
**Algorand mainnet** by default. (Developing against testnet or a local Algorand network is
covered in the [Developer Guide](DEVELOPER.md).)

### Prerequisites

- **Rust** (stable, 2024 edition — Rust 1.85 or newer) via [rustup](https://rustup.rs).
- An **Algorand account** — a standard 25-word mnemonic. You can create one with any Algorand
  wallet, e.g. [Pera](https://perawallet.app/) or [Defly](https://defly.app/).
- A small amount of **ALGO** in that account to cover registration (see
  [Acquiring ALGO](#acquiring-algo)).

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

### Acquiring ALGO

Registering a handle is an on-chain action, so the account needs a small amount of ALGO (roughly
0.6 ALGO covers the minimum balance plus fees). You can obtain ALGO by:

- **Buying with fiat** on a KYC exchange that lists ALGO — for example
  [Coinbase](https://www.coinbase.com/), [Kraken](https://www.kraken.com/), or
  [Binance](https://www.binance.com/) — then withdrawing to your account's address.
- **Swapping other crypto** on a decentralized exchange — for example the Algorand DEXs
  [Tinyman](https://tinyman.org/) and [Pact](https://www.pact.fi/), or an aggregator such as
  [Vestige](https://vestige.fi/) — and holding the result as ALGO in your account.

Send the ALGO to your account's address and wait for the transaction to confirm.

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
