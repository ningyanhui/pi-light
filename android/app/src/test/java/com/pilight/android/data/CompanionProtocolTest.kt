package com.pilight.android.data

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CompanionProtocolTest {
    @Test
    fun parseSnapshotIncludesToolsAndCanStop() {
        val body =
            JSONObject(
                """
                {
                  "ok": true,
                  "writable": true,
                  "streaming": true,
                  "can_stop": true,
                  "partial": "Hel",
                  "host": "desk",
                  "session_title": "日常",
                  "messages": [{"role":"user","text":"hi"}],
                  "tools": [
                    {"id":"c1","name":"bash","status":"running","args":"{\"command\":\"ls\"}"}
                  ]
                }
                """.trimIndent(),
            )
        val snap = CompanionApi().parseSnapshot(body)
        assertEquals("desk", snap.host)
        assertEquals("日常", snap.sessionTitle)
        assertTrue(snap.streaming)
        assertTrue(snap.canStop)
        assertEquals("Hel", snap.partial)
        assertEquals(1, snap.messages.size)
        assertEquals("bash", snap.tools[0].name)
        assertEquals("running", snap.tools[0].status)
        assertEquals("{\"command\":\"ls\"}", snap.tools[0].args)
    }

    @Test
    fun parseSseFramesReadsDeltaToolSettled() {
        val raw =
            """
            event: delta
            data: {"text":"Hel"}

            event: tool
            data: {"id":"c1","name":"bash","status":"running"}

            event: settled
            data: {"ok":true,"reply":"Hello","aborted":false}

            """.trimIndent()
        val events = parseSseFrames(raw)
        assertEquals(listOf("delta", "tool", "settled"), events.map { it.event })
        assertEquals("Hel", events[0].data.optString("text"))
        assertEquals("bash", events[1].data.optString("name"))
        assertEquals("Hello", events[2].data.optString("reply"))
        assertFalse(events[2].data.optBoolean("aborted"))
    }

    @Test
    fun upsertToolReplacesSameIdAndKeepsArgs() {
        val first = ToolCard(id = "c1", name = "bash", status = "running", args = "ls")
        val done = ToolCard(id = "c1", name = "bash", status = "done", durationMs = 12)
        val merged = upsertTool(listOf(first), done)
        assertEquals(1, merged.size)
        assertEquals("done", merged[0].status)
        assertEquals("ls", merged[0].args)
        assertEquals(12L, merged[0].durationMs)
    }

    @Test
    fun parseToolCardAcceptsLegacyKeys() {
        val card =
            parseToolCard(
                JSONObject("""{"tool_call_id":"x","tool_name":"read","status":"error"}"""),
            )
        assertEquals("x", card?.id)
        assertEquals("read", card?.name)
        assertEquals("error", card?.status)
        assertNull(card?.args)
    }

    @Test
    fun speechAndStopErrorsAreHonest() {
        assertTrue(describeSpeechError(2).contains("网络"))
        assertTrue(describeSpeechError(9).contains("麦克风"))
        assertEquals("没有听清，请再试一次", describeSpeechError(7))
        val stop =
            describeStopError(
                ApiResponse(409, JSONObject().put("ok", false).put("error", "not_streaming")),
            )
        assertTrue(stop.contains("没有正在生成"))
    }
}
