/**
 * TypeScript type definitions for the Bingle JSI API.
 *
 * These types mirror the uniffi Record/Enum types defined in
 * bingle_jsi/src/api/types.rs. When uniffi-bindgen generates the
 * native bindings (Swift / Kotlin), these types are the TypeScript
 * counterparts used on the React Native side.
 */

// ── Records ──────────────────────────────────────────────────────────

export interface InetSocketAddress {
  host: string;
  port: number;
}

export interface NetworkSourceKey {
  inet_socket_address: InetSocketAddress | null;
  relay_channel: number | null;
  relay_address: InetSocketAddress | null;
  relay_id: string | null;
}

export interface BingleMessage {
  app: string | null;
  type: string | null;
  tag: string | null;
  response_tag: string | null;
  text: string | null;
  data: string | null;
  /** The cipher suite negotiated for the DTLS session on which this message was received.
   * Derived by the receiving client from the connection; not transmitted on the wire. */
  cipher_suite: string | null;
}

export interface VersionInfo {
  version: string;
  git_sha: string | null;
  build_timestamp: string;
  build_number: string;
}

export interface Keypair {
  id: string;
  passphrase: string;
}

export interface Contact {
  handle: string;
  id: string;
  fields: Record<string, string>;
}

export interface HandleLookupPartialResult {
  /** Algorand address of the matching account. */
  id: string;
  /** The handle exactly as written in the account's blockchain local state. */
  canonical_handle: string;
}

/** Typed cause of a send failure (issue #99). Mirrors the Rust `FailureKind`; use this to process
 * failures reliably instead of parsing `failure_reason`. Whether a kind is retryable is derived via
 * the `failureKindIsRetryable` helper, not stored per message. */
export type FailureKind =
  | 'HandleNotFound'
  | 'HandleLookupFailed'
  | 'RecipientNotAdvertised'
  | 'InvalidRecipientId'
  | 'NoRelayAvailable'
  | 'RelayAllocationFailed'
  | 'PeerUnreachable'
  | 'NoResponse'
  | 'MalformedAdvert'
  | 'ProtocolError'
  | 'NotReady'
  | 'Unknown';

export interface Message {
  sender_handle: string;
  recipient_handles: string[];
  timestamp: number;
  text: string;
  /** The cipher suite negotiated for the DTLS session on which this message was received.
   * Derived by the receiving client from the connection; not transmitted on the wire. */
  cipher_suite: string | null;
  /** Delivery progress (0.0 to 1.0) */
  progress: number;
  /** Human-readable reason for the last failure, if any. */
  failure_reason: string | null;
  /** Typed cause of the last failure (issue #99); null while pending or delivered. Derive whether
   * it is retryable with `failureKindIsRetryable`. */
  failure_kind: FailureKind | null;
  /** Sender-stamped send time (epoch millis) from a Sidewinder store-and-forward envelope (issue
   * #204); null for a live message delivered over the Bingle DTLS session. */
  sent_time: number | null;
  /** Receiver's local clock (epoch millis) when the message was fetched from the Sidewinder Mailbox
   * (issue #204). Locally stamped, not on either transport; null for live messages. */
  delivered_time: number | null;
  /** Base64 Ed25519 sender signature retained from the store-and-forward envelope, for later report
   * attachment (issue #94); null when no signed envelope was opened. */
  signature: string | null;
}

export interface KeypairStatusResponse {
  status: KeypairStatus;
  id: string | null;
  handle: string | null;
  required_algo: number | null;
  /** True when `status` is a last-known value returned during a blockchain outage rather than a
   * fresh on-chain read (issue #18 A2 / #31). Surface as "account status unavailable". */
  stale: boolean;
}

export interface NatTypeResponse {
  nat_type: NatType;
}

export interface BingleJsiConfig {
  handle: string | null;
  passphrase: string | null;
  relay: boolean;
  static_ip: string | null;
  stun_servers: string | null;
  stun_servers_file: string | null;
  node_file: string | null;
  log_level: string | null;
  app_id: number | null;
  asset_id: number | null;
  handle_cache_expiry_secs: number | null;
  debug: boolean;
  local: string | null;
  /** Notify gateway base URL for the give-up nudge (bingle_notify #11). When set (with local mode),
   * a message give-up POSTs a content-free alert to `{url}/alert`; null leaves the nudge dormant. */
  notify_gateway_url: string | null;
  /** Override for the give-up nudge gate (bingle_notify #11). null keeps the default (enabled);
   * false disables the nudge even when a gateway URL is set. */
  notify_on_giveup: boolean | null;
  /** APNs environment this build's device tokens belong to: "sandbox" (Xcode/dev builds) or
   * "production" (TestFlight/App Store). Used as the `env` when registering the device token via
   * /register (bingle_notify #i). null defaults to "sandbox". */
  notify_env: string | null;
  /** Base URL of the Sidewinder node for store-and-forward (epic #200), e.g. `http://host:9101`.
   * When set together with `sidewinder_token` (and local mode), the offline path can post to and
   * read from the recipient Mailbox; null/omitted leaves store-and-forward unconfigured. Optional:
   * a client that does not use store-and-forward need not set it. */
  sidewinder_node_url?: string | null;
  /** Bearer token for the Sidewinder node's client endpoints (the v0.0.2 fixed shared token,
   * Sidewinder #164). Required alongside `sidewinder_node_url`; null/omitted leaves store-and-forward
   * unconfigured. Optional: a client that does not use store-and-forward need not set it. */
  sidewinder_token?: string | null;
  /** Send-side store-and-forward gate (epic #200): when true, a give-up on direct delivery posts the
   * sealed message to the recipient's Sidewinder Mailbox (#214). null/omitted defaults to false (off).
   * Independent of `store_and_forward_receive`; also needs the sidewinder_* fields configured.
   * Optional: a client that does not use store-and-forward need not set it. */
  store_and_forward_send?: boolean | null;
  /** Receive-side store-and-forward gate (epic #200): when true, the client polls its own Sidewinder
   * Mailbox on reconnect and on a cadence, reading messages forwarded to it (#215). null/omitted
   * defaults to false (off). Independent of `store_and_forward_send`. Optional: a client that does not
   * use store-and-forward need not set it. */
  store_and_forward_receive?: boolean | null;
}

