package com.pilight.android.data

import org.json.JSONObject

data class PairPayload(
    val version: Int,
    val app: String,
    val code: String,
    val claimPath: String,
    val chatPath: String,
    val protocol: String,
    val exp: Long?,
    val relay: String?,
) {
    companion object {
        fun parse(raw: String): Result<PairPayload> =
            runCatching {
                val obj = JSONObject(raw.trim())
                val app = obj.optString("app")
                if (app.isNotEmpty() && app != "pi-light") {
                    error("不是 pi-light 配对码")
                }
                val code = PairCodes.normalize(obj.optString("code"))
                if (!PairCodes.isPlausible(code)) {
                    error("配对码无效")
                }
                val relay = obj.optString("relay").trim().ifEmpty { null }
                PairPayload(
                    version = obj.optInt("v", 1),
                    app = app.ifEmpty { "pi-light" },
                    code = code,
                    claimPath = obj.optString("claim").ifEmpty { "/v1/pair/claim" },
                    chatPath = obj.optString("chat").ifEmpty { "/v1/chat" },
                    protocol = obj.optString("protocol").ifEmpty { "pi-light-companion/1" },
                    exp = if (obj.has("exp") && !obj.isNull("exp")) obj.optLong("exp") else null,
                    relay = relay?.let { normalizeRelayUrl(it) },
                )
            }
    }
}

object PairCodes {
    fun normalize(raw: String): String = raw.filter { it.isLetterOrDigit() }.uppercase()

    fun isPlausible(code: String): Boolean = normalize(code).length == 6

    fun display(code: String): String {
        val n = normalize(code)
        return if (n.length == 6) "${n.substring(0, 3)} ${n.substring(3)}" else n
    }
}

fun normalizeRelayUrl(raw: String): String {
    var s = raw.trim()
    if (s.isEmpty()) return s
    if (!s.startsWith("http://", ignoreCase = true) &&
        !s.startsWith("https://", ignoreCase = true)
    ) {
        s = "https://$s"
    }
    return s.trimEnd('/')
}

data class CompanionSession(
    val deviceId: String,
    val token: String,
    val relayUrl: String,
    val hostName: String,
    val chatPath: String,
    val pairedAt: Long,
)

data class ChatLine(
    val role: String,
    val text: String,
)

data class ToolCard(
    val id: String,
    val name: String,
    val status: String,
    val args: String? = null,
    val durationMs: Long? = null,
)

data class ChatSnapshot(
    val writable: Boolean,
    val streaming: Boolean,
    val canStop: Boolean,
    val partial: String,
    val host: String,
    val sessionTitle: String,
    val messages: List<ChatLine>,
    val tools: List<ToolCard>,
    val relay: String?,
)

data class StreamEvent(
    val event: String,
    val data: org.json.JSONObject,
)

fun parseToolCard(item: org.json.JSONObject): ToolCard? {
    val id = item.optString("id").ifBlank { item.optString("tool_call_id") }
    val name = item.optString("name").ifBlank { item.optString("tool_name") }
    if (id.isBlank() && name.isBlank()) return null
    val args = item.optString("args").ifBlank { null }
    val duration =
        if (item.has("duration_ms") && !item.isNull("duration_ms")) {
            item.optLong("duration_ms")
        } else {
            null
        }
    return ToolCard(
        id = id.ifBlank { name },
        name = name.ifBlank { "tool" },
        status = item.optString("status").ifBlank { "done" },
        args = args,
        durationMs = duration,
    )
}

fun upsertTool(tools: List<ToolCard>, incoming: ToolCard): List<ToolCard> {
    val next = tools.toMutableList()
    val idx = next.indexOfFirst { it.id == incoming.id }
    if (idx >= 0) {
        val prev = next[idx]
        next[idx] = incoming.copy(args = incoming.args ?: prev.args)
    } else {
        next.add(incoming)
    }
    return next
}

fun parseSseFrames(text: String): List<StreamEvent> {
    val events = mutableListOf<StreamEvent>()
    var event = "message"
    val data = StringBuilder()
    fun flush() {
        if (data.isEmpty()) return
        val obj =
            runCatching { org.json.JSONObject(data.toString()) }.getOrElse {
                org.json.JSONObject().put("raw", data.toString())
            }
        events.add(StreamEvent(event, obj))
        event = "message"
        data.clear()
    }
    for (raw in text.split("\n")) {
        val line = raw.trimEnd('\r')
        when {
            line.isEmpty() -> flush()
            line.startsWith("event:") -> event = line.substringAfter("event:").trim()
            line.startsWith("data:") -> {
                if (data.isNotEmpty()) data.append('\n')
                data.append(line.substringAfter("data:").trim())
            }
        }
    }
    flush()
    return events
}
