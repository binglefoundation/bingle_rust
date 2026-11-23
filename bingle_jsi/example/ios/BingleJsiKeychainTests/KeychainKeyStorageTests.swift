import Security
import XCTest
@testable import bingle_jsiFFI

// MARK: - The real Keychain (bingle_rust #385)

/// The Keychain key store end to end, through the real Rust library: device-only creation, the sync
/// choice, the backup check, reinstall restore, removal and migration. Runs on a local Mac's
/// simulator (never in continuous integration). This target is hosted in the example app: the
/// simulator Keychain refuses a hostless test bundle (errSecMissingEntitlement, -34018).
final class KeychainKeyStorageTests: XCTestCase {

    private var stateDir: URL!
    private var statePath: URL!
    private var apis: [any BingleJsiApiProtocol] = []

    /// The service the Rust store uses: the main bundle id with `.bingle`, or `bingle`.
    private var service: String {
        if let id = Bundle.main.bundleIdentifier { return "\(id).bingle" }
        return "bingle"
    }

    private var keyAccount: String {
        "key/\(statePath.deletingPathExtension().lastPathComponent)"
    }

    /// The sealed state file's device secret (bingle_rust #459).
    private var stateAccount: String {
        "state/\(statePath.deletingPathExtension().lastPathComponent)"
    }

    override func setUp() {
        super.setUp()
        stateDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("bingle-keychain-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: stateDir, withIntermediateDirectories: true)
        statePath = stateDir.appendingPathComponent("bingle_state_xctest.json")
    }

    override func tearDown() {
        for api in apis { try? api.removeKeyFromDevice() }
        apis = []
        deleteItem(synced: false)
        deleteItem(synced: true)
        try? FileManager.default.removeItem(at: stateDir)
        super.tearDown()
    }

    private func makeApi(keyStorage: String = "keychain", encryptState: Bool? = nil) throws -> any BingleJsiApiProtocol {
        let config = BingleJsiConfig(
            handle: nil, passphrase: nil, relay: false, staticIp: nil, stunServers: nil,
            stunServersFile: nil, nodeFile: nil, logLevel: nil, appId: nil, assetId: nil,
            handleCacheExpirySecs: nil, debug: false, local: statePath.path,
            notifyGatewayUrl: nil, notifyOnGiveup: nil, notifyEnv: nil,
            storeAndForwardSend: nil, storeAndForwardReceive: nil,
            storeAndForwardPollIntervalSecs: nil, sendPendingMessages: nil, keyStorage: keyStorage,
            encryptState: encryptState, enableGlobalBlock: nil
        )
        let api = try createBingleApi(config: config)
        apis.append(api)
        return api
    }

    /// Whether the key item exists with the given sync setting, failing on a Keychain error.
    private func itemExists(synced: Bool, file: StaticString = #filePath, line: UInt = #line) -> Bool {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: keyAccount,
            kSecAttrSynchronizable as String: synced,
            kSecUseDataProtectionKeychain as String: true,
        ]
        let status = SecItemCopyMatching(query as CFDictionary, nil)
        if status == errSecMissingEntitlement {
            XCTFail("the Keychain refused this test bundle (-34018): it must run hosted in the example app", file: file, line: line)
        }
        XCTAssertTrue(status == errSecSuccess || status == errSecItemNotFound, "Keychain status \(status)", file: file, line: line)
        return status == errSecSuccess
    }

