# Public API surface & documentation conventions

This document is the contract for the **published** Bingle crates: `bingle_core`,
`bingle_local`, and `bingle_cli`. It records which items are part of the supported public
interface, which are internal implementation detail, and the doc-comment style every public
item must follow.

It exists so the surface-cleanup and documentation work (issue #90 and its sub-issues) is
consistent across crates: #172/#173 shrink and document `bingle_core` against the core section
below, #174 does `bingle_local`, #175 does `bingle_cli`, #176 enforces coverage, and #177 links
the published docs from the README files.

The three crates that are **not** published — `bingle_webserver`, `bingle_jsi`, and
`bingle_test` (all `publish = false`) — are out of scope here. TypeScript documentation for the
BingleJsi bridge is also out of scope.

## Two kinds of `pub`

Rust has one `pub`, but this repository draws a line between two intents:

- **Supported API** — the interface an external crates.io consumer is meant to build on. These
  items are `pub`, carry doc comments, and appear on docs.rs.
- **Internal, but reachable across the workspace** — items that must stay `pub` only because a
  sibling crate in this workspace (the CLI, web server, local layer, or mobile bridge) imports
  them. They are not a promise to external users.

Prefer to demote internal items to `pub(crate)` / private. When a sibling genuinely needs an
item, keep it `pub` but mark it `#[doc(hidden)]` so it stays reachable without appearing in the
published reference. Fully private is the default; `#[doc(hidden)] pub` is the escape hatch;
documented `pub` is reserved for the supported API listed below.

## `bingle_core`

The peer-to-peer messaging engine plus the Algorand integration. Supported surface:

**Messaging engine (`api`)**
- `api::bingle_api::BingleApi` — the messaging engine trait (the primary entry point).
- `api::bingle_api_impl::BingleApiImpl` — the concrete implementation and its constructor.
- `api::bingle_api::StartOptions` — engine start configuration.
- `api::bingle_api::{BingleError, SendFailureKind}` — error and send-failure types.
- `api::bingle_api::{UserId, Handle}` and the callback aliases `ProgressCallback`,
  `OnMessageHandler`, `OnConnectHandler`, `OnListeningHandler`.
- `api::network_endpoint::NetworkEndpoint` — endpoint addressing type.

**Algorand integration (`blockchain`)**
- `AlgoOps` — generic Algorand helpers (also re-exported at the crate root).
- `AlgoBingle` — Bingle app/asset operations such as handle registration and lookup (also
  re-exported at the crate root).
- `blockchain::error::{AlgoErrorKind, …}` — Algorand error types.

**Support**
- `util::version::{VersionInfo, get_version_info}` and the `get_module_version!` macro /
  `module_version` module.
- `util::logging` — logging mode and initialization (`LogMode`, init helpers).
- `util::config_utils`, `util::cli_utils` — configuration and argument helpers shared with the
  CLI and web server. Supported but secondary; may be narrowed as consumers are cleaned up.

**Internal (not supported; demote or `#[doc(hidden)]` in #172)**
- Transport and networking internals: `stun`, `turn`, `dtls`, `relay`, `packet_transport`.
- Engine internals: the `engine` module, including `BingleAccess` /
  `BingleAccessUnsafeForTests` (in-workspace plumbing, not an external contract), `Engine`,
  `NatType`, `EndpointStatus`.
- Protocol and storage internals: `protocol`, `messages`, `ddb`, `distributed_mutex`, `themes`.
- The `BingleApiInternal` / `BingleApiBoth` traits and the remainder of `util`.
- The `test-hooks` feature and `BingleAccessUnsafeForTests` are test-only and must never appear
  on docs.rs (docs.rs builds with default features, which leave `test-hooks` off).

## `bingle_local`

A thin local-state layer over `bingle_core` (keypair status, message queue, contact store).
Supported surface:

- `api::bingle_local_api::BingleLocalApi` — the local API trait (primary entry point).
- `api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig}` — implementation and config.
- Data types: `Contact`, `Message`, `Keypair`, `KeypairStatus`, `ContactSource`, and the
  `REQUIRED_ALGO` constant.
- Registration: `api::registration::{RegistrationOps, ChainRegistrationOps, run_registration}`.
- Notification posting traits and requests: `api::notify::{AlertPoster, RegisterPoster,
  AlertRequest, HttpAlertPoster, HttpRegisterPoster}`.
- The re-export of `bingle_core::api::bingle_api::SendFailureKind`.

**Internal (not supported; demote or `#[doc(hidden)]` in #174)**
- Envelope/nonce/time helpers in `api::notify::envelope` (`fresh_nonce`, `now_secs`,
  `alert_exp`, `alert_status_accepted`, `build_alert_request`) — construction detail.
- `api::send_retry` helpers (`select_sendable_message`, `classify_send_error`, `SendFailure`,
  `is_transient_send_failure`, `pending_failure_reason`, `RETRY_BACKOFF`) — retry internals.

## `bingle_cli`

Primarily a **binary**. Its supported interface is the command-line itself, documented via
`--help` and the developer guide — not a Rust API. The library target (`src/lib.rs`) exists only
so the pure argument parsers can be unit-tested from the test tree.

- Keep the library surface minimal; items exposed solely for the test tree should be
  `#[doc(hidden)]` (or `pub(crate)` where the test tree allows it).
- The crate-level `//!` doc should describe the binary and point at the CLI docs; it should not
  present the lib modules as a supported API.

## Doc-comment conventions

Applies to every item in the supported API above. Enforced by `#![warn(missing_docs)]` (added in
#176) and `cargo doc` building with no warnings.

- **Crate root** — every published crate has a `//!` overview: one line on what the crate is, a
  short paragraph on the primary entry points, and a minimal example where it makes sense.
- **Every public item** — trait, struct, enum, field, variant, function, type alias, and
  constant gets a `///` comment.
- **Summary line** — the first line is a single, self-contained sentence (rustdoc uses it as the
  summary in module listings). Leave a blank line before any further detail.
- **Sections** — use `# Examples` on primary entry points; `# Errors` on functions returning
  `Result`; `# Panics` where a function can panic. Prefer `no_run` for examples that hit the
  network or the Algorand chain, and `ignore`/`text` where a snippet cannot compile standalone.
- **Intra-doc links** — link related items with `[Type]` / `[method](Type::method)` rather than
  bare names.
- **Repository style (from CLAUDE.md)** — expand an acronym on first use, do not use Title Case
  in comments, and keep example shell commands on a single line.
- **`#[doc(hidden)]`** — apply to any `pub` item kept only for in-workspace reachability, so it
  stays out of the published reference.

## docs.rs build metadata

Each published crate carries a `[package.metadata.docs.rs]` section so docs.rs builds
deterministically:

```toml
[package.metadata.docs.rs]
rustdoc-args = ["--cfg", "docsrs"]
```

docs.rs builds with **default features**, which deliberately leaves `bingle_core`'s `test-hooks`
feature off, so the test-only accessor never reaches the published docs. The `--cfg docsrs` flag
lets later documentation gate platform- or feature-specific items with
`#[cfg_attr(docsrs, doc(cfg(...)))]` without requiring a nightly toolchain for a normal build.
