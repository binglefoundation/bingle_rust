import Foundation
import React
import UIKit
import UserNotifications

/// React Native native module that bridges the uniffi-generated BingleJsi API to JavaScript.
///
/// This module is registered under the name "BingleJsi" and exposes the uniffi-generated
/// `init(config:)` function plus all BingleJsiApi methods to the React Native JS runtime.
@objc(BingleJsi)
class BingleJsiBridge: RCTEventEmitter {

    private var apiInstance: (any BingleJsiApiProtocol)?

    override init() {
        super.init()
        // The app's AppDelegate posts the raw APNs token / failure as Notifications (so it needs no
        // reference to this module and stays a pure conduit). Observe them and forward to Rust.
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleApnsToken(_:)),
            name: Notification.Name("BingleApnsToken"),
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleApnsRegistrationFailed(_:)),
            name: Notification.Name("BingleApnsRegistrationFailed"),
            object: nil
        )
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    /// Inject a mock or alternative API instance for testing.
    /// Call this before any bridge methods that require initialization.
    func injectApi(_ api: any BingleJsiApiProtocol) {
        self.apiInstance = api
    }

    /// Forwards the raw APNs token bytes the AppDelegate captured straight to Rust (which
    /// hex-encodes, signs, and POSTs /register). No logic here — pure conduit.
    @objc private func handleApnsToken(_ note: Notification) {
        guard let api = apiInstance, let data = note.userInfo?["token"] as? Data else { return }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                _ = try api.registerApnsToken(token: data)
            } catch {
                NSLog("[BingleJsi] registerApnsToken failed: \(error)")
            }
        }
    }

    @objc private func handleApnsRegistrationFailed(_ note: Notification) {
        guard let api = apiInstance else { return }
        let reason = (note.userInfo?["reason"] as? String) ?? "unknown"
        api.apnsRegistrationFailed(reason: reason)
    }

    override static func requiresMainQueueSetup() -> Bool {
        return false
    }

    override func supportedEvents() -> [String] {
        return ["onMessage", "onLog", "onListening"]
    }

    @objc
    func initialize(_ config: NSDictionary, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let jsiConfig = BingleJsiConfig(
                    handle: config["handle"] as? String,
                    passphrase: config["passphrase"] as? String,
                    relay: config["relay"] as? Bool ?? false,
                    staticIp: config["static_ip"] as? String,
                    stunServers: config["stun_servers"] as? String,
                    stunServersFile: config["stun_servers_file"] as? String,
                    nodeFile: config["node_file"] as? String,
                    logLevel: config["log_level"] as? String,
                    appId: (config["app_id"] as? NSNumber)?.uint64Value,
                    assetId: (config["asset_id"] as? NSNumber)?.uint64Value,
                    handleCacheExpirySecs: (config["handle_cache_expiry_secs"] as? NSNumber)?.uint64Value,
                    debug: config["debug"] as? Bool ?? false,
                    local: config["local"] as? String,
                    notifyGatewayUrl: config["notify_gateway_url"] as? String,
                    notifyOnGiveup: config["notify_on_giveup"] as? Bool,
                    notifyEnv: config["notify_env"] as? String
                )
                let api = try createBingleApi(config: jsiConfig)
                self.apiInstance = api
                resolve(true)
            } catch {
                reject("BINGLE_INIT_ERROR", "Failed to initialize BingleJsi: \(error)", error)
            }
        }
    }

    @objc
    func handleLookup(_ handle: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let result = try api.handleLookup(handle: handle)
                resolve(result)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func handleLookupPartial(_ handle: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let result = try api.handleLookupPartial(handle: handle)
                resolve([
                    "id": result.id,
                    "canonical_handle": result.canonicalHandle,
                ])
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func sendMessageToId(_ userId: String, message: NSDictionary, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let msg = self.dictToMessage(message)
                let result = try api.sendMessageToId(userId: userId, message: msg)
                resolve(result)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func sendMessageToHandle(_ handle: String, message: NSDictionary, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let msg = self.dictToMessage(message)
                let result = try api.sendMessageToHandle(handle: handle, message: msg)
                resolve(result)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func version(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        DispatchQueue.global(qos: .userInitiated).async {
            // version() works before init — it only reads compile-time constants.
            // If the API is initialized, delegate to it; otherwise call the
            // free function directly.
            if let api = self.apiInstance {
                do {
                    let info = try api.version()
                    resolve([
                        "version": info.version,
                        "git_sha": info.gitSha as Any,
                        "build_timestamp": info.buildTimestamp,
                        "build_number": info.buildNumber,
                    ])
                } catch {
                    reject("BINGLE_ERROR", "\(error)", error)
                }
            } else {
                let info = getVersion()
                resolve([
                    "version": info.version,
                    "git_sha": info.gitSha as Any,
                    "build_timestamp": info.buildTimestamp,
                    "build_number": info.buildNumber,
                ])
            }
        }
    }

    @objc
    func queued(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let messages = try api.queued()
                resolve(messages.map { self.messageToDict($0) })
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func getNatType(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let resp = try api.getNatType()
                resolve(["nat_type": self.natTypeToString(resp.natType)])
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func networkAvailable(_ forceRecheck: Bool, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let available = try api.networkAvailable(forceRecheck: forceRecheck)
                resolve(available)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func generateKeypair(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let kp = try api.generateKeypair()
                resolve(["id": kp.id, "passphrase": kp.passphrase])
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func importKeypair(_ passphrase: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let kp = try api.importKeypair(passphrase: passphrase)
                resolve(["id": kp.id, "passphrase": kp.passphrase])
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func signNotifyEnvelope(_ route: String, iss: String, audience: String, token: String, env: String, nonce: String, exp: NSNumber, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let sig = try api.signNotifyEnvelope(route: route, iss: iss, audience: audience, token: token, env: env, nonce: nonce, exp: exp.int64Value)
                resolve(sig)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    // MARK: - iOS push registration (bingle_notify #i)
    //
    // Pure bridge: the raw APNs token bytes cross to Rust untouched (hex-encoding, envelope
    // building, signing, and the POST all live in Rust). The only native work here is the
    // irreducible UIKit platform calls, driven from the PushRegistrationCallback bridge below.

    @objc(requestPushRegistration:rejecter:)
    func requestPushRegistration(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        do {
            try api.requestPushRegistration()
            resolve(nil)
        } catch {
            reject("BINGLE_ERROR", "\(error)", error)
        }
    }

    @objc(registerApnsToken:resolver:rejecter:)
    func registerApnsToken(_ token: NSArray, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        // The token arrives from JS as an array of byte values (the raw Data iOS delivered). Forward
        // the bytes verbatim — no hex-encoding here. uniffi maps Rust `Vec<u8>` to Swift `Data`.
        let bytes: [UInt8] = token.compactMap { ($0 as? NSNumber)?.uint8Value }
        let data = Data(bytes)
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let accepted = try api.registerApnsToken(token: data)
                resolve(accepted)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc(apnsRegistrationFailed:resolver:rejecter:)
    func apnsRegistrationFailed(_ reason: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        api.apnsRegistrationFailed(reason: reason)
        resolve(nil)
    }

    @objc(setPushRegistrationCallback:rejecter:)
    func setPushRegistrationCallback(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        let bridge = PushRegistrationCallbackBridge()
        api.setPushRegistrationCallback(callback: bridge)
        resolve(nil)
    }

    @objc
    func registerKeypair(_ handle: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let result = try api.registerKeypair(handle: handle)
                resolve(result)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func addContact(_ handle: String, id: String, source: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let src: ContactSource = source == "Received" ? .received : .manual
                try api.addContact(handle: handle, id: id, source: src)
                resolve(nil)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func blockContact(_ id: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try api.blockContact(id: id)
                resolve(nil)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func removeContact(_ id: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try api.removeContact(id: id)
                resolve(nil)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func isBlocked(_ id: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let result = try api.isBlocked(id: id)
                resolve(result)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func getContacts(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let contacts = try api.getContacts()
                resolve(contacts.map { ["handle": $0.handle, "id": $0.id, "fields": $0.fields] })
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func addMessage(_ senderHandle: String, recipientHandles: [String], timestamp: Double, text: String, cipherSuite: String?, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try api.addMessage(
                    senderHandle: senderHandle,
                    recipientHandles: recipientHandles,
                    timestamp: Int64(timestamp),
                    text: text,
                    cipherSuite: cipherSuite
                )
                resolve(nil)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func getMessages(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let messages = try api.getMessages()
                resolve(messages.map {
                    [
                        "sender_handle": $0.senderHandle,
                        "recipient_handles": $0.recipientHandles,
                        "timestamp": $0.timestamp,
                        "text": $0.text,
                        "cipher_suite": $0.cipherSuite as Any,
                        "progress": $0.progress as Any,
                        "failure_reason": $0.failureReason as Any,
                    ] as [String: Any]
                })
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func queueMessage(_ recipientHandles: [String], text: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try api.queueMessage(recipientHandles: recipientHandles, text: text)
                resolve(nil)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func updateMessageStatus(_ timestamp: Double, progress: Double, failureReason: String?, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try api.updateMessageStatus(timestamp: Int64(timestamp), progress: Float(progress), failureReason: failureReason)
                resolve(nil)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func processSendQueue(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let messages = try api.getMessages()
                var processed = 0
                for msg in messages where (msg.progress ?? 1.0) < 1.0 {
                    let bingleMsg = BingleMessage(
                        app: nil, type: nil, tag: nil, responseTag: nil,
                        text: msg.text, data: nil, cipherSuite: nil
                    )
                    var allSuccess = true
                    var lastError: String? = nil
                    for handle in msg.recipientHandles {
                        do {
                            _ = try api.sendMessageToHandle(handle: handle, message: bingleMsg)
                        } catch {
                            allSuccess = false
                            lastError = "\(error)"
                            break
                        }
                    }
                    if allSuccess {
                        try api.updateMessageStatus(timestamp: msg.timestamp, progress: 1.0, failureReason: nil)
                    } else if let err = lastError {
                        try api.updateMessageStatus(timestamp: msg.timestamp, progress: 1.0, failureReason: err)
                    }
                    processed += 1
                }
                resolve(processed)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func keypairStatus(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let status = try api.keypairStatus()
                resolve([
                    "status": self.keypairStatusToString(status.status),
                    "id": status.id as Any,
                    "handle": status.handle as Any,
                    "required_algo": status.requiredAlgo as Any,
                ])
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func save(_ path: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try api.save(path: path)
                resolve(nil)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func load(_ path: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try api.load(path: path)
                resolve(nil)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func start(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try api.start()
                resolve(nil)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func stop(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            resolve(nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try api.stop()
                resolve(nil)
            } catch {
                reject("BINGLE_ERROR", "\(error)", error)
            }
        }
    }

    @objc
    func isStarted(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            let result = api.isStarted()
            resolve(result)
        }
    }

    @objc
    func setString(_ key: String, value: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        UserDefaults.standard.set(value, forKey: key)
        resolve(nil)
    }

    @objc
    func getString(_ key: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        let value = UserDefaults.standard.string(forKey: key)
        resolve(value)
    }

    @objc(setLogCallback:resolver:rejecter:)
    func setLogCallback(_ logLevel: String?, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        let bridge = LogCallbackBridge(emitter: self)
        // setLogCallback works before init — it sets a global log callback.
        // If the API is already initialized, also register on the instance;
        // otherwise use the free function so logs during init are captured.
        if let api = apiInstance {
            api.setLogCallback(callback: bridge)
        } else {
            setLogCallbackGlobal(callback: bridge, logLevel: logLevel)
        }
        resolve(nil)
    }

    @objc(setMessageCallback:rejecter:)
    func setMessageCallback(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        let bridge = MessageCallbackBridge(emitter: self)
        api.setMessageCallback(callback: bridge)
        resolve(nil)
    }

    @objc(setListeningCallback:rejecter:)
    func setListeningCallback(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        let bridge = ListeningCallbackBridge(emitter: self)
        api.setListeningCallback(callback: bridge)
        resolve(nil)
    }

    // MARK: - Helper conversions

    private func dictToMessage(_ dict: NSDictionary) -> BingleMessage {
        return BingleMessage(
            app: dict["app"] as? String,
            type: dict["type"] as? String,
            tag: dict["tag"] as? String,
            responseTag: dict["response_tag"] as? String,
            text: dict["text"] as? String,
            data: dict["data"] as? String,
            cipherSuite: dict["cipher_suite"] as? String
        )
    }

    private func messageToDict(_ msg: BingleMessage) -> [String: Any?] {
        return [
            "app": msg.app,
            "type": msg.type,
            "tag": msg.tag,
            "response_tag": msg.responseTag,
            "text": msg.text,
            "data": msg.data,
            "cipher_suite": msg.cipherSuite,
        ]
    }

    private func natTypeToString(_ natType: NatType) -> String {
        switch natType {
        case .unknown: return "Unknown"
        case .noConnection: return "NoConnection"
        case .symmetric: return "Symmetric"
        case .restricted: return "Restricted"
        case .fullCone: return "FullCone"
        }
    }

    private func keypairStatusToString(_ status: KeypairStatus) -> String {
        switch status {
        case .none: return "None"
        case .unfunded: return "Unfunded"
        case .funded: return "Funded"
        case .active: return "Active"
        case .upgradeRequired: return "UpgradeRequired"
        }
    }
}

/// Bridges uniffi's LogCallback to RCTEventEmitter events.
///
/// Each `onLog` invocation sends an "onLog" event with timestamp, level, and message
/// to the JS layer via React Native's event emitter infrastructure.
class LogCallbackBridge: LogCallback {
    private weak var emitter: RCTEventEmitter?

    init(emitter: RCTEventEmitter) {
        self.emitter = emitter
    }

    func onLog(timestamp: Int64, level: String, message: String) {
        emitter?.sendEvent(withName: "onLog", body: [
            "timestamp": timestamp,
            "level": level,
            "message": message,
        ])
    }
}

/// Bridges uniffi's MessageCallback to RCTEventEmitter events.
///
/// Each `onMessage` invocation sends an "onMessage" event with senderId,
/// senderHandle, and the message fields to the JS layer via React Native's
/// event emitter infrastructure.
class MessageCallbackBridge: MessageCallback {
    private weak var emitter: RCTEventEmitter?

    init(emitter: RCTEventEmitter) {
        self.emitter = emitter
    }

    func onMessage(senderId: String, senderHandle: String, message: BingleMessage) {
        emitter?.sendEvent(withName: "onMessage", body: [
            "sender_id": senderId,
            "sender_handle": senderHandle,
            "message": [
                "app": message.app as Any,
                "type": message.type as Any,
                "tag": message.tag as Any,
                "response_tag": message.responseTag as Any,
                "text": message.text as Any,
                "data": message.data as Any,
                "cipher_suite": message.cipherSuite as Any,
            ],
        ])
    }
}

/// Bridges uniffi's ListeningCallback to RCTEventEmitter events.
///
/// Each `onListening` invocation sends an "onListening" event with listening
/// state and NAT type to the JS layer via React Native's event emitter
/// infrastructure.
class ListeningCallbackBridge: ListeningCallback {
    private weak var emitter: RCTEventEmitter?

    init(emitter: RCTEventEmitter) {
        self.emitter = emitter
    }

    func onListening(listening: Bool, natType: String) {
        emitter?.sendEvent(withName: "onListening", body: [
            "listening": listening,
            "nat_type": natType,
        ])
    }
}

/// Bridges uniffi's PushRegistrationCallback to the iOS platform registration calls.
///
/// When Rust asks the host to register (via `requestPushRegistration`), this performs the only two
/// UIKit calls that cannot live in Rust: request notification permission, then
/// `registerForRemoteNotifications()`. The resulting token is delivered by iOS to the app's
/// AppDelegate, which forwards the raw bytes back through `registerApnsToken` — there is no other
/// logic here.
class PushRegistrationCallbackBridge: PushRegistrationCallback {
    func onRequestRegistration() {
        DispatchQueue.main.async {
            UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { granted, _ in
                guard granted else { return }
                DispatchQueue.main.async {
                    UIApplication.shared.registerForRemoteNotifications()
                }
            }
        }
    }
}
