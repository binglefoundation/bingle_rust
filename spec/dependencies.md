# Workspace crate dependencies

Regenerate with `python3 scripts/gen_dependencies_doc.py` from the repo root.

Generated with `cargo tree -e normal` (runtime dependencies only;
direct dev- and build-dependencies are listed at the end). For each
transitive dependency, the **Via** column lists the direct dependencies
whose subtree pulls it in.

`bingle-local`, `bingle_jsi` and `bingle_webserver` all depend on
`rust_comms`, so they inherit its entire tree. To avoid repeating ~200
rows per crate, transitive dependencies reached *only* through workspace
crates are summarised with a count; the tables list dependencies that a
non-workspace direct dependency also pulls in.

## rust_comms

Core Bingle comms library: P2P messaging engine (DTLS, STUN, Algorand integration).

### Direct dependencies

| Crate | Version | What it does |
|---|---|---|
| `algonaut` | 0.8.0 | A Rusty sdk for the Algorand blockchain |
| `anyhow` | 1.0.102 | Flexible concrete Error type built on std::error::Error |
| `base64` | 0.21.7 | encodes and decodes base64 as bytes or utf8 |
| `chrono` | 0.4.45 | Date and time library for Rust |
| `ctrlc` | 3.5.2 | Easy Ctrl-C handler for Rust projects |
| `data-encoding` | 2.11.0 | Efficient and customizable data-encoding functions like base64, base32, and hex |
| `ed25519-dalek` | 2.2.0 | Fast and efficient ed25519 EdDSA key generations, signing, and verification in pure Rust |
| `libc` | 0.2.186 | Raw FFI bindings to platform libraries like libc |
| `openssl` | 0.10.81 | OpenSSL bindings |
| `openssl-sys` | 0.9.117 | FFI bindings to OpenSSL |
| `rand_core` | 0.6.4 | Core random number generator traits and tools for implementation |
| `serde` | 1.0.228 | A generic serialization/deserialization framework |
| `serde_json` | 1.0.150 | A JSON serialization file format |
| `sha2` | 0.10.9 | Pure Rust implementation of the SHA-2 hash function family including SHA-224, SHA-256, SHA-384, and SHA-512 |
| `stun-rs` | 0.1.11 | Rust framework to manage STUN messages |
| `thiserror` | 1.0.69 | derive(Error) |
| `tokio` | 1.52.3 | An event-driven, non-blocking I/O platform for writing asynchronous I/O backed applications |
| `tokio-openssl` | 0.6.5 | An implementation of SSL streams for Tokio backed by OpenSSL |
| `tracing` | 0.1.44 | Application-level tracing for Rust |
| `tracing-subscriber` | 0.3.23 | Utilities for implementing and composing `tracing` subscribers |
| `uuid` | 1.23.2 | A library to generate and parse UUIDs |

### Transitive dependencies (204)

