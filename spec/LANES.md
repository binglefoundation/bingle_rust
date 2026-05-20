UX:
- ~~build react UX with onboarding~~
- ~~JSI / Uniffi library interface~~
- ~~build react native UX based on messenger / signal / apple messages~~
- ~~user onboarding with held algorand~~
- ~~contact UX~~
- ~~build iOS~~ 
- ~~tidy up bugs~~
- info option to show cipher suite

security:
- ensure protocol is built as documented, changes as follows:
*   ~~Update spec to match correct message changes~~
*   ~~Update spec for issuer suffix of "."~~
*   ~~**DTLS Identity Validation**:~~
*   ~~**Spec**: Explicitly requires that the DTLS implementation "MUST check that the `id` is opted in to the Bingle DAPP and has a `Handle` field in local storage."~~
*   ~~**Code**: The `peer_certificate_handler` in `src/protocol/cert_verify.rs` only verifies the certificate signature and CN/OrganizationName alignment. It does not perform any blockchain-based opt-in or handle verification.~~
*   ~~Implement a check on incoming messages (in the Engine) to validate the id, look it up in blockchain local storage and retrieve/check the handle.~~
*   ~~**Certificate Algorithms**: Ed25519 on CA key, EC (P-256) for signing and ECDHE for protocol on server/client keys.~~
    *   ~~**Spec**: Updated to reflect EC (NIST P-256).~~
    *   ~~**Code**: Updated to use EC (P-256).~~
-*   **`DdbDumpResolve` vs. `DdbDumpResolveResponse`**: Resolve the following
    *   **Spec**: The description for `DdbInitResolve` says it is followed by `DdbDumpResolveResponse` messages.
    *   **Code**: The implementation in `src/ddb/mod.rs` sends `DdbDumpResolve` messages.
    *   **Note**: The spec contains conflicting info, describing `DdbDumpResolve` as the message carrying the record, while naming it `DdbDumpResolveResponse` in the process flow.
*   **Signature Verification**: Implement a signature over the AdvertRecord struct.
    *   **Spec**: Specifies `DdbUpsertResolve` and `DdbDeleteResolve` include an `originalSignature`.
    *   **Code**: While the fields exist in the structs, the message handlers (`on_ddb_upsert_resolve`) do not currently appear to verify these signatures, relying instead on the DTLS-provided identity.
* ~~Network Partitioning Algorithm: fixed this inconsistency in the spec~~
- ensure DAPP methods perform all required checks
- delegate admin tasks to not be creator
- ~~ensure runs in live with full encryption~~
- ~~test encryption for entropy~~
- ensure id is checked and fails on impersonation
- add a cipher suite string to messages with DTLS cipher suite
- ~~TLS1.2 vuln tests:~~
  ~~1. Protocol Downgrade Attacks~~
  ~~2. Weak Cipher Suite Acceptance~~
  ~~3. Weak Key Exchange and Small Keys~~
  ~~4. Certificate Verification Vulnerabilities (Custom Handler)~~
  ~~5. Padding Oracle Attacks (Lucky13)~~
  ~~6. Compression-Related Attacks (CRIME)~~
  ~~7. Insecure Randomness~~
  ~~8. Insecure Renegotiation~~
  ~~9. ROBOT (Return of Bleichenbacher's Oracle Threat)~~
- ~~Ensure and test we have PFS via ECDHE~~  
- ~~Test for extended master secret support~~

robustness:
- fix error 40 on live device (?)
- Refactor DTLS OpenSSL with PeerCmd to remove polling delays
- clean up duplicated code
- ensure fails result in a fail message which gets handled
- implement retry for packet loss and retryable fails (FRPT implementation, no large blocks yet)
- run command processing in a thread
- implement relay cache properly with expiry
- lookup matches handles with downcase and some punctuation normalized (as Gmail?)
- ensure ipv6 is unsupported consistently
- indicate when we get no STUN responses (UDP blocked)
- ensure handle uniqueness
- fix fragile tests
- 
network:
- handle network change and clear caches / rediscover nat type
- remove DDB entries on node stop
- handle clean relay shutdown
- cache DDB locally with timeout / cancel
- cache blockchain locally
- detect relay unresponsive from peers and remove

tokenomics:
- develop tokenomics model
- implement pricing accordingly

android:
- support NDK in library
- support Android for react native UX
- build APK
