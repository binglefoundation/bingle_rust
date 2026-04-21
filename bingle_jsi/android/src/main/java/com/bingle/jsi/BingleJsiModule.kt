package com.bingle.jsi

import com.facebook.react.bridge.*
import uniffi.bingle_jsi.*

/**
 * React Native native module that bridges the uniffi-generated BingleJsi API to JavaScript.
 *
 * Registered under the name "BingleJsi". All methods are exposed as promise-based async calls.
 */
class BingleJsiModule(reactContext: ReactApplicationContext) :
    ReactContextBaseJavaModule(reactContext) {

    private var apiInstance: BingleJsiApi? = null

    override fun getName(): String = "BingleJsi"

    @ReactMethod
    fun initialize(config: ReadableMap, promise: Promise) {
        Thread {
            try {
                val jsiConfig = BingleJsiConfig(
                    handle = config.tryGetString("handle"),
                    passphrase = config.tryGetString("passphrase"),
                    relay = if (config.hasKey("relay")) config.getBoolean("relay") else false,
                    staticIp = config.tryGetString("static_ip"),
                    stunServers = config.tryGetString("stun_servers"),
                    stunServersFile = config.tryGetString("stun_servers_file"),
                    nodeFile = config.tryGetString("node_file"),
                    logLevel = config.tryGetString("log_level"),
                    appId = config.tryGetULong("app_id"),
                    assetId = config.tryGetULong("asset_id"),
                    handleCacheExpirySecs = config.tryGetULong("handle_cache_expiry_secs"),
                    debug = if (config.hasKey("debug")) config.getBoolean("debug") else false,
                    local = config.tryGetString("local")
                )
                val api = createBingleApi(jsiConfig)
                apiInstance = api
                promise.resolve(true)
            } catch (e: Exception) {
                promise.reject("BINGLE_INIT_ERROR", "Failed to initialize BingleJsi: ${e.message}", e)
            }
        }.start()
    }

    @ReactMethod
    fun handleLookup(handle: String, promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val result = api.handleLookup(handle)
                promise.resolve(result)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun sendMessageToId(userId: String, message: ReadableMap, promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val msg = mapToMessage(message)
                val result = api.sendMessageToId(userId, msg)
                promise.resolve(result)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun sendMessageToHandle(handle: String, message: ReadableMap, promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val msg = mapToMessage(message)
                val result = api.sendMessageToHandle(handle, msg)
                promise.resolve(result)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun version(promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val info = api.version()
                val map = Arguments.createMap()
                map.putString("version", info.version)
                if (info.gitSha != null) map.putString("git_sha", info.gitSha) else map.putNull("git_sha")
                map.putString("build_timestamp", info.buildTimestamp)
                map.putString("build_number", info.buildNumber)
                promise.resolve(map)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun queued(promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val messages = api.queued()
                val arr = Arguments.createArray()
                messages.forEach { arr.pushMap(messageToMap(it)) }
                promise.resolve(arr)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun getNatType(promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val resp = api.getNatType()
                val map = Arguments.createMap()
                map.putString("nat_type", natTypeToString(resp.natType))
                promise.resolve(map)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun generateKeypair(promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val kp = api.generateKeypair()
                val map = Arguments.createMap()
                map.putString("id", kp.id)
                map.putString("passphrase", kp.passphrase)
                promise.resolve(map)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun registerKeypair(handle: String, promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val result = api.registerKeypair(handle)
                promise.resolve(result)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun addContact(handle: String, id: String, source: String, promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val src = if (source == "Received") ContactSource.RECEIVED else ContactSource.MANUAL
                api.addContact(handle, id, src)
                promise.resolve(null)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun blockContact(id: String, promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                api.blockContact(id)
                promise.resolve(null)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun removeContact(id: String, promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                api.removeContact(id)
                promise.resolve(null)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun isBlocked(id: String, promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val result = api.isBlocked(id)
                promise.resolve(result)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun getContacts(promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val contacts = api.getContacts()
                val arr = Arguments.createArray()
                contacts.forEach { c ->
                    val map = Arguments.createMap()
                    map.putString("handle", c.handle)
                    map.putString("id", c.id)
                    val fields = Arguments.createMap()
                    c.fields.forEach { (k, v) -> fields.putString(k, v) }
                    map.putMap("fields", fields)
                    arr.pushMap(map)
                }
                promise.resolve(arr)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun addMessage(senderHandle: String, recipientHandles: ReadableArray, timestamp: Double, text: String, promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val handles = mutableListOf<String>()
                for (i in 0 until recipientHandles.size()) {
                    handles.add(recipientHandles.getString(i))
                }
                api.addMessage(senderHandle, handles, timestamp.toLong(), text)
                promise.resolve(null)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun getMessages(promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val messages = api.getMessages()
                val arr = Arguments.createArray()
                messages.forEach { m ->
                    val map = Arguments.createMap()
                    map.putString("sender_handle", m.senderHandle)
                    val handles = Arguments.createArray()
                    m.recipientHandles.forEach { handles.pushString(it) }
                    map.putArray("recipient_handles", handles)
                    map.putDouble("timestamp", m.timestamp.toDouble())
                    map.putString("text", m.text)
                    arr.pushMap(map)
                }
                promise.resolve(arr)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun keypairStatus(promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                val status = api.keypairStatus()
                val map = Arguments.createMap()
                map.putString("status", keypairStatusToString(status.status))
                if (status.id != null) map.putString("id", status.id) else map.putNull("id")
                if (status.handle != null) map.putString("handle", status.handle) else map.putNull("handle")
                if (status.requiredAlgo != null) map.putDouble("required_algo", status.requiredAlgo!!) else map.putNull("required_algo")
                promise.resolve(map)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun save(path: String, promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                api.save(path)
                promise.resolve(null)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    @ReactMethod
    fun load(path: String, promise: Promise) {
        val api = apiInstance
        if (api == null) {
            promise.reject("BINGLE_NOT_INITIALIZED", "BingleJsi not initialized. Call init first.")
            return
        }
        Thread {
            try {
                api.load(path)
                promise.resolve(null)
            } catch (e: Exception) {
                promise.reject("BINGLE_ERROR", e.message, e)
            }
        }.start()
    }

    // -- Helper conversions --

    private fun ReadableMap.tryGetString(key: String): String? =
        if (hasKey(key) && !isNull(key)) getString(key) else null

    private fun ReadableMap.tryGetULong(key: String): ULong? =
        if (hasKey(key) && !isNull(key)) getDouble(key).toULong() else null

    private fun mapToMessage(map: ReadableMap): BingleMessage = BingleMessage(
        app = map.tryGetString("app"),
        type = map.tryGetString("type"),
        tag = map.tryGetString("tag"),
        responseTag = map.tryGetString("response_tag"),
        text = map.tryGetString("text"),
        data = map.tryGetString("data")
    )

    private fun messageToMap(msg: BingleMessage): WritableMap {
        val map = Arguments.createMap()
        if (msg.app != null) map.putString("app", msg.app) else map.putNull("app")
        if (msg.type != null) map.putString("type", msg.type) else map.putNull("type")
        if (msg.tag != null) map.putString("tag", msg.tag) else map.putNull("tag")
        if (msg.responseTag != null) map.putString("response_tag", msg.responseTag) else map.putNull("response_tag")
        if (msg.text != null) map.putString("text", msg.text) else map.putNull("text")
        if (msg.data != null) map.putString("data", msg.data) else map.putNull("data")
        return map
    }

    private fun natTypeToString(natType: NatType): String = when (natType) {
        NatType.UNKNOWN -> "Unknown"
        NatType.NO_CONNECTION -> "NoConnection"
        NatType.SYMMETRIC -> "Symmetric"
        NatType.RESTRICTED -> "Restricted"
        NatType.FULL_CONE -> "FullCone"
    }

    private fun keypairStatusToString(status: KeypairStatus): String = when (status) {
        KeypairStatus.NONE -> "None"
        KeypairStatus.UNFUNDED -> "Unfunded"
        KeypairStatus.FUNDED -> "Funded"
        KeypairStatus.ACTIVE -> "Active"
    }
}
