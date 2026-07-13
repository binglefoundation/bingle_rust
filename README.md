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

The quickest way to see Bingle work is the **Bingle CLI**: register a handle on Algorand testnet
and then run a client that listens for and sends messages.

### Prerequisites

- **Rust** (stable, 2024 edition — Rust 1.85 or newer) via [rustup](https://rustup.rs).
- An **Algorand account** — a standard 25-word mnemonic. You can create one with any Algorand
  wallet (e.g. [Pera](https://perawallet.app/)) or with
  [AlgoKit](https://github.com/algorandfoundation/algokit-cli) (`algokit goal account new`).
- A small amount of **testnet ALGO** in that account (see [Acquiring ALGO](#acquiring-algo)).

The repository ships a ready-made testnet configuration, `nodely_testnet_node.json`, which points
the CLI at Algorand testnet and the deployed Bingle app/asset — pass it with `--node-file` and you
do not need to supply `--app-id` / `--asset-id` yourself.

### Install the CLI

Clone the repo and install the `bingle_cli` binary with cargo:

```bash
git clone https://github.com/bingle-foundation/bingle_rust.git
cd bingle_rust
cargo install --path bingle_core --bin bingle_cli
```

`cargo install` places `bingle_cli` on your `PATH`. Prefer not to install? Run it in place with
`cargo run -p bingle_core --bin bingle_cli -- <args>` instead of `bingle_cli <args>` in the
examples below.

### Acquiring ALGO

Registering a handle is an on-chain action, so the account needs to be funded (roughly 0.6 ALGO on
testnet covers the minimum balance plus fees). On **testnet**, top up your address for free from a
dispenser:

- The [Algorand testnet dispenser](https://bank.testnet.algorand.network/), or
- `algokit dispenser fund` if you use AlgoKit.

Paste your account's address, request funds, and wait for the transaction to confirm. (On
**mainnet** you would instead acquire ALGO from an exchange — testnet is recommended for
exploration.)

### Register a handle

```bash
bingle_cli register \
  --handle alice \
  --passphrase "word1 word2 ... word25" \
  --node-file nodely_testnet_node.json \
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
  --passphrase "word1 word2 ... word25" \
  --node-file nodely_testnet_node.json
```

Add `--relay` to run the node as a relay, or `--static-ip <ip:port>` to advertise a fixed public
address. Run `bingle_cli` with no arguments to see the full usage for every command (`run`,
`register`, `buybingle`, `sellbingle`, `checkrelays`).

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
