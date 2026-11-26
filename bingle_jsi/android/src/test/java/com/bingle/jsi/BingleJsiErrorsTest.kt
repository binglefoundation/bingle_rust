package com.bingle.jsi

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bingle_jsi.BingleJsiException
import uniffi.bingle_jsi.FailureKind

/** How the Android bridge reports failed calls (bingle_rust #464, #470). */
class BingleJsiErrorsTest {
    @Test
    fun syncedKeyWouldBeReplacedCarriesStoredAddress() {
        val e = BingleJsiException.SyncedKeyWouldBeReplaced("STOREDADDRESSEXAMPLE")
        assertEquals(
            BingleJsiErrors.Info("SyncedKeyWouldBeReplaced", mapOf("storedAddress" to "STOREDADDRESSEXAMPLE")),
            BingleJsiErrors.info(e)
        )
        assertEquals("SyncedKeyWouldBeReplaced(storedAddress: \"STOREDADDRESSEXAMPLE\")", BingleJsiErrors.message(e))
    }

    @Test
    fun noBlockchainUsesTheIosMessageForm() {
        val e = BingleJsiException.NoBlockchain("node timed out")
        assertEquals(BingleJsiErrors.Info("NoBlockchain", mapOf("reason" to "node timed out")), BingleJsiErrors.info(e))
        assertEquals("NoBlockchain(reason: \"node timed out\")", BingleJsiErrors.message(e))
    }

    @Test
    fun internalExceptionIsReportedAsInternalError() {
        val e = BingleJsiException.InternalException("boom")
        assertEquals("InternalError", BingleJsiErrors.info(e).kind)
        assertEquals("InternalError(reason: \"boom\")", BingleJsiErrors.message(e))
    }

    @Test
    fun sendFailedCarriesTheFailureKind() {
        val e = BingleJsiException.SendFailed(FailureKind.SEND_BACKOFF, "Sending is failing")
        assertEquals(
            BingleJsiErrors.Info("SendFailed", mapOf("failureKind" to "SendBackoff", "reason" to "Sending is failing")),
            BingleJsiErrors.info(e)
        )
    }

    @Test
    fun quotesAndBackslashesAreEscapedAsSwiftDoes() {
        val e = BingleJsiException.InvalidRequest("say \"hi\"\\now\n")
        assertEquals("InvalidRequest(reason: \"say \\\"hi\\\"\\\\now\\n\")", BingleJsiErrors.message(e))
    }

    @Test
    fun nonEngineErrorIsUnknownAndKeepsItsMessage() {
        val e = IllegalStateException("not an engine error")
        assertEquals(BingleJsiErrors.Info("Unknown", emptyMap()), BingleJsiErrors.info(e))
        assertEquals("not an engine error", BingleJsiErrors.message(e))
    }
}
