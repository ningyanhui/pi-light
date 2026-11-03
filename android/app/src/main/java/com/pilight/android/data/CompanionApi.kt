package com.pilight.android.data

import java.io.IOException
import java.util.concurrent.TimeUnit
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject

data class DesktopSessionInfo(
    val active: Boolean,
    val mode: String,
    val width: Int,
    val height: Int,
    val seq: Long,
    val capture: String,
    val input: String,
    val captureMessage: String,
    val allowPreview: Boolean,
    val allowTouch: Boolean,
)

data class FrameResult(
    val status: Int,
    val jpeg: ByteArray?,
    val width: Int,
    val height: Int,
    val seq: Long,
    val mode: String,
    val capture: String,
    val error: String?,
) {
    val ok: Boolean get() = status in 200..299 && jpeg != null
    val notModified: Boolean get() = status == 304
}

data class ApiResponse(
    val status: Int,
    val body: JSONObject,
) {
    val ok: Boolean get() = status in 200..299 && body.optBoolean("ok", status in 200..299)
    val error: String get() = body.optString("error").ifBlank { "http_$status" }
    val message: String
        get() =
            body.optString("message").ifBlank {
                body.optString("hint").ifBlank { error }
            }
}

class CompanionApi(
    private val client: OkHttpClient = defaultClient(),
) {
    fun health(relay: String): Pair<ApiResponse, Long> {
        val start = System.nanoTime()
        val resp = get(join(relay, "/health"))
        val ms = (System.nanoTime() - start) / 1_000_000L
        return resp to ms
    }

    fun claim(
        relay: String,
        claimPath: String,
        code: String,
        deviceName: String,
        platform: String,
    ): ApiResponse {
        val payload =
            JSONObject()
                .put("code", code)
                .put("device_name", deviceName)
                .put("platform", platform)
                .toString()
        return post(join(relay, claimPath), payload, token = null)
    }

    fun chatSnapshot(session: CompanionSession): ApiResponse =
        get(join(session.relayUrl, session.chatPath), session.token)

    fun sendChat(
        session: CompanionSession,
        text: String,
    ): ApiResponse {
        val payload = JSONObject().put("text", text).toString()
        return post(join(session.relayUrl, session.chatPath), payload, session.token)
    }

    fun sendChatStream(
        session: CompanionSession,
        text: String,
        onEvent: (StreamEvent) -> Unit,
    ): ApiResponse {
        val payload = JSONObject().put("text", text).put("stream", true).toString()
        val request =
            Request.Builder()
                .url(join(session.relayUrl, session.chatPath))
                .post(payload.toRequestBody(JSON))
                .addHeader("Accept", "text/event-stream")
                .addHeader("User-Agent", USER_AGENT)
                .addHeader("Authorization", "Bearer ${session.token}")
                .build()
        return try {
            client.newCall(request).execute().use { resp ->
                val contentType = resp.header("Content-Type").orEmpty()
                if (contentType.contains("text/event-stream")) {
                    readSse(resp, onEvent)
                } else {
                    parseHttpJson(resp)
                }
            }
        } catch (e: IOException) {
            networkError(e)
        }
    }

    fun stopChat(session: CompanionSession): ApiResponse {
        val url = join(session.relayUrl, "/v1/chat/stop")
        return post(url, "{}", session.token)
    }

    fun desktopSession(
        session: CompanionSession,
        action: String,
        mode: String,
    ): ApiResponse {
        val payload =
            JSONObject()
                .put("action", action)
                .put("mode", mode)
                .toString()
        return post(join(session.relayUrl, DESKTOP_SESSION), payload, session.token)
    }

    fun desktopSnapshot(session: CompanionSession): ApiResponse =
        get(join(session.relayUrl, DESKTOP_SESSION), session.token)

    fun parseDesktopSession(body: JSONObject): DesktopSessionInfo =
        DesktopSessionInfo(
            active = body.optBoolean("active", false),
            mode = body.optString("mode").ifBlank { "browse" },
            width = body.optInt("width"),
            height = body.optInt("height"),
            seq = body.optLong("seq"),
            capture = body.optString("capture").ifBlank { "unknown" },
            input = body.optString("input").ifBlank { "unknown" },
            captureMessage = body.optString("capture_message"),
            allowPreview = body.optBoolean("allow_preview", true),
            allowTouch = body.optBoolean("allow_touch", true),
        )

    fun desktopFrame(
        session: CompanionSession,
        sinceSeq: Long?,
    ): FrameResult {
        val request =
            Request.Builder()
                .url(join(session.relayUrl, DESKTOP_FRAME))
                .get()
                .addHeader("Accept", "image/jpeg")
                .addHeader("User-Agent", USER_AGENT)
                .addHeader("Authorization", "Bearer ${session.token}")
                .apply {
                    if (sinceSeq != null && sinceSeq > 0) {
                        addHeader("If-None-Match", sinceSeq.toString())
                    }
                }
                .build()
        return try {
            frameClient.newCall(request).execute().use { resp ->
                val width = resp.header("X-Pi-Light-Width")?.toIntOrNull() ?: 0
                val height = resp.header("X-Pi-Light-Height")?.toIntOrNull() ?: 0
                val seq = resp.header("X-Pi-Light-Seq")?.toLongOrNull() ?: sinceSeq ?: 0L
                val mode = resp.header("X-Pi-Light-Mode").orEmpty()
                val capture = resp.header("X-Pi-Light-Capture").orEmpty()
                if (resp.code == 304) {
                    return FrameResult(304, null, width, height, seq, mode, capture, null)
                }
                if (resp.code == 204) {
                    return FrameResult(204, null, 0, 0, 0, mode, capture, "no_frame")
                }
                if (!resp.isSuccessful) {
                    val text = resp.body?.string().orEmpty()
                    val err =
                        runCatching { JSONObject(text).optString("error") }
                            .getOrDefault("http_${resp.code}")
                    return FrameResult(resp.code, null, 0, 0, 0, mode, capture, err)
                }
                val bytes = resp.body?.bytes()
                FrameResult(resp.code, bytes, width, height, seq, mode, capture, null)
            }
        } catch (e: IOException) {
            FrameResult(0, null, 0, 0, 0, "", "", e.message ?: "network")
        }
    }

    fun sendDesktopInput(
        session: CompanionSession,
        payload: JSONObject,
    ): ApiResponse = post(join(session.relayUrl, DESKTOP_INPUT), payload.toString(), session.token)

    fun parseSnapshot(body: JSONObject): ChatSnapshot {
        val messages = mutableListOf<ChatLine>()
        val arr: JSONArray = body.optJSONArray("messages") ?: JSONArray()
        for (i in 0 until arr.length()) {
            val item = arr.optJSONObject(i) ?: continue
            messages.add(
                ChatLine(
                    role = item.optString("role"),
                    text = item.optString("text"),
                ),
            )
        }
        val tools = mutableListOf<ToolCard>()
        val toolArr: JSONArray = body.optJSONArray("tools") ?: JSONArray()
        for (i in 0 until toolArr.length()) {
            val item = toolArr.optJSONObject(i) ?: continue
            parseToolCard(item)?.let(tools::add)
        }
        return ChatSnapshot(
            writable = body.optBoolean("writable", true),
            streaming = body.optBoolean("streaming", false),
            canStop = body.optBoolean("can_stop", body.optBoolean("streaming", false)),
            partial = body.optString("partial"),
            host = body.optString("host").ifBlank { "desktop" },
            sessionTitle = body.optString("session_title").ifBlank { "会话" },
            messages = messages,
            tools = tools,
            relay = body.optString("relay").ifBlank { null },
        )
    }

    private fun get(
        url: String,
        token: String? = null,
    ): ApiResponse = execute(
        Request.Builder().url(url).get().apply {
            addHeader("Accept", "application/json")
            addHeader("User-Agent", USER_AGENT)
            if (!token.isNullOrBlank()) {
                addHeader("Authorization", "Bearer $token")
            }
        }.build(),
    )

    private fun post(
        url: String,
        json: String,
        token: String?,
    ): ApiResponse = execute(
        Request.Builder()
            .url(url)
            .post(json.toRequestBody(JSON))
            .addHeader("Accept", "application/json")
            .addHeader("User-Agent", USER_AGENT)
            .apply {
                if (!token.isNullOrBlank()) {
                    addHeader("Authorization", "Bearer $token")
                }
            }
            .build(),
    )

    private fun execute(request: Request): ApiResponse {
        try {
            client.newCall(request).execute().use { resp ->
                return parseHttpJson(resp)
            }
        } catch (e: IOException) {
            return networkError(e)
        }
    }

    private fun parseHttpJson(resp: okhttp3.Response): ApiResponse {
        val text = resp.body?.string().orEmpty()
        val obj =
            if (text.isBlank()) {
                JSONObject()
            } else {
                runCatching { JSONObject(text) }.getOrElse {
                    JSONObject().put("raw", text.take(400))
                }
            }
        if (!obj.has("ok")) {
            obj.put("ok", resp.isSuccessful)
        }
        return ApiResponse(resp.code, obj)
    }

    private fun readSse(
        resp: okhttp3.Response,
        onEvent: (StreamEvent) -> Unit,
    ): ApiResponse {
        val source = resp.body?.source() ?: return ApiResponse(
            resp.code,
            JSONObject().put("ok", false).put("error", "empty_stream"),
        )
        var event = "message"
        val data = StringBuilder()
        var lastSettled: JSONObject? = null
        var lastError: JSONObject? = null
        fun flush() {
            if (data.isEmpty()) return
            val obj =
                runCatching { JSONObject(data.toString()) }.getOrElse {
                    JSONObject().put("raw", data.toString())
                }
            val ev = StreamEvent(event, obj)
            onEvent(ev)
            when (event) {
                "settled" -> lastSettled = obj
                "error" -> lastError = obj
            }
            event = "message"
            data.clear()
        }
        while (!source.exhausted()) {
            val line = source.readUtf8Line() ?: break
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
        lastError?.let { err ->
            if (!err.has("ok")) err.put("ok", false)
            return ApiResponse(if (resp.code in 200..299) statusForChatError(err.optString("error")) else resp.code, err)
        }
        lastSettled?.let { settled ->
            if (!settled.has("ok")) settled.put("ok", true)
            return ApiResponse(200, settled)
        }
        return ApiResponse(
            if (resp.isSuccessful) 502 else resp.code,
            JSONObject().put("ok", false).put("error", "incomplete_stream"),
        )
    }

    private fun networkError(e: IOException): ApiResponse =
        ApiResponse(
            0,
            JSONObject()
                .put("ok", false)
                .put("error", "network")
                .put("message", e.message ?: "network error"),
        )

    private fun statusForChatError(error: String): Int =
        when (error) {
            "busy" -> 409
            "pi_unavailable" -> 503
            "unauthorized" -> 401
            "timeout" -> 504
            else -> 502
        }

    private val frameClient: OkHttpClient =
        client.newBuilder()
            .connectTimeout(8, TimeUnit.SECONDS)
            .readTimeout(12, TimeUnit.SECONDS)
            .writeTimeout(8, TimeUnit.SECONDS)
            .callTimeout(15, TimeUnit.SECONDS)
            .build()

    companion object {
        private val JSON = "application/json; charset=utf-8".toMediaType()
        private const val USER_AGENT = "pi-light-android/0.3"
        const val DESKTOP_SESSION = "/v1/desktop/session"
        const val DESKTOP_FRAME = "/v1/desktop/frame"
        const val DESKTOP_INPUT = "/v1/desktop/input"

        fun defaultClient(): OkHttpClient =
            OkHttpClient.Builder()
                .connectTimeout(15, TimeUnit.SECONDS)
                .readTimeout(120, TimeUnit.SECONDS)
                .writeTimeout(30, TimeUnit.SECONDS)
                .callTimeout(125, TimeUnit.SECONDS)
                .followRedirects(true)
                .build()

        fun join(base: String, path: String): String {
            val b = normalizeRelayUrl(base)
            val p = if (path.startsWith("/")) path else "/$path"
            return b + p
        }
    }
}

fun describeClaimError(resp: ApiResponse): String =
    when (resp.error) {
        "invalid_code" -> "配对码不正确"
        "expired" -> "配对码已过期，请在桌面刷新后重试"
        "no_offer" -> "桌面尚未开放配对。请打开「允许手机远程」并等到中继已连接。"
        "code_required", "invalid_json" -> "请求不完整，请重新扫码或输入配对码"
        "unauthorized" -> "未授权，请重新配对"
        "network" -> "无法连接到中继（${resp.message}）。请确认手机走蜂窝或其它公网，而不是指望同一 Wi‑Fi。"
        else -> "配对失败（HTTP ${resp.status} ${resp.error}）"
    }

fun describeChatError(resp: ApiResponse): String =
    when (resp.error) {
        "unauthorized" -> "授权已失效。桌面可能撤销了这台设备，请重新配对。"
        "busy" -> "桌面正在生成回复，请稍后再发"
        "pi_unavailable" -> "桌面尚未连上 Pi sidecar"
        "timeout" -> "这一轮等待超时。中继或模型可能较慢，请重试。"
        "not_streaming" -> "当前没有正在生成的回复"
        "network" -> "中继网络失败（${resp.message}）"
        else -> "发送失败（HTTP ${resp.status} ${resp.error}）"
    }

fun describeStopError(resp: ApiResponse): String =
    when (resp.error) {
        "not_streaming" -> "当前没有正在生成的回复"
        "unauthorized" -> "授权已失效。桌面可能撤销了这台设备，请重新配对。"
        "network" -> "中继网络失败（${resp.message}）"
        else -> "停止失败（HTTP ${resp.status} ${resp.error}）"
    }

fun describeDesktopError(resp: ApiResponse): String =
    when (resp.error) {
        "unauthorized" -> "授权已失效。桌面可能撤销了这台设备，请重新配对。"
        "preview_disabled" -> "桌面关闭了远程桌面预览。"
        "touch_disabled" -> "桌面关闭了触控操控。"
        "view_only" -> "浏览模式只看画面，不发送点击。"
        "no_session" -> "远程桌面会话尚未开始。"
        "inject_failed" -> "桌面无法注入键鼠（${resp.message}）。Windows 可能拦截了模拟输入。"
        "network" -> "中继网络失败（${resp.message}）"
        else -> "远程桌面失败（HTTP ${resp.status} ${resp.error}）"
    }
