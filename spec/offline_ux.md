# Offline UX — React Native app changes

How the React Native app should surface connectivity to the user, consuming the two signals the
core/JSI layer now exposes for issue #31 (offline send) and issue #18 (outage tolerance).

This spec covers the **app repo** (`client.ts` and the screens that render status). The Rust/JSI and
TypeScript-binding halves are already implemented; this is the remaining consumer wiring.

## Background: two independent networks

The app depends on two networks that fail independently, and the UI must not conflate them:

1. **P2P transport** — the STUN-discovered endpoint plus DTLS relays that actually carry messages.
   Handle lookups are cached, so **message delivery does not need the blockchain**.
2. **Algorand node** — read only for account status (registered handle, asset opt-in, balance) and
   for registration/funding. A node outage does **not** stop messaging.

Historically a node outage produced two bad symptoms: a flood of blockchain errors in the log, and a
UI that kept showing `ACTIVE` as if nothing were wrong. The core changes fixed the log flood and now
expose the state the UI needs; this spec closes the loop.

## Signals the app consumes

### Signal A — transport availability (can we send now?)

- **Source:** `networkAvailable(forceRecheck)` → `Promise<boolean>`.
- **Semantics:** reflects the **P2P transport only** — `true` when listening with a usable route,
  `false` when not listening or when the engine reports `NoConnection`. Deliberately independent of
  Algorand-node reachability. `forceRecheck` is accepted for API compatibility but is not needed
  (the transport state is always current).
- **Event alternative (preferred):** subscribe with `setListeningCallback` and handle the
  `onListening(listening: boolean, natType: string)` event. This is push-based and avoids polling.
  Treat the network as available for sending when `listening === true && natType !== "NoConnection"`
  — the same predicate `networkAvailable()` computes internally.

### Signal B — account status freshness (is the shown status real?)

- **Source:** `keypairStatus()` now returns a `stale: boolean` field alongside
  `status | id | handle | required_algo`.
- **Semantics:** `stale === true` means `status` is a **last-known value returned during a blockchain
  outage**, not a fresh on-chain read (issue #18 A2). The status string itself (e.g. `Active`) is the
  last value that was confirmed; `stale` tells the UI it is currently unverifiable.
- `stale === false` on every fresh read and on first run.

## Required UI behavior

### 1. Offline banner (Signal A)

When the transport is unavailable, show a persistent, non-modal banner (e.g. a thin bar under the nav
header):

> **Offline** — messages will send when you reconnect.

- Show whenever transport availability is `false`.
- Hide as soon as it returns to `true`.
- Do **not** block composing or sending. `queueMessage` works offline; queued messages drain
  automatically when the transport recovers (core `run_processing_loop`), so the compose box stays
  fully usable.

### 2. Per-message pending state (Signal A, already partly present)

Messages sent while offline are queued with `progress = 0`. The app already renders progress from
`getMessages()` — ensure a queued (offline) message reads as **"Pending"** rather than "Failed", and
that it transitions to delivered once the queue drains. No new signal is needed; this is a labeling
check to confirm offline sends never surface as errors.

### 3. "Account status unavailable" indicator (Signal B)

When `keypairStatus()` returns `stale === true`:

- Keep rendering the last-known `status` (the account is still usable — `Active` accounts can still
  send over the transport), but add an **unverified** affordance next to the account/status area, e.g.
  a greyed dot or subtitle:

  > Account status unavailable — showing last known.

- Clear it on the next read that returns `stale === false`.
- Do **not** show a blocking error and do **not** downgrade the status (e.g. don't flip `Active` to
  `None`); `stale` is orthogonal to `status`.

### Combined states

| Transport (A) | `stale` (B) | UI |
|---|---|---|
| available | false | Normal. No banner, no indicator. |
| available | true | No offline banner (sending works); show "account status unavailable" — the node is down but relays are up. |
| unavailable | false | Offline banner; status area normal (status was confirmed before going offline). |
| unavailable | true | Offline banner **and** "account status unavailable". |

The middle two rows are the important correction: transport and node health are shown separately, so a
node-only outage no longer looks like being offline, and vice versa.

## Suggested implementation (client.ts)

Event-driven for transport, keep the existing status poll for the node:

```ts
// Transport (Signal A) — push-based, no polling.
bingle.setListeningCallback(); // native emits onListening
onListeningEvent(({ listening, nat_type }) => {
  const canSend = listening && nat_type !== "NoConnection";
  store.setOffline(!canSend);
});

// Account status (Signal B) — folded into the existing keypairStatus poll.
async function refreshStatus() {
  const s = await bingle.keypairStatus();
  store.setStatus(s.status);
  store.setStatusStale(s.stale); // NEW: drives "account status unavailable"
}
```

- If the app cannot use the listening event on a given platform, fall back to polling
  `networkAvailable(false)` on the existing status-poll timer; it is cheap (no node probe).
- Pass `forceRecheck = false`. There is no reason to force a recheck; the value is always current.

## Explicitly out of scope

- No changes to send/queue logic — `queueMessage` and automatic drain already handle offline sends.
- No new `KeypairStatus` variant. Offline/unavailable is expressed by `stale` (Signal B) and the
  transport banner (Signal A), not by a status enum value. The enum stays
  `None | Unfunded | Funded | Active | UpgradeRequired`.
- Registration/funding flows still require the node; if `networkAvailable()`-style transport is up but
  a register/fund action fails with a host-unreachable error, prompt the user to retry when the
  blockchain is reachable (this is existing behavior, not part of this spec).

## Acceptance

1. Kill only the Algorand node (e.g. block `*.nodely.dev`) with the P2P network up: no offline banner,
   messages still send and echo, and the account area shows "account status unavailable". No error
   flood in the log.
2. Drop the device off the network entirely: offline banner appears, composed messages queue as
   Pending, and they deliver automatically once connectivity returns.
3. Restore connectivity: banner and unavailable indicator both clear on the next status read / listening
   event.
