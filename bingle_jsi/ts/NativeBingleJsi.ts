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

export interface Message {
  sender_handle: string;
  recipient_handles: string[];
  timestamp: number;
  text: string;
}

export interface KeypairStatusResponse {
  status: KeypairStatus;
  id: string | null;
  handle: string | null;
  required_algo: number | null;
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

// ── API interface ────────────────────────────────────────────────────

export interface BingleJsiApi {
  // Core messaging
  handleLookup(handle: string): string;
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
  registerKeypair(handle: string): boolean;
  addContact(handle: string, id: string, source: ContactSource): void;
  blockContact(id: string): void;
  removeContact(id: string): void;
  isBlocked(id: string): boolean;
  getContacts(): Contact[];
  addMessage(
    senderHandle: string,
    recipientHandles: string[],
    timestamp: number,
    text: string
  ): void;
  getMessages(): Message[];
  keypairStatus(): KeypairStatusResponse;
  save(path: string): void;
  load(path: string): void;

  // Callbacks
  setMessageCallback(callback: MessageCallback): void;
}
