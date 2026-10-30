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

data class ChatSnapshot(
    val writable: Boolean,
    val streaming: Boolean,
    val partial: String,
    val host: String,
    val sessionTitle: String,
    val messages: List<ChatLine>,
    val relay: String?,
)
