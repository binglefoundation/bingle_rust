# bingle_test

Bingle is a decentralized, peer-to-peer messaging protocol that lets users communicate securely and privately
— so your conversations stay yours, with nobody able to read or shut them down.

Bingle runs with no centralized server and uses end-to-end encryption, so there is no central infrastructure for third parties to compromise.
Key management uses the Algorand blockchain to prevent impersonation, while messaging runs over the established
DTLS (Datagram Transport Layer Security) protocol.
A low-cost funding mechanism incentivizes the provision of relay nodes, keeping the network robust and resilient.

## Role in Bingle

`bingle_test` is the shared test-support crate for Bingle: fixtures, helpers, and the reusable
algokit-localnet integration harness (`bingle_test::localnet`) used across the other crates' test
suites. It is a development-only crate and is **not published to crates.io**.

It is part of [`bingle_rust`](https://github.com/binglefoundation/bingle_rust), the Rust reference implementation of
the Bingle protocol.

## Installing

`bingle_test` is consumed only within the `bingle_rust` workspace as a path dev-dependency — there
is nothing to install separately. Crates in the workspace reference it like this:

```toml
[dev-dependencies]
bingle_test = { path = "../bingle_test" }

# with the reusable algokit-localnet harness:
# bingle_test = { path = "../bingle_test", features = ["localnet"] }
```

The localnet harness additionally requires
[AlgoKit](https://github.com/algorandfoundation/algokit-cli) (`algokit localnet start`).

## For developers

Full source and the developer guide live in the
[`bingle_rust`](https://github.com/binglefoundation/bingle_rust) repository on GitHub.
