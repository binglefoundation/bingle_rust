# bingle_local <img src="https://raw.githubusercontent.com/binglefoundation/bingle_rust/staging/docs/assets/bingle_logo.png" alt="Bingle logo" height="36" align="right" />

Bingle is a decentralized, peer-to-peer messaging protocol that lets users communicate securely and privately
— so your conversations stay yours, with nobody able to read or shut them down.

Bingle runs with no centralized server and uses end-to-end encryption, so there is no central infrastructure for third parties to compromise.
Key management uses the Algorand blockchain to prevent impersonation, while messaging runs over the established
DTLS (Datagram Transport Layer Security) protocol.
A low-cost funding mechanism incentivizes the provision of relay nodes, keeping the network robust and resilient.

## Role in Bingle

`bingle_local` is a thin local-state layer over
[`bingle_core`](https://github.com/binglefoundation/bingle_rust): keypair status, the message queue,
and the contact store. It is kept deliberately small so it can be re-implemented natively
(iOS/Android) in future.

It is part of [`bingle_rust`](https://github.com/binglefoundation/bingle_rust), the Rust reference implementation of
the Bingle protocol.

## Installing

Add `bingle_local` to your project from [crates.io](https://crates.io/crates/bingle_local):

```bash
cargo add bingle_local
```

or add it to your `Cargo.toml` directly:

```toml
[dependencies]
bingle_local = "0.2.8"
```

Building the crate requires the Rust stable toolchain (2024 edition, Rust 1.85 or newer) via
[rustup](https://rustup.rs).

## For developers

Full source, architecture, build instructions, and the developer guide live in the
[`bingle_rust`](https://github.com/binglefoundation/bingle_rust) repository on GitHub.

The hosted API reference is on [docs.rs/bingle_local](https://docs.rs/bingle_local) (built
automatically from each crates.io release). Generate it locally for the current checkout with
`cargo doc --open`.
