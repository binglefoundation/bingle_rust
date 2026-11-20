/**
 * React Native entry point for the Bingle JSI module.
 *
 * Re-exports all types and the native API interface. The actual native
 * binding object is loaded from the platform native module registered
 * under the name "BingleJsi" (see ios/BingleJsiBridge.swift and
 * android/.../BingleJsiModule.kt).
 */
import { NativeModules, Platform } from "react-native";

import type {
  BingleJsiConfig,
  BingleJsiApi,
  DeliveryRoute,
  HandleLookupMatch,
  NotifyRegisterRequest,
  FailureKind,
  MessagingSettings,
  KeyStorageStatus,
  StateFileStatus,
  VersionInfo,
} from "./NativeBingleJsi";

export type {
  InetSocketAddress,
  NetworkSourceKey,
  BingleMessage,
  VersionInfo,
  Keypair,
  Contact,
  HandleLookupPartialResult,
  HandleLookupMatch,
  NotifyRegisterRequest,
  Message,
  FailureKind,
  DeliveryRoute,
  KeypairStatusResponse,
  NatTypeResponse,
  MessagingSettings,
  KeyStorageStatus,
  StateFileStatus,
  BingleJsiConfig,
  MessageCallback,
  LogCallback,
  ListeningCallback,
  MailboxCheckCallback,
  MailboxCheckStage,
  PushRegistrationCallback,
  BingleJsiApi,
} from "./NativeBingleJsi";

export {
  ContactSource,
  KeypairStatus,
  NatType,
} from "./NativeBingleJsi";

/**
 * The permanent (non-retryable) failure kinds. A send that failed with any other kind is transient
 * and will keep being retried. Mirrors `SendFailureKind::is_retryable` in bingle_core (issue #99).
 */
const PERMANENT_FAILURE_KINDS: ReadonlySet<FailureKind> = new Set<FailureKind>([
  "HandleNotFound",
  "InvalidRecipientId",
  "MalformedAdvert",
  "ProtocolError",
  "Unknown",
]);

/**
 * Whether a message that failed with `kind` will keep being retried (transient) or is permanent.
 *
 * Derives the retryable/permanent split from a message's `failure_kind` instead of the value being
 * stored per message (issue #99). A `null`/`undefined` kind (pending or delivered) is not a failure,
 * so it is reported as not retryable.
 */
export function failureKindIsRetryable(
  kind: FailureKind | null | undefined
): boolean {
  if (kind == null) {
    return false;
  }
  return !PERMANENT_FAILURE_KINDS.has(kind);
}

/**
 * The raw native module registered by the platform bridge code.
 * On iOS this is registered via BingleJsiBridge.m / BingleJsiBridge.swift.
 * On Android this is registered via BingleJsiPackage / BingleJsiModule.
 *
 * All methods are promise-based (async) because they cross the native bridge.
 */
const BingleJsiNative = NativeModules.BingleJsi;

/**
 * Initialize the Bingle JSI API with the given configuration.
 *
 * This must be called before any other API method. It delegates to the
 * platform native module which in turn calls the uniffi-generated
 * `createBingleApi(config:)` function.
 *
 * @param config - Configuration object (matches BingleJsiConfig)
 * @returns A promise that resolves when initialization is complete.
 * @throws If the native module is not linked or initialization fails.
 */
export async function initBingleJsi(config: BingleJsiConfig): Promise<void> {
  if (!BingleJsiNative) {
    throw new Error(
      "BingleJsi native module not found. Ensure it is correctly linked.\n" +
        Platform.select({
          ios: "Run 'cd ios && pod install' after building the native libraries.",
          android:
            "Ensure BingleJsiPackage is added to getPackages() in your Application class.",
          default: "",
        })
    );
  }
  await BingleJsiNative.initialize(config);
}

/**
 * The BingleJsi native module proxy.
 *
 * After calling `initBingleJsi(config)`, use this object to call API methods:
 *
 * ```typescript
 * import { BingleJsi, initBingleJsi } from 'react-native-bingle-jsi';
 *
 * await initBingleJsi({ handle: 'alice', relay: false, debug: false, ... });
 * const version = await BingleJsi.version();
 * ```
 *
 * All methods return Promises because they cross the React Native bridge.
 */
