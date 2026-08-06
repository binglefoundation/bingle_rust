# bingle_core

Bingle is a decentralized, peer-to-peer messaging protocol that lets users communicate securely and privately
— so your conversations stay yours, with nobody able to read or shut them down.

Bingle runs with no centralized server and uses end-to-end encryption, so there is no central infrastructure for third parties to compromise.
Key management uses the Algorand blockchain to prevent impersonation, while messaging runs over the established
DTLS (Datagram Transport Layer Security) protocol.
A low-cost funding mechanism incentivizes the provision of relay nodes, keeping the network robust and resilient.

## Role in Bingle

`bingle_core` is the heart of Bingle: the peer-to-peer comms engine (STUN NAT traversal, DTLS
transport, relay discovery and routing) together with the Algorand integration (`AlgoOps` generic
helpers and `AlgoBingle` app/asset operations such as handle registration and lookup). Every other
Bingle crate — the CLI, the web server, the local-state layer, and the mobile bridge — is built on
top of it.

It is part of [`bingle_rust`](https://github.com/binglefoundation/bingle_rust), the Rust reference implementation of
the Bingle protocol.

## Installing

Add `bingle_core` to your project from [crates.io](https://crates.io/crates/bingle_core):

```bash
cargo add bingle_core
```

or add it to your `Cargo.toml` directly:

```toml
[dependencies]
bingle_core = "0.2.8"
```

Building the crate requires the Rust stable toolchain (2024 edition, Rust 1.85 or newer) via
[rustup](https://rustup.rs).

## For developers

Full source, architecture, build instructions, and the developer guide live in the
[`bingle_rust`](https://github.com/binglefoundation/bingle_rust) repository on GitHub. Generate the
API docs locally with `cargo doc --open`.
