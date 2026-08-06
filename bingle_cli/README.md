# bingle_cli

Bingle is a decentralized, peer-to-peer messaging protocol that lets users communicate securely and privately
— so your conversations stay yours, with nobody able to read or shut them down.

Bingle runs with no centralized server and uses end-to-end encryption, so there is no central infrastructure for third parties to compromise.
Key management uses the Algorand blockchain to prevent impersonation, while messaging runs over the established
DTLS (Datagram Transport Layer Security) protocol.
A low-cost funding mechanism incentivizes the provision of relay nodes, keeping the network robust and resilient.

## Role in Bingle

`bingle_cli` is the `bingle_cli` command-line client for Bingle. It registers handles on Algorand,
runs a client that sends and receives messages, runs relay nodes, and trades BINGLE on Algorand
(`run`, `register`, `buybingle`, `sellbingle`, `checkrelays`). It is built on
[`bingle_core`](https://github.com/binglefoundation/bingle_rust) and `bingle_local`, and is the
quickest way to see Bingle working from the desktop.

It is part of [`bingle_rust`](https://github.com/binglefoundation/bingle_rust), the Rust reference implementation of
the Bingle protocol.

## Installing

Install the `bingle_cli` binary from [crates.io](https://crates.io/crates/bingle_cli):

```bash
cargo install bingle_cli
```

This puts the `bingle_cli` command on your `PATH`. Installing requires the Rust stable toolchain
(2024 edition, Rust 1.85 or newer) via [rustup](https://rustup.rs).

Run `bingle_cli` with no arguments to see the full usage for every command. A typical flow:

```bash
# register a handle on Algorand (needs a funded account passphrase)
bingle_cli register --handle alice --passphrase "word1 word2 ... word25" --price-units 1

# run a client that sends and receives messages
bingle_cli run --handle alice --passphrase "word1 word2 ... word25"
```

## For developers

Full source, the wallet/handle setup walkthrough, relay operation, and the developer guide live in
the [`bingle_rust`](https://github.com/binglefoundation/bingle_rust) repository on GitHub.
