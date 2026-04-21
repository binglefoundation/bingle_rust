import Foundation
import React

/// React Native native module that bridges the uniffi-generated BingleJsi API to JavaScript.
///
/// This module is registered under the name "BingleJsi" and exposes the uniffi-generated
/// `init(config:)` function plus all BingleJsiApi methods to the React Native JS runtime.
@objc(BingleJsi)
class BingleJsiBridge: RCTEventEmitter {

    private var apiInstance: BingleJsiApi?

    override static func requiresMainQueueSetup() -> Bool {
        return false
    }

    override func supportedEvents() -> [String] {
        return ["onMessage"]
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
                    local: config["local"] as? String
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
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
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
    func addMessage(_ senderHandle: String, recipientHandles: [String], timestamp: Double, text: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        guard let api = apiInstance else {
            reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.", nil)
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try api.addMessage(senderHandle: senderHandle, recipientHandles: recipientHandles, timestamp: Int64(timestamp), text: text)
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
                    ] as [String: Any]
                })
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

    // MARK: - Helper conversions

    private func dictToMessage(_ dict: NSDictionary) -> BingleMessage {
        return BingleMessage(
            app: dict["app"] as? String,
            type: dict["type"] as? String,
            tag: dict["tag"] as? String,
            responseTag: dict["response_tag"] as? String,
            text: dict["text"] as? String,
            data: dict["data"] as? String
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
        }
    }
}
