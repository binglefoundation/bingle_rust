import XCTest
@testable import bingle_jsiFFI

/// A spy subclass of BingleJsiBridge that captures sendEvent calls for testing.
final class SpyBingleJsiBridge: BingleJsiBridge {
    var capturedEvents: [(name: String, body: Any?)] = []

    override func sendEvent(withName name: String!, body: Any!) {
        capturedEvents.append((name: name, body: body))
    }
}

/// XCTests for BingleJsiBridge, exercising the Swift bridge logic using a MockBingleJsiApi.
/// These tests verify that each bridge method correctly delegates to the API, maps parameters,
/// and resolves/rejects promises appropriately — without requiring a running React Native app.
final class BingleJsiBridgeTests: XCTestCase {

    var bridge: SpyBingleJsiBridge!
    var mockApi: MockBingleJsiApi!

    override func setUp() {
        super.setUp()
        bridge = SpyBingleJsiBridge()
        mockApi = MockBingleJsiApi()
        bridge.injectApi(mockApi)
    }

    override func tearDown() {
        bridge = nil
        mockApi = nil
        super.tearDown()
    }

    // MARK: - handleLookup

    func testHandleLookup_resolvesWithExpectedUserId() {
        mockApi.handleLookupResult = "user-abc-123"
        let expectation = self.expectation(description: "resolve called")
        var resolvedValue: Any?

        bridge.handleLookup(
            "alice",
            resolver: { value in
                resolvedValue = value
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(resolvedValue as? String, "user-abc-123")
        XCTAssertEqual(mockApi.handleLookupCalls, ["alice"])
    }

    func testHandleLookup_rejectsOnApiError() {
        mockApi.handleLookupError = NSError(domain: "test", code: 42, userInfo: [NSLocalizedDescriptionKey: "lookup failed"])
        let expectation = self.expectation(description: "reject called")
        var rejectedCode: String?

        bridge.handleLookup(
            "bob",
            resolver: { _ in XCTFail("unexpected resolve") },
            rejecter: { code, _, _ in
                rejectedCode = code
                expectation.fulfill()
            }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(rejectedCode, "BINGLE_ERROR")
    }

    func testHandleLookup_notInitialized_rejectsWithCorrectCode() {
        let uninitBridge = BingleJsiBridge()
        let expectation = self.expectation(description: "reject called")
        var rejectedCode: String?

        uninitBridge.handleLookup(
            "carol",
            resolver: { _ in XCTFail("unexpected resolve") },
            rejecter: { code, _, _ in
                rejectedCode = code
                expectation.fulfill()
            }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(rejectedCode, "BINGLE_NOT_INITIALIZED")
    }

    // MARK: - sendMessageToId

    func testSendMessageToId_resolvesAndRecordsCall() {
        mockApi.sendMessageToIdResult = true
        let expectation = self.expectation(description: "resolve called")
        var resolvedValue: Any?

        bridge.sendMessageToId(
            "user-xyz",
            message: ["text": "hello", "app": "testapp"] as NSDictionary,
            resolver: { value in
                resolvedValue = value
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(resolvedValue as? Bool, true)
        XCTAssertEqual(mockApi.sendMessageToIdCalls.count, 1)
        let call = mockApi.sendMessageToIdCalls[0]
        XCTAssertEqual(call.userId, "user-xyz")
        XCTAssertEqual(call.message.text, "hello")
        XCTAssertEqual(call.message.app, "testapp")
    }

    func testSendMessageToId_mapsAllMessageFields() {
        let expectation = self.expectation(description: "resolve called")

        bridge.sendMessageToId(
            "user-xyz",
            message: [
                "text": "body",
                "app": "myapp",
                "type": "chat",
                "tag": "t1",
                "response_tag": "rt1",
                "data": "base64data",
                "cipher_suite": "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
            ] as NSDictionary,
            resolver: { _ in expectation.fulfill() },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(mockApi.sendMessageToIdCalls.count, 1)
        let msg = mockApi.sendMessageToIdCalls[0].message
        XCTAssertEqual(msg.text, "body")
        XCTAssertEqual(msg.app, "myapp")
        XCTAssertEqual(msg.type, "chat")
        XCTAssertEqual(msg.tag, "t1")
        XCTAssertEqual(msg.responseTag, "rt1")
        XCTAssertEqual(msg.data, "base64data")
        XCTAssertEqual(msg.cipherSuite, "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256")
    }

    func testSendMessageToId_rejectsOnApiError() {
        mockApi.sendMessageToIdError = NSError(domain: "test", code: 1, userInfo: [NSLocalizedDescriptionKey: "send failed"])
        let expectation = self.expectation(description: "reject called")
        var rejectedCode: String?

        bridge.sendMessageToId(
            "user-xyz",
            message: ["text": "hi"] as NSDictionary,
            resolver: { _ in XCTFail("unexpected resolve") },
            rejecter: { code, _, _ in
                rejectedCode = code
                expectation.fulfill()
            }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(rejectedCode, "BINGLE_ERROR")
    }

    func testSendMessageToId_notInitialized_rejectsWithCorrectCode() {
        let uninitBridge = BingleJsiBridge()
        let expectation = self.expectation(description: "reject called")
        var rejectedCode: String?

        uninitBridge.sendMessageToId(
            "user-xyz",
            message: [:] as NSDictionary,
            resolver: { _ in XCTFail("unexpected resolve") },
            rejecter: { code, _, _ in
                rejectedCode = code
                expectation.fulfill()
            }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(rejectedCode, "BINGLE_NOT_INITIALIZED")
    }

    // MARK: - sendMessageToHandle

    func testSendMessageToHandle_resolvesAndRecordsCall() {
        mockApi.sendMessageToHandleResult = true
        let expectation = self.expectation(description: "resolve called")

        bridge.sendMessageToHandle(
            "alice",
            message: ["text": "hey alice"] as NSDictionary,
            resolver: { _ in expectation.fulfill() },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(mockApi.sendMessageToHandleCalls.count, 1)
        XCTAssertEqual(mockApi.sendMessageToHandleCalls[0].handle, "alice")
        XCTAssertEqual(mockApi.sendMessageToHandleCalls[0].message.text, "hey alice")
    }

    // MARK: - version

    func testVersion_resolvesWithVersionInfo() {
        mockApi.versionResult = VersionInfo(
            version: "1.2.3",
            gitSha: "deadbeef",
            buildTimestamp: "2024-06-01T12:00:00Z",
            buildNumber: "42"
        )
        let expectation = self.expectation(description: "resolve called")
        var resolvedDict: [String: Any]?

        bridge.version(
            { value in
                resolvedDict = value as? [String: Any]
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        let dict = resolvedDict
        XCTAssertNotNil(dict)
        XCTAssertEqual(dict?["version"] as? String, "1.2.3")
        XCTAssertEqual(dict?["git_sha"] as? String, "deadbeef")
        XCTAssertEqual(dict?["build_number"] as? String, "42")
        XCTAssertEqual(dict?["build_timestamp"] as? String, "2024-06-01T12:00:00Z")
    }

    // MARK: - isStarted

    func testIsStarted_resolvesWithTrueWhenStarted() {
        mockApi.isStartedResult = true
        let expectation = self.expectation(description: "resolve called")
        var resolvedValue: Any?

        bridge.isStarted(
            { value in
                resolvedValue = value
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(resolvedValue as? Bool, true)
        XCTAssertTrue(mockApi.isStartedCalled)
    }

    func testIsStarted_resolvesWithFalseWhenNotStarted() {
        mockApi.isStartedResult = false
        let expectation = self.expectation(description: "resolve called")
        var resolvedValue: Any?

        bridge.isStarted(
            { value in
                resolvedValue = value
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(resolvedValue as? Bool, false)
    }

    // MARK: - start

    func testStart_callsApiStart() {
        let expectation = self.expectation(description: "resolve called")

        bridge.start(
            { _ in expectation.fulfill() },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertTrue(mockApi.startCalled)
    }

    // MARK: - setMessageCallback

    func testSetMessageCallback_registersCallback() {
        let expectation = self.expectation(description: "resolve called")

        bridge.setMessageCallback(
            { _ in expectation.fulfill() },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertNotNil(mockApi.messageCallback)
    }

    func testSetMessageCallback_deliversMessageToEventEmitter() {
        let callbackExpectation = self.expectation(description: "setMessageCallback resolved")

        bridge.setMessageCallback(
            { _ in callbackExpectation.fulfill() },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )
        waitForExpectations(timeout: 2.0)

        guard let callback = mockApi.messageCallback else {
            XCTFail("messageCallback was not registered")
            return
        }

        let inboundMessage = BingleMessage(
            app: "chat",
            type: "text",
            tag: "tag1",
            responseTag: nil,
            text: "Hello from peer",
            data: nil,
            cipherSuite: "TLS_AES_256_GCM_SHA384"
        )
        callback.onMessage(senderId: "peer-user-id", senderHandle: "peer-handle", message: inboundMessage)

        XCTAssertEqual(bridge.capturedEvents.count, 1)
        let event = bridge.capturedEvents[0]
        XCTAssertEqual(event.name, "onMessage")
        let body = event.body as? [String: Any]
        XCTAssertNotNil(body)
        XCTAssertEqual(body?["sender_id"] as? String, "peer-user-id")
        XCTAssertEqual(body?["sender_handle"] as? String, "peer-handle")
        let msg = body?["message"] as? [String: Any]
        XCTAssertNotNil(msg)
        XCTAssertEqual(msg?["text"] as? String, "Hello from peer")
        XCTAssertEqual(msg?["app"] as? String, "chat")
        XCTAssertEqual(msg?["cipher_suite"] as? String, "TLS_AES_256_GCM_SHA384")
    }

    // MARK: - keypairStatus

    func testKeypairStatus_resolvesWithExpectedFields() {
        mockApi.keypairStatusResult = KeypairStatusResponse(
            status: .active, id: "test-id", handle: "test-handle", requiredAlgo: nil
        )
        let expectation = self.expectation(description: "resolve called")
        var resolvedDict: [String: Any]?

        bridge.keypairStatus(
            { value in
                resolvedDict = value as? [String: Any]
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        let dict = resolvedDict
        XCTAssertNotNil(dict)
        XCTAssertEqual(dict?["status"] as? String, "Active")
        XCTAssertEqual(dict?["id"] as? String, "test-id")
        XCTAssertEqual(dict?["handle"] as? String, "test-handle")
    }

    // MARK: - getNatType

    func testGetNatType_resolvesWithNatTypeString() {
        mockApi.natTypeResult = NatTypeResponse(natType: .symmetric)
        let expectation = self.expectation(description: "resolve called")
        var resolvedDict: [String: Any]?

        bridge.getNatType(
            { value in
                resolvedDict = value as? [String: Any]
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(resolvedDict?["nat_type"] as? String, "Symmetric")
    }

    // MARK: - generateKeypair

    func testGenerateKeypair_resolvesWithKeypairFields() {
        mockApi.keypairResult = Keypair(id: "gen-id-42", passphrase: "gen-passphrase-42")
        let expectation = self.expectation(description: "resolve called")
        var resolvedDict: [String: Any]?

        bridge.generateKeypair(
            { value in
                resolvedDict = value as? [String: Any]
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(resolvedDict?["id"] as? String, "gen-id-42")
        XCTAssertEqual(resolvedDict?["passphrase"] as? String, "gen-passphrase-42")
    }

    // MARK: - isBlocked

    func testIsBlocked_resolvesWithFalseByDefault() {
        mockApi.isBlockedResult = false
        let expectation = self.expectation(description: "resolve called")
        var resolvedValue: Any?

        bridge.isBlocked(
            "some-user-id",
            resolver: { value in
                resolvedValue = value
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(resolvedValue as? Bool, false)
    }

    func testIsBlocked_resolvesWithTrueForBlockedContact() {
        mockApi.isBlockedResult = true
        let expectation = self.expectation(description: "resolve called")
        var resolvedValue: Any?

        bridge.isBlocked(
            "blocked-user-id",
            resolver: { value in
                resolvedValue = value
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertEqual(resolvedValue as? Bool, true)
    }

    // MARK: - queueMessage

    func testQueueMessage_callsApiWithCorrectArgs() {
        let expectation = self.expectation(description: "resolve called")
        var resolvedValue: Any?

        bridge.queueMessage(
            ["alice", "bob"],
            text: "hello queue",
            resolver: { value in
                resolvedValue = value
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertNil(resolvedValue)
        XCTAssertEqual(mockApi.queueMessageCalls.count, 1)
        XCTAssertEqual(mockApi.queueMessageCalls[0].recipientHandles, ["alice", "bob"])
        XCTAssertEqual(mockApi.queueMessageCalls[0].text, "hello queue")
    }

    func testQueueMessage_appearsInGetMessagesWithZeroProgress() {
        mockApi.nextQueueTimestamp = 1700001000

        let queueExp = expectation(description: "queueMessage resolves")
        bridge.queueMessage(
            ["alice"],
            text: "outbound",
            resolver: { _ in queueExp.fulfill() },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )
        waitForExpectations(timeout: 2.0)

        let getExp = expectation(description: "getMessages resolves")
        var messages: [[String: Any]]?
        bridge.getMessages(
            { value in
                messages = value as? [[String: Any]]
                getExp.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )
        waitForExpectations(timeout: 2.0)

        XCTAssertEqual(messages?.count, 1)
        let msg = messages?[0]
        XCTAssertEqual(msg?["recipient_handles"] as? [String], ["alice"])
        XCTAssertEqual(msg?["text"] as? String, "outbound")
        XCTAssertEqual(msg?["progress"] as? Float, 0.0)
        XCTAssertNil(msg?["failure_reason"] as? String)
    }

    func testProcessSendQueue_callsSendMessageToHandleAndUpdatesProgress() {
        mockApi.nextQueueTimestamp = 1700003000

        let queueExp = expectation(description: "queueMessage resolves")
        bridge.queueMessage(
            ["alice"],
            text: "queued text",
            resolver: { _ in queueExp.fulfill() },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )
        waitForExpectations(timeout: 2.0)

        let sendExp = expectation(description: "processSendQueue resolves")
        var processedCount: Any?
        bridge.processSendQueue(
            { value in
                processedCount = value
                sendExp.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )
        waitForExpectations(timeout: 2.0)

        XCTAssertEqual(processedCount as? Int, 1)
        XCTAssertEqual(mockApi.sendMessageToHandleCalls.count, 1)
        XCTAssertEqual(mockApi.sendMessageToHandleCalls[0].handle, "alice")
        XCTAssertEqual(mockApi.sendMessageToHandleCalls[0].message.text, "queued text")
        XCTAssertEqual(mockApi.updateMessageStatusCalls.count, 1)
        XCTAssertEqual(mockApi.updateMessageStatusCalls[0].timestamp, mockApi.nextQueueTimestamp)
        XCTAssertEqual(mockApi.updateMessageStatusCalls[0].progress, 1.0)
        XCTAssertNil(mockApi.updateMessageStatusCalls[0].failureReason)
    }

    func testSendingLoop_updatesProgressAndSuccessInQueue() {
        mockApi.nextQueueTimestamp = 1700002000

        let queueExp = expectation(description: "queueMessage resolves")
        bridge.queueMessage(
            ["bob"],
            text: "test message",
            resolver: { _ in queueExp.fulfill() },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )
        waitForExpectations(timeout: 2.0)

        // simulate the background sender completing successfully
        let updateExp = expectation(description: "updateMessageStatus resolves")
        bridge.updateMessageStatus(
            Double(mockApi.nextQueueTimestamp),
            progress: 1.0,
            failureReason: nil,
            resolver: { _ in updateExp.fulfill() },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )
        waitForExpectations(timeout: 2.0)

        let getExp = expectation(description: "getMessages resolves")
        var messages: [[String: Any]]?
        bridge.getMessages(
            { value in
                messages = value as? [[String: Any]]
                getExp.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )
        waitForExpectations(timeout: 2.0)

        XCTAssertEqual(messages?.count, 1)
        let msg = messages?[0]
        XCTAssertEqual(msg?["text"] as? String, "test message")
        XCTAssertEqual(msg?["progress"] as? Float, 1.0)
        XCTAssertNil(msg?["failure_reason"] as? String)
        XCTAssertEqual(mockApi.updateMessageStatusCalls.count, 1)
        XCTAssertEqual(mockApi.updateMessageStatusCalls[0].timestamp, mockApi.nextQueueTimestamp)
        XCTAssertEqual(mockApi.updateMessageStatusCalls[0].progress, 1.0)
        XCTAssertNil(mockApi.updateMessageStatusCalls[0].failureReason)
    }

    // MARK: - getMessages

    func testGetMessages_includesCipherSuite() {
        mockApi.messagesResult = [
            Message(
                senderHandle: "alice",
                recipientHandles: ["bob"],
                timestamp: 1700000000,
                text: "hi",
                cipherSuite: "TLS_AES_256_GCM_SHA384",
                progress: 1.0,
                failureReason: nil
            ),
            Message(
                senderHandle: "carol",
                recipientHandles: ["bob"],
                timestamp: 1700000001,
                text: "hey",
                cipherSuite: nil,
                progress: 1.0,
                failureReason: nil
            ),
        ]
        let expectation = self.expectation(description: "resolve called")
        var resolvedArray: [[String: Any]]?

        bridge.getMessages(
            { value in
                resolvedArray = value as? [[String: Any]]
                expectation.fulfill()
            },
            rejecter: { _, _, _ in XCTFail("unexpected rejection") }
        )

        waitForExpectations(timeout: 2.0)
        XCTAssertNotNil(resolvedArray)
        XCTAssertEqual(resolvedArray?.count, 2)

        let first = resolvedArray?[0]
        XCTAssertEqual(first?["sender_handle"] as? String, "alice")
        XCTAssertEqual(first?["text"] as? String, "hi")
        XCTAssertEqual(first?["cipher_suite"] as? String, "TLS_AES_256_GCM_SHA384")

        let second = resolvedArray?[1]
        XCTAssertEqual(second?["sender_handle"] as? String, "carol")
        // nil cipher_suite is mapped to NSNull/nil, not a String
        XCTAssertNil(second?["cipher_suite"] as? String)
    }
}