    private func deleteItem(synced: Bool) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: keyAccount,
            kSecAttrSynchronizable as String: synced,
            kSecUseDataProtectionKeychain as String: true,
        ]
        SecItemDelete(query as CFDictionary)
    }

    /// The device secret item's accessibility, or nil if there is none (unsynced items only).
    private func stateSecretAccessibility() -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: stateAccount,
            kSecAttrSynchronizable as String: false,
            kSecUseDataProtectionKeychain as String: true,
            kSecReturnAttributes as String: true,
        ]
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let attributes = result as? [String: Any] else { return nil }
        return attributes[kSecAttrAccessible as String] as? String
    }

    private func stateFileText() -> String {
        (try? String(contentsOf: statePath, encoding: .utf8)) ?? ""
    }

    func testNewKeyIsDeviceOnlyUntilTheUserChooses() throws {
        let api = try makeApi()
        let kp = try api.generateKeypair()
        let status = try api.keyStorageStatus()
        XCTAssertEqual(status.backend, .keychain)
        XCTAssertTrue(status.deviceOnly)
        XCTAssertTrue(status.syncChoicePending)
        XCTAssertEqual(status.storedAddress, kp.id)
        XCTAssertTrue(itemExists(synced: false))
        XCTAssertFalse(itemExists(synced: true), "nothing synced before the user chooses")
        XCTAssertFalse(stateFileText().contains(kp.passphrase), "no phrase in the state file")
        XCTAssertTrue(stateFileText().contains(kp.id))
    }

    func testChoosingSyncThenDeviceOnlyNeedsTheBackupConfirmed() throws {
        let api = try makeApi()
        _ = try api.generateKeypair()

        try api.setDeviceOnly(enabled: false)
        XCTAssertTrue(itemExists(synced: true))
        XCTAssertFalse(itemExists(synced: false))
        XCTAssertFalse(try api.keyStorageStatus().syncChoicePending)

        XCTAssertThrowsError(try api.setDeviceOnly(enabled: true), "refused before the backup is confirmed")
        XCTAssertTrue(itemExists(synced: true), "unchanged after the refusal")

        try api.markPhraseBackedUp()
        try api.setDeviceOnly(enabled: true)
        XCTAssertTrue(itemExists(synced: false))
        XCTAssertFalse(itemExists(synced: true))
        XCTAssertTrue(try api.keyStorageStatus().deviceOnly)
    }

    func testANewKeyDoesNotSilentlyReplaceASyncedKey() throws {
        let api = try makeApi()
        let synced = try api.generateKeypair()
        try api.setDeviceOnly(enabled: false)
        XCTAssertTrue(itemExists(synced: true))

        XCTAssertThrowsError(try api.generateKeypair()) { error in
            guard case BingleJsiError.SyncedKeyWouldBeReplaced(let storedAddress) = error else {
                return XCTFail("unexpected error \(error)")
            }
            XCTAssertEqual(storedAddress, synced.id)
        }
        XCTAssertTrue(itemExists(synced: true), "the synced key is untouched")
        XCTAssertEqual(try api.keyStorageStatus().storedAddress, synced.id)

        let replacement = try api.generateKeypairReplacing(replaceSyncedKey: true)
        XCTAssertFalse(itemExists(synced: true), "replacing removes the synced item")
        XCTAssertTrue(itemExists(synced: false))
        let status = try api.keyStorageStatus()
        XCTAssertEqual(status.storedAddress, replacement.id)
        XCTAssertTrue(status.syncChoicePending)
    }

    func testAReinstallRestoresTheStoredKey() throws {
        let first = try makeApi()
        let kp = try first.generateKeypair()

        // an uninstall removes the app container (state file and install marker), not the Keychain.
        try FileManager.default.removeItem(at: stateDir)
        try FileManager.default.createDirectory(at: stateDir, withIntermediateDirectories: true)

        let reinstalled = try makeApi()
        let status = try reinstalled.keyStorageStatus()
        XCTAssertTrue(status.reinstallDetected)
        XCTAssertEqual(status.storedAddress, kp.id)
        let restored = try reinstalled.restoreStoredKey()
        XCTAssertEqual(restored.id, kp.id)
        XCTAssertEqual(restored.passphrase, kp.passphrase)
    }

    func testRemoveKeyFromDeviceDeletesTheItemsAndTheStateFile() throws {
        let api = try makeApi()
        _ = try api.generateKeypair()
        XCTAssertTrue(FileManager.default.fileExists(atPath: statePath.path))

        try api.removeKeyFromDevice()
        XCTAssertFalse(itemExists(synced: false))
        XCTAssertFalse(itemExists(synced: true))
        XCTAssertFalse(FileManager.default.fileExists(atPath: statePath.path))
        XCTAssertNil(try api.keyStorageStatus().storedAddress)
    }

    func testAnExistingStateFileMigratesToADeviceOnlyItem() throws {
        let old = try makeApi(keyStorage: "file")
        let kp = try old.generateKeypair()
        XCTAssertTrue(stateFileText().contains(kp.passphrase), "today's state file holds the phrase")

        let migrated = try makeApi()
        XCTAssertFalse(stateFileText().contains(kp.passphrase), "rewritten without the phrase")
        XCTAssertTrue(itemExists(synced: false))
        XCTAssertFalse(itemExists(synced: true))
        let status = try migrated.keyStorageStatus()
        XCTAssertTrue(status.syncChoicePending)
        XCTAssertEqual(status.storedAddress, kp.id)
    }

    // MARK: - The sealed state file (bingle_rust #459)

    func testASealedStateFileHoldsNothingReadableAndReopens() throws {
        let api = try makeApi(encryptState: true)
        let kp = try api.generateKeypair()
        try api.addContact(handle: "friend", id: "FRIEND_ID", source: .manual)
        try api.save(path: statePath.path)

        let bytes = try Data(contentsOf: statePath)
        for text in [kp.id, kp.passphrase, "FRIEND_ID", "friend"] {
            XCTAssertNil(bytes.range(of: Data(text.utf8)), "'\(text)' is readable in the file")
        }
        XCTAssertEqual(stateSecretAccessibility(), kSecAttrAccessibleWhenUnlockedThisDeviceOnly as String)

        let reopened = try makeApi(encryptState: true)
        XCTAssertEqual(try reopened.stateFileStatus(), .sealed)
        XCTAssertEqual(try reopened.getContacts().map(\.id), ["FRIEND_ID"])
    }

    func testAPlainStateFileIsSealedOnFirstLoad() throws {
        let old = try makeApi()
        let kp = try old.generateKeypair()
        XCTAssertTrue(stateFileText().contains(kp.id), "plain before")

        let sealed = try makeApi(encryptState: true)
        XCTAssertEqual(try sealed.stateFileStatus(), .sealed)
        XCTAssertFalse(stateFileText().contains(kp.id), "sealed after")
    }

    func testRemoveKeyFromDeviceDeletesTheStateSecret() throws {
        let api = try makeApi(encryptState: true)
        _ = try api.generateKeypair()
        XCTAssertNotNil(stateSecretAccessibility())

        try api.removeKeyFromDevice()
        XCTAssertNil(stateSecretAccessibility())
    }
}
