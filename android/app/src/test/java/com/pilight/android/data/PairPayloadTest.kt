package com.pilight.android.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PairPayloadTest {
    @Test
    fun parseDesktopQrJson() {
        val raw =
            """
            {"v":1,"app":"pi-light","code":"AB12CD","claim":"/v1/pair/claim","chat":"/v1/chat",
             "protocol":"pi-light-companion/1","exp":1730000000,"relay":"https://demo.trycloudflare.com"}
            """.trimIndent()
        val parsed = PairPayload.parse(raw).getOrThrow()
        assertEquals("AB12CD", parsed.code)
        assertEquals("https://demo.trycloudflare.com", parsed.relay)
        assertEquals("/v1/pair/claim", parsed.claimPath)
        assertEquals("/v1/chat", parsed.chatPath)
        assertEquals("pi-light", parsed.app)
    }

    @Test
    fun rejectForeignApp() {
        val raw = """{"app":"other","code":"AB12CD","relay":"https://x.example"}"""
        assertTrue(PairPayload.parse(raw).isFailure)
    }

    @Test
    fun normalizeCodeStripsSpacesAndCase() {
        assertEquals("AB12CD", PairCodes.normalize("ab 12 cd"))
        assertEquals("AB12CD", PairCodes.display("ab12cd").replace(" ", ""))
        assertTrue(PairCodes.isPlausible("746 382"))
        assertFalse(PairCodes.isPlausible("12"))
    }

    @Test
    fun relayUrlGetsHttpsAndLosesTrailingSlash() {
        assertEquals("https://demo.trycloudflare.com", normalizeRelayUrl("demo.trycloudflare.com/"))
        assertEquals("http://10.0.2.2:17831", normalizeRelayUrl("http://10.0.2.2:17831/"))
    }

    @Test
    fun missingRelayIsNullNotLanStory() {
        val raw = """{"app":"pi-light","code":"AB12CD"}"""
        val parsed = PairPayload.parse(raw).getOrThrow()
        assertNull(parsed.relay)
    }

    @Test
    fun joinKeepsPath() {
        assertEquals(
            "https://demo.trycloudflare.com/v1/pair/claim",
            CompanionApi.join("https://demo.trycloudflare.com/", "/v1/pair/claim"),
        )
    }
}