| Crate | Version(s) | Via | What it does |
|---|---|---|---|
| `aho-corasick` | 1.1.4 | `algonaut`, `tracing-subscriber` | Fast multiple substring searching |
| `algonaut_abi` | 0.8.0 | `algonaut` | Application Binary Interface (ABI) to invoke smart contract methods with a standarized interface |
| `algonaut_abi_macros` | 0.8.0 | `algonaut` | Compile-time checked ARC-4 ABI method-call macros (abi_call!/abi_method!) for algonaut_abi |
| `algonaut_abi_sig` | 0.8.0 | `algonaut` | Pure ARC-4 ABI signature and type grammar shared by algonaut_abi and its compile-time macros |
| `algonaut_algod` | 0.8.0 | `algonaut` | API endpoint for algod operations |
| `algonaut_core` | 0.8.0 | `algonaut` | Core structs for the Algorand blockchain |
| `algonaut_crypto` | 0.8.0 | `algonaut` | Crypto utilities for the Algorand blockchain |
| `algonaut_encoding` | 0.8.0 | `algonaut` | Encoding utilities for the Algorand blockchain |
| `algonaut_indexer` | 0.8.0 | `algonaut` | Algorand ledger analytics API |
| `algonaut_kmd` | 0.8.0 | `algonaut` | API clients for the Algorand blockchain |
| `algonaut_model` | 0.8.0 | `algonaut` | Model objects |
| `algonaut_transaction` | 0.8.0 | `algonaut` | Implementation of the Algorand blockchain transaction set |
| `anstream` | 1.0.0 | `algonaut` | IO stream adapters for writing colored text that will gracefully degrade according to your terminal's capabilities |
| `anstyle` | 1.0.14 | `algonaut` | ANSI text styling |
| `anstyle-parse` | 1.0.0 | `algonaut` | Parse ANSI Style Escapes |
| `anstyle-query` | 1.1.5 | `algonaut` | Look up colored console capabilities |
| `async-trait` | 0.1.89 | `algonaut` | Type erasure for async trait methods |
| `atomic-waker` | 1.1.2 | `algonaut` | A synchronization primitive for task wakeup |
| `aws-lc-rs` | 1.17.0 | `algonaut` | aws-lc-rs is a cryptographic library using AWS-LC for its cryptographic operations. This library strives to be API-compatible with the popular Rust library named ring |
| `aws-lc-sys` | 0.41.0 | `algonaut` | AWS-LC is a general-purpose cryptographic library maintained by the AWS Cryptography team for AWS and their customers. It іs based on code from the Google BoringSSL project and the OpenSSL project |
| `bitflags` | 2.13.0 | `algonaut`, `ctrlc`, `openssl`, `tokio-openssl` | A macro to generate structures which behave like bitflags |
| `block-buffer` | 0.10.4, 0.12.0 | `algonaut`, `ed25519-dalek`, `sha2`, `stun-rs` | Buffer types for block processing of data |
| `block2` | 0.6.2 | `ctrlc` | Apple's C language extension of blocks |
| `bounded-integer` | 0.5.8 | `stun-rs` | Bounded integers |
| `byteorder` | 1.5.0 | `stun-rs` | Library for reading/writing numbers in big-endian and little-endian |
| `bytes` | 1.11.1 | `algonaut`, `tokio`, `tokio-openssl` | Types and traits for working with bytes |
| `cfg-if` | 1.0.4 | `algonaut`, `ctrlc`, `ed25519-dalek`, `openssl`, `rand_core`, `sha2`, `stun-rs`, `tokio-openssl`, `tracing-subscriber`, `uuid` | A macro to ergonomically define an item depending on a large number of #[cfg] parameters. Structured like an if-else chain, the first matching branch is the item that gets emitted |
| `colorchoice` | 1.0.5 | `algonaut` | Global override of color control |
| `const-oid` | 0.10.2 | `algonaut` | Const-friendly implementation of the ISO/IEC Object Identifier (OID) standard as defined in ITU X.660, with support for BER/DER encoding/decoding as well as heapless no_std (i.e. embedded) support |
| `convert_case` | 0.10.0 | `algonaut` | Convert strings into any case |
| `core-foundation` | 0.10.1, 0.9.4 | `algonaut` | Bindings to Core Foundation for macOS |
| `core-foundation-sys` | 0.8.7 | `algonaut`, `chrono` | Bindings to Core Foundation for macOS |
| `cpufeatures` | 0.2.17, 0.3.0 | `algonaut`, `ed25519-dalek`, `sha2`, `stun-rs` | Lightweight runtime CPU feature detection for aarch64, loongarch64, and x86/x86_64 targets, with no_std support and support for mobile targets including Android and iOS |
| `crc` | 3.4.0 | `stun-rs` | Rust implementation of CRC with support of various standards |
| `crc-catalog` | 2.5.0 | `stun-rs` | Catalog of CRC algorithms (generated from http://reveng.sourceforge.net/crc-catalogue) expressed as simple Rust structs |
| `crypto-common` | 0.1.7, 0.2.2 | `algonaut`, `ed25519-dalek`, `sha2`, `stun-rs` | Common traits used by cryptographic algorithms |
| `curve25519-dalek` | 4.1.3 | `algonaut`, `ed25519-dalek` | A pure-Rust implementation of group operations on ristretto255 and Curve25519 |
| `darling` | 0.23.0 | `algonaut` | A proc-macro library for reading attributes into structs when implementing custom derives |
| `darling_core` | 0.23.0 | `algonaut` | Helper crate for proc-macro library for reading attributes into structs when implementing custom derives. Use https://crates.io/crates/darling in your code |
| `darling_macro` | 0.23.0 | `algonaut` | Internal support for a proc-macro library for reading attributes into structs when implementing custom derives. Use https://crates.io/crates/darling in your code |
| `derive_more` | 2.1.1 | `algonaut` | Adds #[derive(x)] macros for more traits |
| `derive_more-impl` | 2.1.1 | `algonaut` | Internal implementation of `derive_more` crate |
| `digest` | 0.10.7, 0.11.3 | `algonaut`, `ed25519-dalek`, `sha2`, `stun-rs` | Traits for cryptographic hash functions and message authentication codes |
| `dispatch2` | 0.3.1 | `ctrlc` | Bindings and wrappers for Apple's Grand Central Dispatch (GCD) |
| `displaydoc` | 0.2.6 | `algonaut` | A derive macro for implementing the display Trait via a doc comment and string interpolation |
| `ed25519` | 2.2.3 | `algonaut`, `ed25519-dalek` | Edwards Digital Signature Algorithm (EdDSA) over Curve25519 (as specified in RFC 8032) support library providing signature type definitions and PKCS#8 private key decoding/encoding support |
| `encoding_rs` | 0.8.35 | `algonaut` | A Gecko-oriented implementation of the Encoding Standard |
| `enumflags2` | 0.7.12 | `stun-rs` | Enum-based bit flags |
| `enumflags2_derive` | 0.7.12 | `stun-rs` | Do not use directly, use the reexport in the `enumflags2` crate. This allows for better compatibility across versions |
| `env_filter` | 1.0.1 | `algonaut` | Filter log events using environment variables |
| `env_logger` | 0.11.10 | `algonaut` | A logging implementation for `log` which is configured via an environment variable |
| `equivalent` | 1.0.2 | `algonaut` | Traits for key comparison in maps |
| `fallible-iterator` | 0.3.0 | `stun-rs` | Fallible iterator traits |
| `fnv` | 1.0.7 | `algonaut` | Fowler–Noll–Vo hash function |
| `foreign-types` | 0.3.2 | `openssl`, `tokio-openssl` | A framework for Rust wrappers over C APIs |
| `foreign-types-shared` | 0.1.1 | `openssl`, `tokio-openssl` | An internal crate used by foreign-types |
| `form_urlencoded` | 1.2.2 | `algonaut` | Parser and serializer for the application/x-www-form-urlencoded syntax, as used by HTML forms |
| `futures-channel` | 0.3.32 | `algonaut` | Channels for asynchronous communication using futures-rs |
| `futures-core` | 0.3.32 | `algonaut` | The core traits and types in for the `futures` library |
| `futures-sink` | 0.3.32 | `algonaut` | The asynchronous `Sink` trait for the futures-rs library |
| `futures-task` | 0.3.32 | `algonaut` | Tools for working with tasks |
| `futures-timer` | 3.0.4 | `algonaut` | Timeouts for futures |
| `futures-util` | 0.3.32 | `algonaut` | Common utilities and extension traits for the futures-rs library |
| `generic-array` | 0.14.7 | `algonaut`, `ed25519-dalek`, `sha2`, `stun-rs` | Generic types implementing functionality of arrays |
| `getrandom` | 0.2.17, 0.3.4, 0.4.2 | `algonaut`, `ed25519-dalek`, `rand_core`, `stun-rs`, `uuid` | A small cross-platform library for retrieving random data from system source |
| `h2` | 0.4.14 | `algonaut` | An HTTP/2 client and server |
| `hashbrown` | 0.17.1 | `algonaut` | A Rust port of Google's SwissTable hash map |
| `hmac` | 0.12.1 | `stun-rs` | Generic implementation of Hash-based Message Authentication Code (HMAC) |
| `hmac-sha1` | 0.2.2 | `stun-rs` | A simple wrapper around the RustCrypto hmac and sha1 crates for simple HMAC-SHA1 generation |
| `hmac-sha256` | 1.1.14 | `stun-rs` | A small, self-contained SHA256, HMAC-SHA256, and HKDF-SHA256 implementation |
| `hostname-validator` | 1.1.1 | `stun-rs` | Validate hostnames according to IETF RFC 1123 |
| `http` | 1.4.1 | `algonaut` | A set of types for representing HTTP requests and responses |
| `http-body` | 1.0.1 | `algonaut` | Trait representing an asynchronous, streaming, HTTP request or response body |
| `http-body-util` | 0.1.3 | `algonaut` | Combinators and adapters for HTTP request or response bodies |
| `httparse` | 1.10.1 | `algonaut` | A tiny, safe, speedy, zero-copy HTTP/1.x parser |
| `hybrid-array` | 0.4.12 | `algonaut` | Hybrid typenum-based and const generic array types designed to provide the flexibility of typenum-based expressions while also allowing interoperability and a transition path to const generics |
| `hyper` | 1.10.1 | `algonaut` | A protective and efficient HTTP library for all |
| `hyper-rustls` | 0.27.9 | `algonaut` | Rustls+hyper integration for pure rust HTTPS |
| `hyper-util` | 0.1.20 | `algonaut` | hyper utilities |
| `iana-time-zone` | 0.1.65 | `chrono` | get the IANA time zone for the current system |
| `icu_collections` | 2.2.0 | `algonaut` | Collection of API for use in ICU libraries |
| `icu_locale_core` | 2.2.0 | `algonaut` | API for managing Unicode Language and Locale Identifiers |
| `icu_normalizer` | 2.2.0 | `algonaut` | API for normalizing text into Unicode Normalization Forms |
| `icu_normalizer_data` | 2.2.0 | `algonaut` | Data for the icu_normalizer crate |
| `icu_properties` | 2.2.0 | `algonaut` | Definitions for Unicode properties |
| `icu_properties_data` | 2.2.0 | `algonaut` | Data for the icu_properties crate |
| `icu_provider` | 2.2.0 | `algonaut` | Trait and struct definitions for the ICU data provider |
| `ident_case` | 1.0.1 | `algonaut` | Utility for applying case rules to Rust identifiers |
| `idna` | 1.1.0 | `algonaut` | IDNA (Internationalizing Domain Names in Applications) and Punycode |
| `idna_adapter` | 1.2.2 | `algonaut` | Back end adapter for idna |
| `indexmap` | 2.14.0 | `algonaut` | A hash table with consistent order and fast iteration |
| `instant` | 0.1.13 | `algonaut` | Unmaintained, consider using web-time instead - A partial replacement for std::time::Instant that works on WASM to |
| `ipnet` | 2.12.0 | `algonaut` | Provides types and useful methods for working with IPv4 and IPv6 network addresses, commonly called IP prefixes. The new `IpNet`, `Ipv4Net`, and `Ipv6Net` types build on the existing `IpAddr`, `Ipv4Addr`, and `Ipv6Addr` types already provided in Rust's standard library and align to their design to stay consistent. The module also provides useful traits that extend `Ipv4Addr` and `Ipv6Addr` with methods for `Add`, `Sub`, `BitAnd`, and `BitOr` operations. The module only uses stable feature so it is guaranteed to compile using the stable toolchain |
| `is_terminal_polyfill` | 1.70.2 | `algonaut` | Polyfill for `is_terminal` stdlib feature for use with older MSRVs |
| `itoa` | 1.0.18 | `algonaut`, `serde_json` | Fast integer primitive to string conversion |
| `jiff` | 0.2.28 | `algonaut` | A date-time library that encourages you to jump into the pit of success. This library is heavily inspired by the Temporal project |
| `lazy_static` | 1.5.0 | `algonaut`, `stun-rs`, `tracing-subscriber` | A macro for declaring lazily evaluated statics in Rust |
| `litemap` | 0.8.2 | `algonaut` | A key-value Map implementation based on a flat, sorted Vec |
| `log` | 0.4.32 | `algonaut`, `tracing-subscriber` | A lightweight logging facade for Rust |
| `matchers` | 0.2.0 | `tracing-subscriber` | Regex matching on character and byte streams |
| `md5` | 0.7.0 | `stun-rs` | The package provides the MD5 hash function |
| `memchr` | 2.8.1 | `algonaut`, `serde_json`, `stun-rs`, `tracing-subscriber` | Provides extremely fast (uses SIMD on x86_64, aarch64 and wasm32) routines for 1, 2 or 3 byte search and single substring search |
| `mime` | 0.3.17 | `algonaut` | Strongly Typed Mimes |
| `mime_guess` | 2.0.5 | `algonaut` | A simple crate for detection of a file's MIME type by its extension |
| `mio` | 1.2.1 | `algonaut`, `tokio`, `tokio-openssl` | Lightweight non-blocking I/O |
| `nix` | 0.31.3 | `ctrlc` | Rust friendly bindings to *nix APIs |
| `nu-ansi-term` | 0.50.3 | `tracing-subscriber` | Library for ANSI terminal colors and styles (bold, underline) |
| `num-bigint` | 0.4.6 | `algonaut` | Big integer implementation for Rust |
| `num-integer` | 0.1.46 | `algonaut` | Integer traits and functions |
| `num-traits` | 0.2.19 | `algonaut`, `chrono` | Numeric traits for generic mathematics |
| `objc2` | 0.6.4 | `ctrlc` | Objective-C interface and runtime bindings |
| `objc2-encode` | 4.1.0 | `ctrlc` | Objective-C type-encoding representation and parsing |
| `once_cell` | 1.21.4 | `algonaut`, `tracing`, `tracing-subscriber` | Single assignment cells and lazy values |
| `openssl-macros` | 0.1.1 | `openssl`, `tokio-openssl` | Internal macros used by the openssl crate |
| `paste` | 1.0.15 | `stun-rs` | Macros for all your token pasting needs |
| `percent-encoding` | 2.3.2 | `algonaut` | Percent encoding and decoding |
| `pest` | 2.8.6 | `stun-rs` | The Elegant Parser |
| `pest_derive` | 2.8.6 | `stun-rs` | pest's derive macro |
| `pest_generator` | 2.8.6 | `stun-rs` | pest code generator |
| `pest_meta` | 2.8.6 | `stun-rs` | pest meta language parser and validator |
| `pin-project-lite` | 0.2.17 | `algonaut`, `tokio`, `tokio-openssl`, `tracing`, `tracing-subscriber` | A lightweight version of pin-project written with declarative macros |
| `potential_utf` | 0.1.5 | `algonaut` | Unvalidated string and character types |
| `ppv-lite86` | 0.2.21 | `algonaut`, `stun-rs` | Cross-platform cryptography-oriented low-level SIMD library |
| `precis-core` | 0.1.11 | `stun-rs` | PRECIS Framework: Preparation, Enforcement, and Comparison of Internationalized Strings in Application Protocols as defined in rfc8264 |
| `precis-profiles` | 0.1.13 | `stun-rs` | Implementation of the PRECIS Framework: Preparation, Enforcement, and Comparison of Internationalized Strings Representing Usernames and Passwords as defined in rfc8265; and Nicknames as defined in rfc8266 |
| `proc-macro2` | 1.0.106 | `algonaut`, `chrono`, `openssl`, `serde`, `stun-rs`, `thiserror`, `tokio`, `tokio-openssl`, `tracing`, `tracing-subscriber` | A substitute implementation of the compiler's `proc_macro` API to decouple token-based libraries from the procedural macro use case |
| `quote` | 1.0.45 | `algonaut`, `chrono`, `openssl`, `serde`, `stun-rs`, `thiserror`, `tokio`, `tokio-openssl`, `tracing`, `tracing-subscriber` | Quasi-quoting macro quote!(...) |
| `quoted-string-parser` | 0.1.0 | `stun-rs` | Quoted string parser for grammar defined in RFC3261 |
| `rand` | 0.8.6, 0.9.4 | `algonaut`, `stun-rs` | Random number generators and other randomness functionality |
| `rand_chacha` | 0.3.1, 0.9.0 | `algonaut`, `stun-rs` | ChaCha random number generator |
| `regex` | 1.12.3 | `algonaut` | An implementation of regular expressions for Rust. This implementation uses finite automata and guarantees linear time matching on all inputs |
| `regex-automata` | 0.4.14 | `algonaut`, `tracing-subscriber` | Automata construction and matching using regular expressions |
| `regex-syntax` | 0.8.10 | `algonaut`, `tracing-subscriber` | A regular expression parser |
| `reqwest` | 0.13.4 | `algonaut` | higher level HTTP client library |
| `rmp` | 0.8.15 | `algonaut` | Pure Rust MessagePack serialization implementation |
| `rmp-serde` | 1.3.1 | `algonaut` | Serde support for MessagePack |
| `rustls` | 0.23.40 | `algonaut` | Rustls is a modern TLS library written in Rust |
| `rustls-pki-types` | 1.14.1 | `algonaut` | Shared types for the rustls PKI ecosystem |
| `rustls-platform-verifier` | 0.7.0 | `algonaut` | rustls-platform-verifier supports verifying TLS certificates in rustls with the operating system verifier |
| `rustls-webpki` | 0.103.13 | `algonaut` | Web PKI X.509 Certificate Verification |
| `ryu` | 1.0.23 | `algonaut` | Fast floating point to string conversion |
| `security-framework` | 3.7.0 | `algonaut` | Security.framework bindings for macOS and iOS |
| `security-framework-sys` | 2.17.0 | `algonaut` | Apple `Security.framework` low-level FFI bindings |
| `serde_bytes` | 0.11.19 | `algonaut` | Optimized handling of `&[u8]` and `Vec<u8>` for Serde |
| `serde_core` | 1.0.228 | `algonaut`, `chrono`, `serde`, `serde_json`, `uuid` | Serde traits only, with no support for derive -- use the `serde` crate instead |
| `serde_derive` | 1.0.228 | `algonaut`, `chrono`, `serde` | Macros 1.1 implementation of #[derive(Serialize, Deserialize)] |
| `serde_urlencoded` | 0.7.1 | `algonaut` | `x-www-form-urlencoded` meets Serde |
| `serde_with` | 3.21.0 | `algonaut` | Custom de/serialization functions for Rust's serde |
| `serde_with_macros` | 3.21.0 | `algonaut` | proc-macro library for serde_with |
| `sha1` | 0.10.6 | `stun-rs` | SHA-1 hash function |
| `sharded-slab` | 0.1.7 | `tracing-subscriber` | A lock-free concurrent slab |
| `signature` | 2.2.0 | `algonaut`, `ed25519-dalek` | Traits for cryptographic signature algorithms (e.g. ECDSA, Ed25519) |
| `slab` | 0.4.12 | `algonaut` | Pre-allocated storage for a uniform data type |
| `smallvec` | 1.15.1 | `algonaut`, `tracing-subscriber` | 'Small vector' optimization: store up to a small number of items on the stack |
| `socket2` | 0.6.4 | `algonaut`, `tokio`, `tokio-openssl` | Utilities for handling networking sockets with a maximal amount of configuration possible intended |
| `stable_deref_trait` | 1.2.1 | `algonaut` | An unsafe marker trait for types like Box and Rc that dereference to a stable address even when moved, and hence can be used with libraries such as owning_ref and rental |
| `static_assertions` | 1.1.0 | `algonaut` | Compile-time assertions to ensure that invariants are met |
| `strsim` | 0.11.1 | `algonaut` | Implementations of string similarity metrics. Includes Hamming, Levenshtein, OSA, Damerau-Levenshtein, Jaro, Jaro-Winkler, and Sørensen-Dice |
| `subtle` | 2.6.1 | `algonaut`, `ed25519-dalek`, `sha2`, `stun-rs` | Pure-Rust traits and utilities for constant-time cryptographic implementations |
| `syn` | 2.0.117 | `algonaut`, `chrono`, `openssl`, `serde`, `stun-rs`, `thiserror`, `tokio`, `tokio-openssl`, `tracing`, `tracing-subscriber` | Parser for Rust source code |
| `sync_wrapper` | 1.0.2 | `algonaut` | A tool for enlisting the compiler's help in proving the absence of concurrency |
| `synstructure` | 0.13.2 | `algonaut` | Helper methods and macros for custom derives |
| `system-configuration` | 0.7.0 | `algonaut` | Bindings to SystemConfiguration framework for macOS |
| `system-configuration-sys` | 0.6.0 | `algonaut` | Low level bindings to SystemConfiguration framework for macOS |
| `thiserror-impl` | 1.0.69, 2.0.18 | `algonaut`, `thiserror` | Implementation detail of the `thiserror` crate |
| `thread_local` | 1.1.9 | `tracing-subscriber` | Per-object thread-local storage |
| `tinystr` | 0.8.3 | `algonaut` | A small ASCII-only bounded length string representation |
| `tinyvec` | 1.11.0 | `stun-rs` | `tinyvec` provides 100% safe vec-like data structures |
| `tinyvec_macros` | 0.1.1 | `stun-rs` | Some macros for tiny containers |
| `tokio-macros` | 2.7.0 | `algonaut`, `tokio`, `tokio-openssl` | Tokio's proc macros |
| `tokio-rustls` | 0.26.4 | `algonaut` | Asynchronous TLS/SSL streams for Tokio using Rustls |
| `tokio-util` | 0.7.18 | `algonaut` | Additional utilities for working with Tokio |
| `tower` | 0.5.3 | `algonaut` | Tower is a library of modular and reusable components for building robust clients and servers |
| `tower-http` | 0.6.11 | `algonaut` | Tower middleware and utilities for HTTP clients and servers |
| `tower-layer` | 0.3.3 | `algonaut` | Decorates a `Service` to allow easy composition between `Service`s |
| `tower-service` | 0.3.3 | `algonaut` | Trait representing an asynchronous, request / response based, client or server |
| `tracing-attributes` | 0.1.31 | `algonaut`, `tracing`, `tracing-subscriber` | Procedural macro attributes for automatically instrumenting functions |
| `tracing-core` | 0.1.36 | `algonaut`, `tracing`, `tracing-subscriber` | Core primitives for application-level tracing |
| `tracing-log` | 0.2.0 | `tracing-subscriber` | Provides compatibility between `tracing` and the `log` crate |
| `try-lock` | 0.2.5 | `algonaut` | A lightweight atomic lock |
| `typenum` | 1.20.1 | `algonaut`, `ed25519-dalek`, `sha2`, `stun-rs` | Typenum is a Rust library for type-level numbers evaluated at compile time. It currently supports bits, unsigned integers, and signed integers. It also provides a type-level array of type-level numbers, but its implementation is incomplete |
| `ucd-trie` | 0.1.7 | `stun-rs` | A trie for storing Unicode codepoint sets and maps |
| `unicase` | 2.9.0 | `algonaut` | A case-insensitive wrapper around strings |
| `unicode-ident` | 1.0.24 | `algonaut`, `chrono`, `openssl`, `serde`, `stun-rs`, `thiserror`, `tokio`, `tokio-openssl`, `tracing`, `tracing-subscriber` | Determine whether characters have the XID_Start or XID_Continue properties according to Unicode Standard Annex #31 |
| `unicode-normalization` | 0.1.25 | `stun-rs` | This crate provides functions for normalization of Unicode strings, including Canonical and Compatible Decomposition and Recomposition, as described in Unicode Standard Annex #15 |
| `unicode-segmentation` | 1.13.3 | `algonaut` | This crate provides Grapheme Cluster, Word and Sentence boundaries according to Unicode Standard Annex #29 rules |
| `unicode-xid` | 0.2.6 | `algonaut` | Determine whether characters have the XID_Start or XID_Continue properties according to Unicode Standard Annex #31 |
| `untrusted` | 0.9.0 | `algonaut` | Safe, fast, zero-panic, zero-crashing, zero-allocation parsing of untrusted inputs in Rust |
| `url` | 2.5.8 | `algonaut` | URL library for Rust, based on the WHATWG URL Standard |
| `urlencoding` | 2.1.3 | `algonaut` | A Rust library for doing URL percentage encoding |
| `utf8_iter` | 1.0.4 | `algonaut` | Iterator by char over potentially-invalid UTF-8 in &[u8] |
| `utf8parse` | 0.2.2 | `algonaut` | Table-driven UTF-8 parser |
| `want` | 0.3.1 | `algonaut` | Detect when another Future wants a result |
| `writeable` | 0.6.3 | `algonaut` | A more efficient alternative to fmt::Display |
| `yoke` | 0.8.3 | `algonaut` | Abstraction allowing borrowed data to be carried along with the backing data it borrows from |
| `yoke-derive` | 0.8.2 | `algonaut` | Custom derive for the yoke crate |
| `zerocopy` | 0.8.50 | `algonaut`, `stun-rs` | Zerocopy makes zero-cost memory manipulation effortless. We write "unsafe" so you don't have to |
| `zerofrom` | 0.1.8 | `algonaut` | ZeroFrom trait for constructing |
| `zerofrom-derive` | 0.1.7 | `algonaut` | Custom derive for the zerofrom crate |
| `zeroize` | 1.8.2 | `algonaut`, `ed25519-dalek` | Securely clear secrets from memory with a simple trait built on stable Rust primitives which guarantee memory is zeroed using an operation will not be 'optimized away' by the compiler. Uses a portable pure Rust implementation that works everywhere, even WASM! |
| `zerotrie` | 0.2.4 | `algonaut` | A data structure that efficiently maps strings to integers |
| `zerovec` | 0.11.6 | `algonaut` | Zero-copy vector backed by a byte array |
| `zerovec-derive` | 0.11.3 | `algonaut` | Custom derive for the zerovec crate |
| `zmij` | 1.0.21 | `algonaut`, `serde_json` | A double-to-string conversion algorithm based on Schubfach and yy |

## bingle-local

Local API crate for storing messages and contacts for Bingle.

### Direct dependencies

| Crate | Version | What it does |
|---|---|---|
| `rust_comms` | 0.1.0 | Core Bingle comms library: P2P messaging engine (DTLS, STUN, Algorand integration) |
| `serde` | 1.0.228 | A generic serialization/deserialization framework |
| `serde_json` | 1.0.150 | A JSON serialization file format |
| `tracing` | 0.1.44 | Application-level tracing for Rust |
| `tracing-subscriber` | 0.3.23 | Utilities for implementing and composing `tracing` subscribers |

### Transitive dependencies (221)

196 crates are inherited solely via the workspace dependencies `rust_comms` — see the `rust_comms` section above for what each does. The 25 crates below are (also) pulled in by this crate's own direct dependencies:

| Crate | Version(s) | Via | What it does |
|---|---|---|---|
| `aho-corasick` | 1.1.4 | `rust_comms`, `tracing-subscriber` | Fast multiple substring searching |
| `cfg-if` | 1.0.4 | `rust_comms`, `tracing-subscriber` | A macro to ergonomically define an item depending on a large number of #[cfg] parameters. Structured like an if-else chain, the first matching branch is the item that gets emitted |
| `itoa` | 1.0.18 | `rust_comms`, `serde_json` | Fast integer primitive to string conversion |
| `lazy_static` | 1.5.0 | `rust_comms`, `tracing-subscriber` | A macro for declaring lazily evaluated statics in Rust |
| `log` | 0.4.32 | `rust_comms`, `tracing-subscriber` | A lightweight logging facade for Rust |
| `matchers` | 0.2.0 | `rust_comms`, `tracing-subscriber` | Regex matching on character and byte streams |
| `memchr` | 2.8.1 | `rust_comms`, `serde_json`, `tracing-subscriber` | Provides extremely fast (uses SIMD on x86_64, aarch64 and wasm32) routines for 1, 2 or 3 byte search and single substring search |
| `nu-ansi-term` | 0.50.3 | `rust_comms`, `tracing-subscriber` | Library for ANSI terminal colors and styles (bold, underline) |
| `once_cell` | 1.21.4 | `rust_comms`, `tracing`, `tracing-subscriber` | Single assignment cells and lazy values |
| `pin-project-lite` | 0.2.17 | `rust_comms`, `tracing`, `tracing-subscriber` | A lightweight version of pin-project written with declarative macros |
| `proc-macro2` | 1.0.106 | `rust_comms`, `serde`, `tracing`, `tracing-subscriber` | A substitute implementation of the compiler's `proc_macro` API to decouple token-based libraries from the procedural macro use case |
| `quote` | 1.0.45 | `rust_comms`, `serde`, `tracing`, `tracing-subscriber` | Quasi-quoting macro quote!(...) |
| `regex-automata` | 0.4.14 | `rust_comms`, `tracing-subscriber` | Automata construction and matching using regular expressions |
| `regex-syntax` | 0.8.10 | `rust_comms`, `tracing-subscriber` | A regular expression parser |
| `serde_core` | 1.0.228 | `rust_comms`, `serde`, `serde_json` | Serde traits only, with no support for derive -- use the `serde` crate instead |
| `serde_derive` | 1.0.228 | `rust_comms`, `serde` | Macros 1.1 implementation of #[derive(Serialize, Deserialize)] |
| `sharded-slab` | 0.1.7 | `rust_comms`, `tracing-subscriber` | A lock-free concurrent slab |
| `smallvec` | 1.15.1 | `rust_comms`, `tracing-subscriber` | 'Small vector' optimization: store up to a small number of items on the stack |
| `syn` | 2.0.117 | `rust_comms`, `serde`, `tracing`, `tracing-subscriber` | Parser for Rust source code |
| `thread_local` | 1.1.9 | `rust_comms`, `tracing-subscriber` | Per-object thread-local storage |
| `tracing-attributes` | 0.1.31 | `rust_comms`, `tracing`, `tracing-subscriber` | Procedural macro attributes for automatically instrumenting functions |
| `tracing-core` | 0.1.36 | `rust_comms`, `tracing`, `tracing-subscriber` | Core primitives for application-level tracing |
| `tracing-log` | 0.2.0 | `rust_comms`, `tracing-subscriber` | Provides compatibility between `tracing` and the `log` crate |
| `unicode-ident` | 1.0.24 | `rust_comms`, `serde`, `tracing`, `tracing-subscriber` | Determine whether characters have the XID_Start or XID_Continue properties according to Unicode Standard Annex #31 |
| `zmij` | 1.0.21 | `rust_comms`, `serde_json` | A double-to-string conversion algorithm based on Schubfach and yy |

## bingle_jsi

React Native JSI bridge for Bingle using uniffi proc macros.

### Direct dependencies

| Crate | Version | What it does |
|---|---|---|
| `bingle-local` | 0.0.1 | Local API crate for storing messages and contacts for Bingle |
| `chrono` | 0.4.45 | Date and time library for Rust |
| `rust_comms` | 0.1.0 | Core Bingle comms library: P2P messaging engine (DTLS, STUN, Algorand integration) |
| `serde_json` | 1.0.150 | A JSON serialization file format |
| `thiserror` | 1.0.69 | derive(Error) |
| `tracing` | 0.1.44 | Application-level tracing for Rust |
| `tracing-subscriber` | 0.3.23 | Utilities for implementing and composing `tracing` subscribers |
| `uniffi` | 0.28.3 | a multi-language bindings generator for rust |

### Transitive dependencies (255)

175 crates are inherited solely via the workspace dependencies `bingle-local` / `rust_comms` — see the `rust_comms` section above for what each does. The 80 crates below are (also) pulled in by this crate's own direct dependencies:

| Crate | Version(s) | Via | What it does |
|---|---|---|---|
| `aho-corasick` | 1.1.4 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Fast multiple substring searching |
| `anstream` | 1.0.0 | `bingle-local`, `rust_comms`, `uniffi` | IO stream adapters for writing colored text that will gracefully degrade according to your terminal's capabilities |
| `anstyle` | 1.0.14 | `bingle-local`, `rust_comms`, `uniffi` | ANSI text styling |
| `anstyle-parse` | 1.0.0 | `bingle-local`, `rust_comms`, `uniffi` | Parse ANSI Style Escapes |
| `anstyle-query` | 1.1.5 | `bingle-local`, `rust_comms`, `uniffi` | Look up colored console capabilities |
| `anyhow` | 1.0.102 | `bingle-local`, `rust_comms`, `uniffi` | Flexible concrete Error type built on std::error::Error |
| `askama` | 0.12.1 | `uniffi` | Type-safe, compiled Jinja-like templates for Rust |
| `askama_derive` | 0.12.5 | `uniffi` | Procedural macro package for Askama |
| `askama_escape` | 0.10.3 | `uniffi` | Optimized HTML escaping code, extracted from Askama |
| `askama_parser` | 0.2.1 | `uniffi` | Parser for Askama templates |
| `basic-toml` | 0.1.10 | `uniffi` | Minimal TOML library with few dependencies |
| `bincode` | 1.3.3 | `uniffi` | A binary serialization / deserialization strategy that uses Serde for transforming structs into bytes and vice versa! |
| `bytes` | 1.11.1 | `bingle-local`, `rust_comms`, `uniffi` | Types and traits for working with bytes |
| `camino` | 1.2.2 | `uniffi` | UTF-8 paths |
| `cargo-platform` | 0.1.9 | `uniffi` | Cargo's representation of a target platform |
| `cargo_metadata` | 0.15.4 | `uniffi` | structured access to the output of `cargo metadata` |
| `cfg-if` | 1.0.4 | `bingle-local`, `rust_comms`, `tracing-subscriber` | A macro to ergonomically define an item depending on a large number of #[cfg] parameters. Structured like an if-else chain, the first matching branch is the item that gets emitted |
| `clap` | 4.6.1 | `uniffi` | A simple to use, efficient, and full-featured Command Line Argument Parser |
| `clap_builder` | 4.6.0 | `uniffi` | A simple to use, efficient, and full-featured Command Line Argument Parser |
| `clap_derive` | 4.6.1 | `uniffi` | Parse command line argument by defining a struct, derive crate |
| `clap_lex` | 1.1.0 | `uniffi` | Minimal, flexible command line parser |
| `colorchoice` | 1.0.5 | `bingle-local`, `rust_comms`, `uniffi` | Global override of color control |
| `core-foundation-sys` | 0.8.7 | `bingle-local`, `chrono`, `rust_comms` | Bindings to Core Foundation for macOS |
| `fs-err` | 2.11.0 | `uniffi` | A drop-in replacement for std::fs with more helpful error messages |
| `glob` | 0.3.3 | `uniffi` | Support for matching file paths against Unix shell style patterns |
| `goblin` | 0.8.2 | `uniffi` | An impish, cross-platform, ELF, Mach-o, and PE binary parsing and loading crate |
| `heck` | 0.5.0 | `uniffi` | heck is a case conversion library |
| `iana-time-zone` | 0.1.65 | `bingle-local`, `chrono`, `rust_comms` | get the IANA time zone for the current system |
| `is_terminal_polyfill` | 1.70.2 | `bingle-local`, `rust_comms`, `uniffi` | Polyfill for `is_terminal` stdlib feature for use with older MSRVs |
| `itoa` | 1.0.18 | `bingle-local`, `rust_comms`, `serde_json`, `uniffi` | Fast integer primitive to string conversion |
| `lazy_static` | 1.5.0 | `bingle-local`, `rust_comms`, `tracing-subscriber` | A macro for declaring lazily evaluated statics in Rust |
| `log` | 0.4.32 | `bingle-local`, `rust_comms`, `tracing-subscriber`, `uniffi` | A lightweight logging facade for Rust |
| `matchers` | 0.2.0 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Regex matching on character and byte streams |
| `memchr` | 2.8.1 | `bingle-local`, `rust_comms`, `serde_json`, `tracing-subscriber`, `uniffi` | Provides extremely fast (uses SIMD on x86_64, aarch64 and wasm32) routines for 1, 2 or 3 byte search and single substring search |
| `mime` | 0.3.17 | `bingle-local`, `rust_comms`, `uniffi` | Strongly Typed Mimes |
| `mime_guess` | 2.0.5 | `bingle-local`, `rust_comms`, `uniffi` | A simple crate for detection of a file's MIME type by its extension |
| `minimal-lexical` | 0.2.1 | `uniffi` | Fast float parsing conversion routines |
| `nom` | 7.1.3 | `uniffi` | A byte-oriented, zero-copy, parser combinators library |
| `nu-ansi-term` | 0.50.3 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Library for ANSI terminal colors and styles (bold, underline) |
| `num-traits` | 0.2.19 | `bingle-local`, `chrono`, `rust_comms` | Numeric traits for generic mathematics |
| `once_cell` | 1.21.4 | `bingle-local`, `rust_comms`, `tracing`, `tracing-subscriber`, `uniffi` | Single assignment cells and lazy values |
| `paste` | 1.0.15 | `bingle-local`, `rust_comms`, `uniffi` | Macros for all your token pasting needs |
| `pin-project-lite` | 0.2.17 | `bingle-local`, `rust_comms`, `tracing`, `tracing-subscriber` | A lightweight version of pin-project written with declarative macros |
| `plain` | 0.2.3 | `uniffi` | A small Rust library that allows users to reinterpret data of certain types safely |
| `proc-macro2` | 1.0.106 | `bingle-local`, `chrono`, `rust_comms`, `thiserror`, `tracing`, `tracing-subscriber`, `uniffi` | A substitute implementation of the compiler's `proc_macro` API to decouple token-based libraries from the procedural macro use case |
| `quote` | 1.0.45 | `bingle-local`, `chrono`, `rust_comms`, `thiserror`, `tracing`, `tracing-subscriber`, `uniffi` | Quasi-quoting macro quote!(...) |
| `regex-automata` | 0.4.14 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Automata construction and matching using regular expressions |
| `regex-syntax` | 0.8.10 | `bingle-local`, `rust_comms`, `tracing-subscriber` | A regular expression parser |
| `scroll` | 0.12.0 | `uniffi` | A suite of powerful, extensible, generic, endian-aware Read/Write traits for byte buffers |
| `scroll_derive` | 0.12.1 | `uniffi` | A macros 1.1 derive implementation for Pread and Pwrite traits from the scroll crate |
| `semver` | 1.0.28 | `uniffi` | Parser and evaluator for Cargo's flavor of Semantic Versioning |
| `serde` | 1.0.228 | `bingle-local`, `chrono`, `rust_comms`, `uniffi` | A generic serialization/deserialization framework |
| `serde_core` | 1.0.228 | `bingle-local`, `chrono`, `rust_comms`, `serde_json`, `uniffi` | Serde traits only, with no support for derive -- use the `serde` crate instead |
| `serde_derive` | 1.0.228 | `bingle-local`, `chrono`, `rust_comms`, `uniffi` | Macros 1.1 implementation of #[derive(Serialize, Deserialize)] |
| `sharded-slab` | 0.1.7 | `bingle-local`, `rust_comms`, `tracing-subscriber` | A lock-free concurrent slab |
| `siphasher` | 0.3.11 | `uniffi` | SipHash-2-4, SipHash-1-3 and 128-bit variants in pure Rust |
| `smallvec` | 1.15.1 | `bingle-local`, `rust_comms`, `tracing-subscriber` | 'Small vector' optimization: store up to a small number of items on the stack |
| `smawk` | 0.3.2 | `uniffi` | Functions for finding row-minima in a totally monotone matrix |
| `static_assertions` | 1.1.0 | `bingle-local`, `rust_comms`, `uniffi` | Compile-time assertions to ensure that invariants are met |
| `strsim` | 0.11.1 | `bingle-local`, `rust_comms`, `uniffi` | Implementations of string similarity metrics. Includes Hamming, Levenshtein, OSA, Damerau-Levenshtein, Jaro, Jaro-Winkler, and Sørensen-Dice |
| `syn` | 2.0.117 | `bingle-local`, `chrono`, `rust_comms`, `thiserror`, `tracing`, `tracing-subscriber`, `uniffi` | Parser for Rust source code |
| `textwrap` | 0.16.2 | `uniffi` | Library for word wrapping, indenting, and dedenting strings. Has optional support for Unicode and emojis as well as machine hyphenation |
| `thiserror-impl` | 1.0.69, 2.0.18 | `bingle-local`, `rust_comms`, `thiserror`, `uniffi` | Implementation detail of the `thiserror` crate |
| `thread_local` | 1.1.9 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Per-object thread-local storage |
| `toml` | 0.5.11 | `uniffi` | A native Rust encoder and decoder of TOML-formatted files and streams. Provides implementations of the standard Serialize/Deserialize traits for TOML data to facilitate deserializing and serializing Rust structures |
| `tracing-attributes` | 0.1.31 | `bingle-local`, `rust_comms`, `tracing`, `tracing-subscriber` | Procedural macro attributes for automatically instrumenting functions |
| `tracing-core` | 0.1.36 | `bingle-local`, `rust_comms`, `tracing`, `tracing-subscriber` | Core primitives for application-level tracing |
| `tracing-log` | 0.2.0 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Provides compatibility between `tracing` and the `log` crate |
| `unicase` | 2.9.0 | `bingle-local`, `rust_comms`, `uniffi` | A case-insensitive wrapper around strings |
| `unicode-ident` | 1.0.24 | `bingle-local`, `chrono`, `rust_comms`, `thiserror`, `tracing`, `tracing-subscriber`, `uniffi` | Determine whether characters have the XID_Start or XID_Continue properties according to Unicode Standard Annex #31 |
| `uniffi_bindgen` | 0.28.3 | `uniffi` | a multi-language bindings generator for rust (codegen and cli tooling) |
| `uniffi_checksum_derive` | 0.28.3 | `uniffi` | a multi-language bindings generator for rust (checksum custom derive) |
| `uniffi_core` | 0.28.3 | `uniffi` | a multi-language bindings generator for rust (runtime support code) |
| `uniffi_macros` | 0.28.3 | `uniffi` | a multi-language bindings generator for rust (convenience macros) |
| `uniffi_meta` | 0.28.3 | `uniffi` | uniffi_meta |
| `uniffi_testing` | 0.28.3 | `uniffi` | a multi-language bindings generator for rust (testing helpers) |
| `uniffi_udl` | 0.28.3 | `uniffi` | udl parsing for the uniffi project |
| `utf8parse` | 0.2.2 | `bingle-local`, `rust_comms`, `uniffi` | Table-driven UTF-8 parser |
| `weedle2` | 5.0.0 | `uniffi` | A WebIDL Parser |
| `zmij` | 1.0.21 | `bingle-local`, `rust_comms`, `serde_json`, `uniffi` | A double-to-string conversion algorithm based on Schubfach and yy |

## bingle_webserver

Axum-based web server exposing the Bingle engine over HTTP/WebSocket.

### Direct dependencies

| Crate | Version | What it does |
|---|---|---|
| `anyhow` | 1.0.102 | Flexible concrete Error type built on std::error::Error |
| `axum` | 0.7.9 | Web framework that focuses on ergonomics and modularity |
| `bingle-local` | 0.0.1 | Local API crate for storing messages and contacts for Bingle |
| `rust_comms` | 0.1.0 | Core Bingle comms library: P2P messaging engine (DTLS, STUN, Algorand integration) |
| `serde` | 1.0.228 | A generic serialization/deserialization framework |
| `serde_json` | 1.0.150 | A JSON serialization file format |
| `tokio` | 1.52.3 | An event-driven, non-blocking I/O platform for writing asynchronous I/O backed applications |
| `tower` | 0.5.3 | Tower is a library of modular and reusable components for building robust clients and servers |
| `tower-http` | 0.5.2 | Tower middleware and utilities for HTTP clients and servers |
| `tracing` | 0.1.44 | Application-level tracing for Rust |
| `tracing-subscriber` | 0.3.23 | Utilities for implementing and composing `tracing` subscribers |

### Transitive dependencies (231)

132 crates are inherited solely via the workspace dependencies `bingle-local` / `rust_comms` — see the `rust_comms` section above for what each does. The 99 crates below are (also) pulled in by this crate's own direct dependencies:

| Crate | Version(s) | Via | What it does |
|---|---|---|---|
| `aho-corasick` | 1.1.4 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Fast multiple substring searching |
| `async-trait` | 0.1.89 | `axum`, `bingle-local`, `rust_comms` | Type erasure for async trait methods |
| `atomic-waker` | 1.1.2 | `axum`, `bingle-local`, `rust_comms` | A synchronization primitive for task wakeup |
| `axum-core` | 0.4.5 | `axum` | Core types and traits for axum |
| `base64` | 0.21.7, 0.22.1 | `axum`, `bingle-local`, `rust_comms` | encodes and decodes base64 as bytes or utf8 |
| `bitflags` | 2.13.0 | `axum`, `bingle-local`, `rust_comms`, `tower-http` | A macro to generate structures which behave like bitflags |
| `block-buffer` | 0.10.4, 0.12.0 | `axum`, `bingle-local`, `rust_comms` | Buffer types for block processing of data |
| `byteorder` | 1.5.0 | `axum`, `bingle-local`, `rust_comms` | Library for reading/writing numbers in big-endian and little-endian |
| `bytes` | 1.11.1 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower`, `tower-http` | Types and traits for working with bytes |
| `cfg-if` | 1.0.4 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower`, `tracing-subscriber` | A macro to ergonomically define an item depending on a large number of #[cfg] parameters. Structured like an if-else chain, the first matching branch is the item that gets emitted |
| `core-foundation` | 0.10.1, 0.9.4 | `axum`, `bingle-local`, `rust_comms` | Bindings to Core Foundation for macOS |
| `core-foundation-sys` | 0.8.7 | `axum`, `bingle-local`, `rust_comms` | Bindings to Core Foundation for macOS |
| `cpufeatures` | 0.2.17, 0.3.0 | `axum`, `bingle-local`, `rust_comms` | Lightweight runtime CPU feature detection for aarch64, loongarch64, and x86/x86_64 targets, with no_std support and support for mobile targets including Android and iOS |
| `crypto-common` | 0.1.7, 0.2.2 | `axum`, `bingle-local`, `rust_comms` | Common traits used by cryptographic algorithms |
| `data-encoding` | 2.11.0 | `axum`, `bingle-local`, `rust_comms` | Efficient and customizable data-encoding functions like base64, base32, and hex |
| `digest` | 0.10.7, 0.11.3 | `axum`, `bingle-local`, `rust_comms` | Traits for cryptographic hash functions and message authentication codes |
| `equivalent` | 1.0.2 | `axum`, `bingle-local`, `rust_comms` | Traits for key comparison in maps |
| `errno` | 0.3.14 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower` | Cross-platform interface to the `errno` variable |
| `fnv` | 1.0.7 | `axum`, `bingle-local`, `rust_comms` | Fowler–Noll–Vo hash function |
| `form_urlencoded` | 1.2.2 | `axum`, `bingle-local`, `rust_comms` | Parser and serializer for the application/x-www-form-urlencoded syntax, as used by HTML forms |
| `futures-channel` | 0.3.32 | `axum`, `bingle-local`, `rust_comms` | Channels for asynchronous communication using futures-rs |
| `futures-core` | 0.3.32 | `axum`, `bingle-local`, `rust_comms`, `tower`, `tower-http` | The core traits and types in for the `futures` library |
| `futures-sink` | 0.3.32 | `axum`, `bingle-local`, `rust_comms`, `tower` | The asynchronous `Sink` trait for the futures-rs library |
| `futures-task` | 0.3.32 | `axum`, `bingle-local`, `rust_comms`, `tower` | Tools for working with tasks |
| `futures-util` | 0.3.32 | `axum`, `bingle-local`, `rust_comms`, `tower` | Common utilities and extension traits for the futures-rs library |
| `generic-array` | 0.14.7 | `axum`, `bingle-local`, `rust_comms` | Generic types implementing functionality of arrays |
| `getrandom` | 0.2.17, 0.3.4, 0.4.2 | `axum`, `bingle-local`, `rust_comms` | A small cross-platform library for retrieving random data from system source |
| `h2` | 0.4.14 | `axum`, `bingle-local`, `rust_comms` | An HTTP/2 client and server |
| `hashbrown` | 0.17.1 | `axum`, `bingle-local`, `rust_comms` | A Rust port of Google's SwissTable hash map |
| `http` | 1.4.1 | `axum`, `bingle-local`, `rust_comms`, `tower-http` | A set of types for representing HTTP requests and responses |
| `http-body` | 1.0.1 | `axum`, `bingle-local`, `rust_comms`, `tower-http` | Trait representing an asynchronous, streaming, HTTP request or response body |
| `http-body-util` | 0.1.3 | `axum`, `bingle-local`, `rust_comms`, `tower-http` | Combinators and adapters for HTTP request or response bodies |
| `httparse` | 1.10.1 | `axum`, `bingle-local`, `rust_comms` | A tiny, safe, speedy, zero-copy HTTP/1.x parser |
| `httpdate` | 1.0.3 | `axum`, `bingle-local`, `rust_comms` | HTTP date parsing and formatting |
| `hyper` | 1.10.1 | `axum`, `bingle-local`, `rust_comms` | A protective and efficient HTTP library for all |
| `hyper-util` | 0.1.20 | `axum`, `bingle-local`, `rust_comms` | hyper utilities |
| `indexmap` | 2.14.0 | `axum`, `bingle-local`, `rust_comms` | A hash table with consistent order and fast iteration |
| `ipnet` | 2.12.0 | `axum`, `bingle-local`, `rust_comms` | Provides types and useful methods for working with IPv4 and IPv6 network addresses, commonly called IP prefixes. The new `IpNet`, `Ipv4Net`, and `Ipv6Net` types build on the existing `IpAddr`, `Ipv4Addr`, and `Ipv6Addr` types already provided in Rust's standard library and align to their design to stay consistent. The module also provides useful traits that extend `Ipv4Addr` and `Ipv6Addr` with methods for `Add`, `Sub`, `BitAnd`, and `BitOr` operations. The module only uses stable feature so it is guaranteed to compile using the stable toolchain |
| `itoa` | 1.0.18 | `axum`, `bingle-local`, `rust_comms`, `serde_json`, `tower-http` | Fast integer primitive to string conversion |
| `lazy_static` | 1.5.0 | `bingle-local`, `rust_comms`, `tracing-subscriber` | A macro for declaring lazily evaluated statics in Rust |
| `libc` | 0.2.186 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower` | Raw FFI bindings to platform libraries like libc |
| `lock_api` | 0.4.14 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower` | Wrappers to create fully-featured Mutex and RwLock types. Compatible with no_std |
| `log` | 0.4.32 | `axum`, `bingle-local`, `rust_comms`, `tower`, `tracing`, `tracing-subscriber` | A lightweight logging facade for Rust |
| `matchers` | 0.2.0 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Regex matching on character and byte streams |
| `matchit` | 0.7.3 | `axum` | A high performance, zero-copy URL router |
| `memchr` | 2.8.1 | `axum`, `bingle-local`, `rust_comms`, `serde_json`, `tracing-subscriber` | Provides extremely fast (uses SIMD on x86_64, aarch64 and wasm32) routines for 1, 2 or 3 byte search and single substring search |
| `mime` | 0.3.17 | `axum`, `bingle-local`, `rust_comms` | Strongly Typed Mimes |
| `mio` | 1.2.1 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower` | Lightweight non-blocking I/O |
| `nu-ansi-term` | 0.50.3 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Library for ANSI terminal colors and styles (bold, underline) |
| `once_cell` | 1.21.4 | `axum`, `bingle-local`, `rust_comms`, `tower`, `tracing`, `tracing-subscriber` | Single assignment cells and lazy values |
| `parking_lot` | 0.12.5 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower` | More compact and efficient implementations of the standard synchronization primitives |
| `parking_lot_core` | 0.9.12 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower` | An advanced API for creating custom synchronization primitives |
| `percent-encoding` | 2.3.2 | `axum`, `bingle-local`, `rust_comms` | Percent encoding and decoding |
| `pin-project-lite` | 0.2.17 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower`, `tower-http`, `tracing`, `tracing-subscriber` | A lightweight version of pin-project written with declarative macros |
| `ppv-lite86` | 0.2.21 | `axum`, `bingle-local`, `rust_comms` | Cross-platform cryptography-oriented low-level SIMD library |
| `proc-macro2` | 1.0.106 | `axum`, `bingle-local`, `rust_comms`, `serde`, `tokio`, `tower`, `tracing`, `tracing-subscriber` | A substitute implementation of the compiler's `proc_macro` API to decouple token-based libraries from the procedural macro use case |
| `quote` | 1.0.45 | `axum`, `bingle-local`, `rust_comms`, `serde`, `tokio`, `tower`, `tracing`, `tracing-subscriber` | Quasi-quoting macro quote!(...) |
| `rand` | 0.8.6, 0.9.4 | `axum`, `bingle-local`, `rust_comms` | Random number generators and other randomness functionality |
| `rand_chacha` | 0.3.1, 0.9.0 | `axum`, `bingle-local`, `rust_comms` | ChaCha random number generator |
| `rand_core` | 0.6.4, 0.9.5 | `axum`, `bingle-local`, `rust_comms` | Core random number generator traits and tools for implementation |
| `regex-automata` | 0.4.14 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Automata construction and matching using regular expressions |
| `regex-syntax` | 0.8.10 | `bingle-local`, `rust_comms`, `tracing-subscriber` | A regular expression parser |
| `rustversion` | 1.0.22 | `axum` | Conditional compilation according to rustc compiler version |
| `ryu` | 1.0.23 | `axum`, `bingle-local`, `rust_comms` | Fast floating point to string conversion |
| `scopeguard` | 1.2.0 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower` | A RAII scope guard that will run a given closure when it goes out of scope, even if the code between panics (assuming unwinding panic). Defines the macros `defer!`, `defer_on_unwind!`, `defer_on_success!` as shorthands for guards with one of the implemented strategies |
| `serde_core` | 1.0.228 | `axum`, `bingle-local`, `rust_comms`, `serde`, `serde_json` | Serde traits only, with no support for derive -- use the `serde` crate instead |
| `serde_derive` | 1.0.228 | `axum`, `bingle-local`, `rust_comms`, `serde` | Macros 1.1 implementation of #[derive(Serialize, Deserialize)] |
| `serde_path_to_error` | 0.1.20 | `axum` | Path to the element that failed to deserialize |
| `serde_urlencoded` | 0.7.1 | `axum`, `bingle-local`, `rust_comms` | `x-www-form-urlencoded` meets Serde |
| `sha1` | 0.10.6 | `axum`, `bingle-local`, `rust_comms` | SHA-1 hash function |
| `sharded-slab` | 0.1.7 | `bingle-local`, `rust_comms`, `tracing-subscriber` | A lock-free concurrent slab |
| `signal-hook-registry` | 1.4.8 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower` | Backend crate for signal-hook |
| `slab` | 0.4.12 | `axum`, `bingle-local`, `rust_comms`, `tower` | Pre-allocated storage for a uniform data type |
| `smallvec` | 1.15.1 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower`, `tracing-subscriber` | 'Small vector' optimization: store up to a small number of items on the stack |
| `socket2` | 0.6.4 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower` | Utilities for handling networking sockets with a maximal amount of configuration possible intended |
| `subtle` | 2.6.1 | `axum`, `bingle-local`, `rust_comms` | Pure-Rust traits and utilities for constant-time cryptographic implementations |
| `syn` | 2.0.117 | `axum`, `bingle-local`, `rust_comms`, `serde`, `tokio`, `tower`, `tracing`, `tracing-subscriber` | Parser for Rust source code |
| `sync_wrapper` | 1.0.2 | `axum`, `bingle-local`, `rust_comms`, `tower` | A tool for enlisting the compiler's help in proving the absence of concurrency |
| `system-configuration` | 0.7.0 | `axum`, `bingle-local`, `rust_comms` | Bindings to SystemConfiguration framework for macOS |
| `system-configuration-sys` | 0.6.0 | `axum`, `bingle-local`, `rust_comms` | Low level bindings to SystemConfiguration framework for macOS |
| `thiserror` | 1.0.69, 2.0.18 | `axum`, `bingle-local`, `rust_comms` | derive(Error) |
| `thiserror-impl` | 1.0.69, 2.0.18 | `axum`, `bingle-local`, `rust_comms` | Implementation detail of the `thiserror` crate |
| `thread_local` | 1.1.9 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Per-object thread-local storage |
| `tokio-macros` | 2.7.0 | `axum`, `bingle-local`, `rust_comms`, `tokio`, `tower` | Tokio's proc macros |
| `tokio-tungstenite` | 0.24.0 | `axum` | Tokio binding for Tungstenite, the Lightweight stream-based WebSocket implementation |
| `tokio-util` | 0.7.18 | `axum`, `bingle-local`, `rust_comms` | Additional utilities for working with Tokio |
| `tower-layer` | 0.3.3 | `axum`, `bingle-local`, `rust_comms`, `tower`, `tower-http` | Decorates a `Service` to allow easy composition between `Service`s |
| `tower-service` | 0.3.3 | `axum`, `bingle-local`, `rust_comms`, `tower`, `tower-http` | Trait representing an asynchronous, request / response based, client or server |
| `tracing-attributes` | 0.1.31 | `axum`, `bingle-local`, `rust_comms`, `tower`, `tracing`, `tracing-subscriber` | Procedural macro attributes for automatically instrumenting functions |
| `tracing-core` | 0.1.36 | `axum`, `bingle-local`, `rust_comms`, `tower`, `tracing`, `tracing-subscriber` | Core primitives for application-level tracing |
| `tracing-log` | 0.2.0 | `bingle-local`, `rust_comms`, `tracing-subscriber` | Provides compatibility between `tracing` and the `log` crate |
| `try-lock` | 0.2.5 | `axum`, `bingle-local`, `rust_comms` | A lightweight atomic lock |
| `tungstenite` | 0.24.0 | `axum` | Lightweight stream-based WebSocket implementation |
| `typenum` | 1.20.1 | `axum`, `bingle-local`, `rust_comms` | Typenum is a Rust library for type-level numbers evaluated at compile time. It currently supports bits, unsigned integers, and signed integers. It also provides a type-level array of type-level numbers, but its implementation is incomplete |
| `unicode-ident` | 1.0.24 | `axum`, `bingle-local`, `rust_comms`, `serde`, `tokio`, `tower`, `tracing`, `tracing-subscriber` | Determine whether characters have the XID_Start or XID_Continue properties according to Unicode Standard Annex #31 |
| `utf-8` | 0.7.6 | `axum` | Incremental, zero-copy UTF-8 decoding with error handling |
| `want` | 0.3.1 | `axum`, `bingle-local`, `rust_comms` | Detect when another Future wants a result |
| `zerocopy` | 0.8.50 | `axum`, `bingle-local`, `rust_comms` | Zerocopy makes zero-cost memory manipulation effortless. We write "unsafe" so you don't have to |
| `zmij` | 1.0.21 | `axum`, `bingle-local`, `rust_comms`, `serde_json` | A double-to-string conversion algorithm based on Schubfach and yy |

## Direct dev- and build-dependencies

Not part of the shipped artifacts; used for tests and builds.

### rust_comms

| Crate | Kind | Version req | What it does |
|---|---|---|---|
| `vergen` | build | ^8 | Generate 'cargo:rustc-env' instructions via 'build.rs' for use in your code via the 'env!' macro |
| `bingle_test` | dev | * | Test helper crate for Bingle |
| `ntest` | dev | ^0.9 | Testing framework for rust which enhances the built-in library with some useful features |
| `serial_test` | dev | ^3 | Allows for the creation of serialised Rust tests |
| `tempfile` | dev | ^3 | A library for managing temporary files and directories |

### bingle-local

| Crate | Kind | Version req | What it does |
|---|---|---|---|
| `tempfile` | dev | ^3 | A library for managing temporary files and directories |

### bingle_jsi

| Crate | Kind | Version req | What it does |
|---|---|---|---|
| `vergen` | build | ^8 | Generate 'cargo:rustc-env' instructions via 'build.rs' for use in your code via the 'env!' macro |
| `tempfile` | dev | ^3 | A library for managing temporary files and directories |
| `uniffi` | dev | ^0.28 | a multi-language bindings generator for rust |

### bingle_webserver

| Crate | Kind | Version req | What it does |
|---|---|---|---|
| `ntest` | dev | ^0.9 | Testing framework for rust which enhances the built-in library with some useful features |
| `reqwest` | dev | ^0.11 | higher level HTTP client library |
| `serial_test` | dev | ^3 | Allows for the creation of serialised Rust tests |
| `tempfile` | dev | ^3 | A library for managing temporary files and directories |
