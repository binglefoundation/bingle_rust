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
} from "./NativeBingleJsi";

export type {
  InetSocketAddress,
  NetworkSourceKey,
  BingleMessage,
  VersionInfo,
  Keypair,
  Contact,
  Message,
  KeypairStatusResponse,
  NatTypeResponse,
  BingleJsiConfig,
  MessageCallback,
  LogCallback,
  ListeningCallback,
  BingleJsiApi,
} from "./NativeBingleJsi";

export {
  ContactSource,
  KeypairStatus,
  NatType,
} from "./NativeBingleJsi";

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
  sendMessageToId(userId: string, message: Record<string, unknown>): Promise<boolean>;
  sendMessageToHandle(handle: string, message: Record<string, unknown>): Promise<boolean>;
  version(): Promise<{
    version: string;
    git_sha: string | null;
    build_timestamp: string;
    build_number: string;
  }>;
  queued(): Promise<Record<string, unknown>[]>;
  getNatType(): Promise<{ nat_type: string }>;
  generateKeypair(): Promise<{ id: string; passphrase: string }>;
  registerKeypair(handle: string): Promise<boolean>;
  addContact(handle: string, id: string, source: string): Promise<void>;
  blockContact(id: string): Promise<void>;
  removeContact(id: string): Promise<void>;
  isBlocked(id: string): Promise<boolean>;
  getContacts(): Promise<
    { handle: string; id: string; fields: Record<string, string> }[]
  >;
  addMessage(
    senderHandle: string,
    recipientHandles: string[],
    timestamp: number,
    text: string
  ): Promise<void>;
  getMessages(): Promise<
    {
      sender_handle: string;
      recipient_handles: string[];
      timestamp: number;
      text: string;
    }[]
  >;
  keypairStatus(): Promise<{
    status: string;
    id: string | null;
    handle: string | null;
    required_algo: number | null;
  }>;
  save(path: string): Promise<void>;
  load(path: string): Promise<void>;
  setLogCallback(logLevel: string | null): Promise<void>;
  setMessageCallback(): Promise<void>;
  setListeningCallback(): Promise<void>;
  start(): Promise<void>;
  isStarted(): Promise<boolean>;
};
