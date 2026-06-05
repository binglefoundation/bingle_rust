import Foundation
import XCTest
@testable import bingle_jsiFFI

/// A mock implementation of BingleJsiApiProtocol for unit testing BingleJsiBridge.
/// Records calls and returns configurable results.
class MockBingleJsiApi: BingleJsiApiProtocol {

    // MARK: - Recorded calls

    struct SendMessageToIdCall {
        let userId: String
        let message: BingleMessage
    }

    struct SendMessageToHandleCall {
        let handle: String
        let message: BingleMessage
    }

    var handleLookupCalls: [String] = []
    var sendMessageToIdCalls: [SendMessageToIdCall] = []
    var sendMessageToHandleCalls: [SendMessageToHandleCall] = []
    var startCalled = false
    var isStartedCalled = false
    var messageCallback: MessageCallback?
    var logCallback: LogCallback?
    var listeningCallback: ListeningCallback?

    // MARK: - Configurable return values

    var handleLookupResult: String = "mock-user-id"
    var sendMessageToIdResult: Bool = true
    var sendMessageToHandleResult: Bool = true
    var isStartedResult: Bool = true
    var versionResult: VersionInfo = VersionInfo(
        version: "0.0.0-test",
        gitSha: "abc123",
        buildTimestamp: "2024-01-01T00:00:00Z",
        buildNumber: "1"
    )
    var natTypeResult: NatTypeResponse = NatTypeResponse(natType: .fullCone)
    var keypairResult: Keypair = Keypair(id: "mock-id", passphrase: "mock-passphrase")
    var contactsResult: [Contact] = []
    var messagesResult: [Message] = []
    var keypairStatusResult: KeypairStatusResponse = KeypairStatusResponse(
        status: .active, id: "mock-id", handle: "mock-handle", requiredAlgo: nil
    )
    var isBlockedResult: Bool = false

    // MARK: - Error injection

    var handleLookupError: Error?
    var sendMessageToIdError: Error?
    var sendMessageToHandleError: Error?

    // MARK: - BingleJsiApiProtocol implementation

    func handleLookup(handle: String) throws -> String {
        handleLookupCalls.append(handle)
        if let error = handleLookupError { throw error }
        return handleLookupResult
    }

    func sendMessageToId(userId: String, message: BingleMessage) throws -> Bool {
        sendMessageToIdCalls.append(SendMessageToIdCall(userId: userId, message: message))
        if let error = sendMessageToIdError { throw error }
        return sendMessageToIdResult
    }

    func sendMessageToHandle(handle: String, message: BingleMessage) throws -> Bool {
        sendMessageToHandleCalls.append(SendMessageToHandleCall(handle: handle, message: message))
        return sendMessageToHandleResult
    }

    func sendMessageToNetwork(networkSourceKey: NetworkSourceKey, userId: String, message: BingleMessage) throws -> Bool {
        return true
    }

    func sendMessageToIdWithResponse(userId: String, message: BingleMessage) throws -> BingleMessage {
        return message
    }

    func sendMessageToHandleWithResponse(handle: String, message: BingleMessage) throws -> BingleMessage {
        return message
    }

    func sendMessageToNetworkWithResponse(networkSourceKey: NetworkSourceKey, userId: String, message: BingleMessage) throws -> BingleMessage {
        return message
    }

    func queued() throws -> [BingleMessage] {
        return []
    }

    func version() throws -> VersionInfo {
        return versionResult
    }

    func getVersions() throws -> [String: VersionInfo] {
        return [:]
    }

    func getNatType() throws -> NatTypeResponse {
        return natTypeResult
    }

    func generateKeypair() throws -> Keypair {
        return keypairResult
    }

    func registerKeypair(handle: String) throws {}

    func addContact(handle: String, id: String, source: ContactSource) throws {}

    func blockContact(id: String) throws {}

    func removeContact(id: String) throws {}

    func isBlocked(id: String) throws -> Bool {
        return isBlockedResult
    }

    func getContacts() throws -> [Contact] {
        return contactsResult
    }

    func addMessage(senderHandle: String, recipientHandles: [String], timestamp: Int64, text: String, cipherSuite: String?) throws {}

    func getMessages() throws -> [Message] {
        return messagesResult
    }

    func keypairStatus() throws -> KeypairStatusResponse {
        return keypairStatusResult
    }

    func save(path: String) throws {}

    func load(path: String) throws {}

    func setMessageCallback(callback: MessageCallback) {
        messageCallback = callback
    }

    func setLogCallback(callback: LogCallback) {
        logCallback = callback
    }

    func setListeningCallback(callback: ListeningCallback) {
        listeningCallback = callback
    }

    func start() throws {
        startCalled = true
    }

    func isStarted() -> Bool {
        isStartedCalled = true
        return isStartedResult
    }
}
