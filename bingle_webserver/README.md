# bingle_webserver

Bingle is a decentralized, peer-to-peer messaging protocol that lets users communicate securely and privately
— so your conversations stay yours, with nobody able to read or shut them down.

Bingle runs with no centralized server and uses end-to-end encryption, so there is no central infrastructure for third parties to compromise.
Key management uses the Algorand blockchain to prevent impersonation, while messaging runs over the established
DTLS (Datagram Transport Layer Security) protocol.
A low-cost funding mechanism incentivizes the provision of relay nodes, keeping the network robust and resilient.

## Role in Bingle

`bingle_webserver` is an HTTP/WebSocket server that exposes the Bingle API over an axum-based
interface — used to run relays and to drive Bingle from non-Rust environments. It acts as a local
bridge between a browser (or other HTTP) client and the Bingle engine in
[`bingle_core`](https://github.com/binglefoundation/bingle_rust).

It is part of [`bingle_rust`](https://github.com/binglefoundation/bingle_rust), the Rust reference implementation of
the Bingle protocol.

## Installing

`bingle_webserver` is a server application crate and is not published to crates.io — install it by
building from a clone of the repository. This requires the Rust stable toolchain (2024 edition,
Rust 1.85 or newer) via [rustup](https://rustup.rs):

```bash
git clone https://github.com/binglefoundation/bingle_rust.git
cd bingle_rust
cargo build -p bingle_webserver
```

## Building

To build the webserver:

```bash
cargo build -p bingle_webserver
```

## Running

To run the webserver with default settings (port 12121, address 127.0.0.1):

```bash
cargo run -p bingle_webserver -- <handle>
```

You can specify the port and address:

```bash
cargo run -p bingle_webserver -- --port 8080 --address 0.0.0.0 <handle>
```

Other arguments are forwarded to the Bingle API, same as `bingle_cli run`.

### Running against Testnet

To run against the Algorand testnet using the provided Nodely configuration:

```bash
cargo run -p bingle_webserver -- <handle> --node-file nodely_staging_testnet_node.json --passphrase "your passphrase"
```

## Testing with curl

Once the server is running, you can interact with it using `curl`.

### Lookup an ID by handle

```bash
curl "http://localhost:12121/handleLookup?handle=alice"
```

### Send a plaintext message to a User ID

```bash
curl -X POST http://localhost:12121/sendMessageToId \
     -H "Content-Type: application/json" \
     -d '{
       "userId": "P577...",
       "message": { "text": "Hello from curl" }
     }'
```

### Send a message to a handle

```bash
curl -X POST http://localhost:12121/sendMessageToHandle \
     -H "Content-Type: application/json" \
     -d '{
       "handle": "bob",
       "message": { "text": "Hi Bob" }
     }'
```

### Retrieve queued messages

```bash
curl http://localhost:12121/queued
```

### Send a message to a network endpoint

```bash
curl -X POST http://localhost:12121/sendMessageToNetwork \
     -H "Content-Type: application/json" \
     -d '{
       "networkSourceKey": {
         "inetSocketAddress": {
           "host": "127.0.0.1",
           "port": 4433
         }
       },
       "userId": "P577...",
       "message": { "text": "Direct message" }
     }'
```

### Retrieve local messages

```bash
curl http://localhost:12121/local/getMessages
```

Returns the local message list as a JSON array. Each element carries the stored message fields
(`sender_handle`, `recipient_handles`, `timestamp`, `text`, `progress`, …). For a message whose send
failed, the response also describes the failure (issue #108, a follow-up to the typed send-failure
cause from #99):

| Field | Type | Meaning |
| --- | --- | --- |
| `failure_reason` | string | Human-readable reason, for display. Unchanged from before #99. |
| `failure_category` | string enum | Typed cause, mirroring the `bingle_jsi` read model. One of: `HandleNotFound`, `HandleLookupFailed`, `RecipientNotAdvertised`, `InvalidRecipientId`, `NoRelayAvailable`, `RelayAllocationFailed`, `PeerUnreachable`, `NoResponse`, `MalformedAdvert`, `ProtocolError`, `NotReady`, `Unknown`. |
| `failure_retryable` | boolean | Whether the failure is transient and the message will keep retrying. Derived server-side from the category, so clients must not hardcode the retryable set. |
| `failure_kind` | string enum | Raw serde form of the typed cause; retained for back-compat and identical in value to `failure_category`. Prefer `failure_category`. |

All four failure fields are omitted for a message that is pending or delivered. `failure_category`
and `failure_retryable` are additive — existing readers of `failure_reason`/`failure_kind` are
unaffected.

```json
[
  {
    "sender_handle": "me",
    "recipient_handles": ["bob"],
    "timestamp": 1000,
    "text": "hi",
    "progress": 0.5,
    "failure_reason": "Recipient is not connected right now — will keep retrying",
    "failure_kind": "RecipientNotAdvertised",
    "failure_category": "RecipientNotAdvertised",
    "failure_retryable": true
  }
]
```
