package com.pilight.android.data

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DesktopProtocolTest {
    @Test
    fun sessionPathsStayOnV1() {
        assertEquals("/v1/desktop/session", CompanionApi.DESKTOP_SESSION)
        assertEquals("/v1/desktop/frame", CompanionApi.DESKTOP_FRAME)
        assertEquals("/v1/desktop/input", CompanionApi.DESKTOP_INPUT)
        assertEquals(
            "https://demo.trycloudflare.com/v1/desktop/frame",
            CompanionApi.join("https://demo.trycloudflare.com/", CompanionApi.DESKTOP_FRAME),
        )
    }

    @Test
    fun parseDesktopSession() {
        val body =
            JSONObject(
                """
                {"ok":true,"active":true,"mode":"touch","width":1280,"height":720,"seq":9,
                 "capture":"ok","input":"ok","capture_message":"","allow_preview":true,"allow_touch":true}
                """.trimIndent(),
            )
        val info = CompanionApi().parseDesktopSession(body)
        assertTrue(info.active)
        assertEquals("touch", info.mode)
        assertEquals(1280, info.width)
        assertEquals(720, info.height)
        assertEquals(9L, info.seq)
        assertEquals("ok", info.capture)
        assertTrue(info.allowTouch)
    }

    @Test
    fun browseDoesNotInject() {
        assertFalse(DesktopMode.BROWSE.injectsInput)
        assertEquals("view_only", describeDesktopError(ApiResponse(403, JSONObject().put("error", "view_only"))).let {
            // keep wording aligned with 浏览
            assertTrue(it.contains("浏览"))
            "view_only"
        })
    }
}
