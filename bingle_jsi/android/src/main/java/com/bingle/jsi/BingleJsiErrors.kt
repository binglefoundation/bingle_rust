package com.bingle.jsi

import uniffi.bingle_jsi.BingleJsiException
import uniffi.bingle_jsi.FailureKind

/**
 * How a failed call is reported to JavaScript (bingle_rust #464, #470). Kept apart from
 * [BingleJsiModule], with no React Native types, so it can be unit tested on the JVM.
 *
 * Names are the Rust and Swift ones, so one JavaScript check works on both platforms: UniFFI's
 * Kotlin renames `InternalError` to `InternalException`, and this reports `InternalError`.
 */
internal object BingleJsiErrors {
    /** A failed call's structured form: the variant name and its fields, in declaration order. */
    data class Info(val kind: String, val fields: Map<String, String>)

    /**
     * The structured form of [e], sent as `error.userInfo`: `kind` is the `BingleJsiError` variant
     * name, plus that variant's fields under their camelCase names. Any other error is
     * `kind: "Unknown"`. Matches `BingleJsiErrorInfo` in ts/index.ts and the iOS bridge.
     */
    fun info(e: Throwable): Info {
        if (e !is BingleJsiException) return Info("Unknown", emptyMap())
        // bingle-error-kinds:begin
        return when (e) {
            is BingleJsiException.NotFound -> Info("NotFound", mapOf("reason" to e.reason))
            is BingleJsiException.InvalidRequest -> Info("InvalidRequest", mapOf("reason" to e.reason))
            is BingleJsiException.NotImplemented -> Info("NotImplemented", mapOf("reason" to e.reason))
            is BingleJsiException.InternalException -> Info("InternalError", mapOf("reason" to e.reason))
            is BingleJsiException.NoBlockchain -> Info("NoBlockchain", mapOf("reason" to e.reason))
            is BingleJsiException.Retryable -> Info("Retryable", mapOf("reason" to e.reason))
            is BingleJsiException.HandleTaken -> Info("HandleTaken", mapOf("reason" to e.reason))
            is BingleJsiException.KeyStorageUnavailable -> Info("KeyStorageUnavailable", mapOf("reason" to e.reason))
            is BingleJsiException.StateLocked -> Info("StateLocked", mapOf("reason" to e.reason))
            is BingleJsiException.RecoveryPhraseRequired -> Info("RecoveryPhraseRequired", mapOf("reason" to e.reason))
            is BingleJsiException.SyncedKeyWouldBeReplaced ->
                Info("SyncedKeyWouldBeReplaced", mapOf("storedAddress" to e.storedAddress))
            is BingleJsiException.SendFailed ->
                Info("SendFailed", mapOf("failureKind" to failureKindName(e.failureKind), "reason" to e.reason))
        }
        // bingle-error-kinds:end
    }

    /**
     * The rejection message. For an engine error it is in the iOS form, `<Variant>(field: "value")`,
     * for example `NoBlockchain(reason: "node timed out")`, so apps can read it the same way on both
     * platforms (bingle_rust #464). UniFFI's own Kotlin message is only `reason=...`. Any other
     * error keeps its own message.
     */
    fun message(e: Throwable): String {
        if (e !is BingleJsiException) return e.message ?: e.toString()
        val info = info(e)
        return info.fields.entries.joinToString(", ", "${info.kind}(", ")") { (name, value) ->
            "$name: ${quoted(value)}"
        }
    }

    /** A string as Swift's description prints it: in double quotes, with escapes. */
    private fun quoted(value: String): String {
        val escaped = StringBuilder()
        for (c in value) {
            when (c) {
                '\\' -> escaped.append("\\\\")
                '"' -> escaped.append("\\\"")
                '\n' -> escaped.append("\\n")
                '\r' -> escaped.append("\\r")
                '\t' -> escaped.append("\\t")
                else -> escaped.append(c)
            }
        }
        return "\"$escaped\""
    }

    /**
     * Map the typed send-failure cause (issue #99) to the FailureKind string the TypeScript layer
     * expects (matches the FailureKind union in ts/NativeBingleJsi.ts).
     */
    fun failureKindName(kind: FailureKind): String = when (kind) {
        FailureKind.HANDLE_NOT_FOUND -> "HandleNotFound"
        FailureKind.HANDLE_LOOKUP_FAILED -> "HandleLookupFailed"
        FailureKind.RECIPIENT_NOT_ADVERTISED -> "RecipientNotAdvertised"
        FailureKind.INVALID_RECIPIENT_ID -> "InvalidRecipientId"
        FailureKind.NO_RELAY_AVAILABLE -> "NoRelayAvailable"
        FailureKind.RELAY_ALLOCATION_FAILED -> "RelayAllocationFailed"
        FailureKind.PEER_UNREACHABLE -> "PeerUnreachable"
        FailureKind.SEND_BACKOFF -> "SendBackoff"
        FailureKind.NO_RESPONSE -> "NoResponse"
        FailureKind.MALFORMED_ADVERT -> "MalformedAdvert"
        FailureKind.PROTOCOL_ERROR -> "ProtocolError"
        FailureKind.NOT_READY -> "NotReady"
        FailureKind.UNKNOWN -> "Unknown"
    }
}