// ── Enums ────────────────────────────────────────────────────────────

export enum ContactSource {
  Manual = "Manual",
  Received = "Received",
}

export enum KeypairStatus {
  None = "None",
  Unfunded = "Unfunded",
  Funded = "Funded",
  Active = "Active",
  UpgradeRequired = "UpgradeRequired",
}

export enum NatType {
  Unknown = "Unknown",
  NoConnection = "NoConnection",
  Symmetric = "Symmetric",
  Restricted = "Restricted",
  FullCone = "FullCone",
}

// ── Callback interface ───────────────────────────────────────────────

export interface MessageCallback {
  onMessage(senderId: string, senderHandle: string, message: BingleMessage): void;
}

export interface LogCallback {
  onLog(timestamp: number, level: string, message: string): void;
}

export interface ListeningCallback {
  onListening(listening: boolean, natType: string): void;
}

export interface PushRegistrationCallback {
  /** Called when the host should ask iOS for an APNs device token (permission prompt + register).
   * The thin Swift bridge does the platform calls; the token comes back via registerApnsToken. */
  onRequestRegistration(): void;
}

// ── API interface ────────────────────────────────────────────────────

export interface BingleJsiApi {
  // Core messaging
  handleLookup(handle: string): string;
  handleLookupPartial(handle: string): HandleLookupPartialResult;
  sendMessageToId(userId: string, message: BingleMessage): boolean;
  sendMessageToHandle(handle: string, message: BingleMessage): boolean;
  sendMessageToNetwork(
    networkSourceKey: NetworkSourceKey,
    userId: string,
    message: BingleMessage
  ): boolean;
  sendMessageToIdWithResponse(userId: string, message: BingleMessage): BingleMessage;
  sendMessageToHandleWithResponse(handle: string, message: BingleMessage): BingleMessage;
  sendMessageToNetworkWithResponse(
    networkSourceKey: NetworkSourceKey,
    userId: string,
    message: BingleMessage
  ): BingleMessage;
  queued(): BingleMessage[];
  version(): VersionInfo;
  getNatType(): NatTypeResponse;

  // Local storage and contacts
  generateKeypair(): Keypair;
  importKeypair(passphrase: string): Keypair;
  registerKeypair(handle: string): boolean;
  signNotifyEnvelope(
    route: string,
    iss: string,
    audience: string,
    token: string,
    env: string,
    nonce: string,
    exp: number,
  ): string;
  requestPushRegistration(): void;
  registerApnsToken(token: Uint8Array): boolean;
  apnsRegistrationFailed(reason: string): void;
  addContact(handle: string, id: string, source: ContactSource): void;
  blockContact(id: string): void;
  removeContact(id: string): void;
  isBlocked(id: string): boolean;
  getContacts(): Contact[];
  addMessage(
    senderHandle: string,
    recipientHandles: string[],
    timestamp: number,
    text: string,
    cipher_suite: string | null
  ): void;
  getMessages(): Message[];
  /** Drain this user's Sidewinder Mailbox, decrypting and storing each held store-and-forward
   * message, and return the batch read this poll (sorted by sent time). Empty when store-and-forward
   * receive is off / no node is configured. Call on start and on a cadence; read messages also
   * appear in getMessages(). (store-and-forward epic #200, story #215) */
  pollMailbox(): Message[];
  queueMessage(recipientHandles: string[], text: string): void;
  updateMessageStatus(
    timestamp: number,
    progress: number,
    failureReason: string | null
  ): void;
  keypairStatus(): KeypairStatusResponse;
  /**
   * Whether the network is available for sending (issue #31). Reflects the P2P transport only:
   * true when listening with a usable route, false when not listening or NoConnection. Independent
   * of Algorand-node reachability (messages go over relays), so a node outage does not make sending
   * unavailable. forceRecheck is accepted for compatibility but not needed.
   */
  networkAvailable(forceRecheck: boolean): boolean;
  save(path: string): void;
  load(path: string): void;

  // Callbacks
  setMessageCallback(callback: MessageCallback): void;
  setLogCallback(callback: LogCallback): void;
  setListeningCallback(callback: ListeningCallback): void;
  setPushRegistrationCallback(callback: PushRegistrationCallback): void;

  // Events (emitted by the native bridge)
  // onMessage: { sender_id: string; sender_handle: string; message: BingleMessage }
  // onLog: { timestamp: number; level: string; message: string }
  // onListening: { listening: boolean; nat_type: string }

  // Engine lifecycle
  start(): void;
  stop(): void;
  isStarted(): boolean;

  // App lifecycle (call from the host app's AppState listener).
  // foregrounding refreshes the relay registration after background/idle so
  // inbound recovers immediately (issue #50); backgrounding lets the engine
  // pause battery-costly work while suspended.
  foregrounding(): void;
  backgrounding(): void;
}