export const BingleJsi = BingleJsiNative as {
  initialize(config: BingleJsiConfig): Promise<boolean>;
  handleLookup(handle: string): Promise<string>;
  handleLookupPartial(
    handle: string
  ): Promise<{ id: string; canonical_handle: string }>;
  /** Handle search (#390): up to `limit` handles starting with `prefix`, exact matches first,
   *  then the rest, each group oldest first. An empty array when nothing matches. */
  handleLookupMatches(prefix: string, limit: number): Promise<HandleLookupMatch[]>;
  sendMessageToId(userId: string, message: Record<string, unknown>): Promise<boolean>;
  sendMessageToHandle(handle: string, message: Record<string, unknown>): Promise<boolean>;
  version(): Promise<{
    version: string;
    git_sha: string | null;
    build_timestamp: string;
    build_number: string;
  }>;
  /** The version of every library module, keyed by module name (issue #144). Needs init, unlike
   *  version(). */
  getVersions(): Promise<Record<string, VersionInfo>>;
  queued(): Promise<Record<string, unknown>[]>;
  getNatType(): Promise<{ nat_type: string }>;
  generateKeypair(): Promise<{ id: string; passphrase: string }>;
  importKeypair(passphrase: string): Promise<{ id: string; passphrase: string }>;
  /** The address (`id`) and on-chain status of the account a recovery phrase belongs to, on the
   * configured network, without importing it: the stored keypair is unchanged (issue #388).
   * `status` is as for keypairStatus (`Active` when registered). Rejects an invalid phrase as
   * importKeypair does. */
  inspectPassphrase(passphrase: string): Promise<{
    status: string;
    id: string | null;
    handle: string | null;
    required_algo: number | null;
    stale: boolean;
  }>;
  registerKeypair(handle: string): Promise<boolean>;
  signNotifyEnvelope(
    route: string,
    iss: string,
    audience: string,
    token: string,
    env: string,
    topic: string | null,
    nonce: string,
    exp: number,
  ): Promise<string>;
  /** Build the signed notify-gateway `/register` body with the active keypair (#392). The nonce
   *  and `exp` are set natively once the engine's lock is held, so the envelope can't expire while
   *  the call waits. Send the result as the request's JSON body; `topic` is absent when not given.
   *  Prefer this to `signNotifyEnvelope` for registration. */
  buildNotifyRegisterRequest(
    iss: string,
    token: string,
    env: string,
    topic: string | null,
  ): Promise<NotifyRegisterRequest>;
  /** Sign the client's transactions in the registration service's app-store registration group
   * (#344) with the current keypair, after checking them against the registration template.
   * `transactions` is the service's whole unsigned group (base64 msgpack) and `signIndexes` its
   * `sign_indexes`. Resolves to the signed transactions (base64 msgpack) in `signIndexes` order,
   * for `signed_transactions`. Rejects, signing nothing, if any client transaction is unexpected. */
  signRegistrationGroup(transactions: string[], signIndexes: number[], handle: string): Promise<string[]>;
  /** Start or resume a handle clash recovery (#344): resolves to the pending recovery keypair, whose
   * id is the recovery request's `new_address`. It is saved beside the current keypair before this
   * resolves, and returned again until completeRecoveryKeypair. */
  beginRecoveryKeypair(): Promise<{ id: string; passphrase: string }>;
  /** Sign the client's transactions in a handle clash recovery group (#344): the old account's with
   * the current keypair, the new account's with the pending recovery keypair. Rejects if no
   * recovery has begun or any client transaction is unexpected. */
  signRecoveryGroup(transactions: string[], signIndexes: number[], handle: string): Promise<string[]>;
  /** Finish a handle clash recovery once the service reports it `registered`: the pending recovery
   * keypair becomes current. Rejects if no recovery is pending. */
  completeRecoveryKeypair(): Promise<{ id: string; passphrase: string }>;
  /** What the key store reports about the recovery phrase (#385): the backend, the sync choice,
   * the backup confirmation, and reinstall detection. */
  keyStorageStatus(): Promise<KeyStorageStatus>;
  /** Keep the phrase on this device only (true) or sync it through iCloud Keychain (false), and
   * record that the user has chosen (#385). Turning a synced key device-only removes it from the
   * user's other devices, so it rejects until markPhraseBackedUp. Rejects with the file store. */
  setDeviceOnly(enabled: boolean): Promise<void>;
  /** Record that the user confirmed the phrase is backed up (#385). Rejects with the file store. */
  markPhraseBackedUp(): Promise<void>;
  /** After a reinstall, make the key the Keychain still holds the current keypair, without the user
   * entering the phrase, and save (#385). Rejects with the file store or if no key is stored. */
  restoreStoredKey(): Promise<{ id: string; passphrase: string }>;
  /** Delete this device's stored keys and the state file (#385). A synced key is removed from every
   * device on the user's Apple Account, so say so first; then re-initialise. Rejects with the file
   * store. */
  removeKeyFromDevice(): Promise<void>;
  /** Delete the account (#100): opt it out of the Bingle DApp on chain, delete its stored keys,
   * contacts and messages, stop the engine and delete the state file. The handle is free at once;
   * the Algorand account, its ALGO and its Bingle$ stay. A synced key is removed from every device
   * on the user's Apple Account, so say so first; then re-initialise into first run. Works with
   * either key store. Rejects with code `BINGLE_ERROR`; the message names the cause, and every
   * cause is safe to retry (a retry resumes):
   * - `NoBlockchain`: the Algorand node can't be reached; nothing has changed.
   * - `Retryable`: the on-chain opt-out failed; nothing on this device has changed.
   * - `InternalError`: the opt-out succeeded, but deleting the stored keys or the state file
   *   failed. */
  deleteAccount(): Promise<void>;
  /** What the last load of the state file found (#459). Check it at start-up before treating an
   * empty state as a new user: "locked" means load again after the unlock, and
   * "recovery_phrase_required" means ask for the recovery phrase. */
  stateFileStatus(): Promise<StateFileStatus>;
  /** Save, then wipe the sealed state file's key from memory (#459). Call it on
   * protectedDataWillBecomeUnavailable. Saves then fail with a StateLocked error until the device
   * unlocks; nothing is lost, as the next save writes everything. */
  forgetStateKey(): Promise<void>;
  /** Open a sealed state file restored from a backup with the account's recovery phrase (#459). A
   * wrong phrase changes nothing and rejects with a message naming RecoveryPhraseRequired. On
   * success the file opens on this device from now on, its contacts and messages are loaded, and
   * the phrase's keypair is current; resolves with it. */
  openStateWithRecoveryPhrase(phrase: string): Promise<{ id: string; passphrase: string }>;
  /** The current keypair with its recovery phrase, for Export Key and the backup step (#466). Read
   * from memory, so it works with either key store and a sealed state file; the state file no
   * longer holds the phrase with the Keychain store. Rejects with InvalidRequest when there is no
   * keypair. */
  exportKeypair(): Promise<{ id: string; passphrase: string }>;
  /** Ask the host to begin iOS push registration (bingle_notify #i). Invokes the registered
   * PushRegistrationCallback so the thin Swift bridge performs the platform calls; the token arrives
   * asynchronously via registerApnsToken. Rejects if no push-registration callback is set. */
  requestPushRegistration(): Promise<void>;
  /** Hand the raw APNs device token (exactly as iOS delivered it) to Rust, which hex-encodes,
   * signs, and POSTs the /register envelope. Resolves to whether the gateway accepted it. Rejects
   * if the token is not 32 bytes, there is no keypair/handle, or no gateway URL is configured. */
  registerApnsToken(token: Uint8Array): Promise<boolean>;
  /** Report that iOS push registration failed (permission denied or APNs error); logged only. */
  apnsRegistrationFailed(reason: string): Promise<void>;
  addContact(handle: string, id: string, source: string): Promise<void>;
  blockContact(id: string): Promise<void>;
  unblockContact(id: string): Promise<void>;
  removeContact(id: string): Promise<void>;
  isBlocked(id: string): Promise<boolean>;
  getContacts(): Promise<
    { handle: string; id: string; fields: Record<string, string> }[]
  >;
  getBlockedContacts(): Promise<
    { handle: string; id: string; fields: Record<string, string> }[]
  >;
  addMessage(
    senderHandle: string,
    recipientHandles: string[],
    timestamp: number,
    text: string,
    cipherSuite?: string | null
  ): Promise<void>;
  getMessages(): Promise<
    {
      // Stable identifier (issue #209); pass it to `updateMessageStatusById`.
      id: string;
      sender_handle: string;
      recipient_handles: string[];
      /** @deprecated (issue #209) Identify a message by `id`; order by send time as getMessages does. */
      timestamp: number;
      text: string;
      cipher_suite: string | null;
      progress?: number;
      failure_reason?: string | null;
      // Typed failure cause (issue #99); absent while pending or delivered. Derive retryability
      // with `failureKindIsRetryable`.
      failure_kind?: FailureKind | null;
      // Store-and-forward carry fields (issue #204); absent for live DTLS messages.
      sent_time?: number | null;
      delivered_time?: number | null;
      signature?: string | null;
      // How the message was delivered (issue #291); null while pending or failed.
      delivery_route?: DeliveryRoute | null;
    }[]
  >;
  queueMessage(recipientHandles: string[], text: string): Promise<void>;
  /** @deprecated (issue #209) Use `updateMessageStatusById`; messages are identified by `id`. */
  updateMessageStatus(timestamp: number, progress: number, failureReason: string | null): Promise<void>;
  /** Update the status of the message with this `id` (issue #209). */
  updateMessageStatusById(id: string, progress: number, failureReason: string | null): Promise<void>;
  keypairStatus(): Promise<{
    status: string;
    id: string | null;
    handle: string | null;
    required_algo: number | null;
    /** True when status is a last-known value returned during a blockchain outage (issue #18 A2 /
     * #31), not a fresh read. Surface as "account status unavailable". */
    stale: boolean;
  }>;
  /** Whether the network is available for sending (issue #31); reflects the P2P transport only —
   * true when listening with a usable route, false when not listening or NoConnection. Independent
   * of Algorand-node reachability (messages go over relays). forceRecheck accepted for compat. */
  /** Set the store-and-forward send/receive gates on the running session (Settings screen, issue
   * #242). Takes effect on the next send/poll without a re-init; keypair, contacts, history and
   * active connections are preserved. */
  setStoreAndForward(send: boolean, receive: boolean): Promise<void>;
  /** Set the give-up notify nudge on the running session (issue #242): `enabled` toggles it and
   * `gatewayUrl` sets the notify gateway base URL (null leaves the nudge dormant even when enabled). */
  setNotify(enabled: boolean, gatewayUrl: string | null): Promise<void>;
  /** Current effective messaging settings, for the Settings screen to render live state (#242). */
  messagingSettings(): Promise<MessagingSettings>;
  networkAvailable(forceRecheck: boolean): Promise<boolean>;
  save(path: string): Promise<void>;
  load(path: string): Promise<void>;
  setLogCallback(logLevel: string | null): Promise<void>;
  setMessageCallback(): Promise<void>;
  setListeningCallback(): Promise<void>;
  /** Register for "onMailboxChecked" events, sent after each store-and-forward Mailbox check that
   * reached the node with the number of new messages it read (issue #358), and "onMailboxProgress"
   * events, sent as a check progresses (bingle_rust #447). */
  setMailboxCheckCallback(): Promise<void>;
  /** Register a callback invoked when requestPushRegistration asks the host to start iOS push
   * registration (bingle_notify #i). Replaces any previously registered callback. */
  setPushRegistrationCallback(): Promise<void>;
  start(): Promise<void>;
  stop(): Promise<void>;
  isStarted(): Promise<boolean>;
  // App lifecycle (call from the host app's AppState listener). foregrounding refreshes the relay
  // registration and polls the store-and-forward Mailbox, keeping a backstop poll running while
  // foregrounded; backgrounding stops that poll and hands undelivered messages to the Mailbox,
  // resolving once that is done (up to about 20 seconds) (issues #50, #215, #352).
  foregrounding(): Promise<void>;
  backgrounding(): Promise<void>;
};
