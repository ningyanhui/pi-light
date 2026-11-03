package com.pilight.android

import android.app.Application
import android.os.Build
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.pilight.android.data.ChatLine
import com.pilight.android.data.CompanionApi
import com.pilight.android.data.CompanionSession
import com.pilight.android.data.CompanionStore
import com.pilight.android.data.DesktopMode
import com.pilight.android.data.PairCodes
import com.pilight.android.data.PairPayload
import com.pilight.android.data.SecurityPrefs
import com.pilight.android.data.StreamEvent
import com.pilight.android.data.ToolCard
import com.pilight.android.data.describeChatError
import com.pilight.android.data.describeClaimError
import com.pilight.android.data.describeDesktopError
import com.pilight.android.data.describeStopError
import com.pilight.android.data.normalizeRelayUrl
import com.pilight.android.data.parseToolCard
import com.pilight.android.data.upsertTool
import org.json.JSONObject
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

data class HomeUi(
    val session: CompanionSession? = null,
    val connected: Boolean = false,
    val latencyMs: Long? = null,
    val hostName: String = "",
    val error: String? = null,
    val pairing: Boolean = false,
    val lastCode: String? = null,
)

data class DesktopUi(
    val active: Boolean = false,
    val mode: DesktopMode = DesktopMode.BROWSE,
    val jpeg: ByteArray? = null,
    val width: Int = 0,
    val height: Int = 0,
    val seq: Long = 0,
    val fps: Double = 0.0,
    val capture: String = "unknown",
    val input: String = "unknown",
    val captureMessage: String = "",
    val error: String? = null,
)

data class ChatUi(
    val title: String = "会话",
    val messages: List<ChatLine> = emptyList(),
    val tools: List<ToolCard> = emptyList(),
    val streaming: Boolean = false,
    val canStop: Boolean = false,
    val stopping: Boolean = false,
    val partial: String = "",
    val sending: Boolean = false,
    val error: String? = null,
    val writable: Boolean = true,
)

class CompanionViewModel(app: Application) : AndroidViewModel(app) {
    private val store = CompanionStore(app)
    private val api = CompanionApi()

    private val _home = MutableStateFlow(HomeUi(session = store.load()))
    val home: StateFlow<HomeUi> = _home

    private val _chat = MutableStateFlow(ChatUi())
    val chat: StateFlow<ChatUi> = _chat

    private val _security = MutableStateFlow(store.loadSecurity())
    val security: StateFlow<SecurityPrefs> = _security

    private val _desktop = MutableStateFlow(DesktopUi())
    val desktop: StateFlow<DesktopUi> = _desktop

    private var pollJob: Job? = null
    private var desktopJob: Job? = null
    private var lastDesktopActivityMs: Long = 0L
    private var lastFrameAtNs: Long = 0L

    init {
        if (_home.value.session != null) {
            startPolling()
        }
    }

    fun pairFromQr(raw: String) {
        val parsed = PairPayload.parse(raw)
        parsed.fold(
            onSuccess = { payload ->
                val relay = payload.relay
                if (relay.isNullOrBlank()) {
                    _home.update {
                        it.copy(error = "二维码里没有中继地址。请改用手输配对码，并填写桌面上的公网中继 URL。")
                    }
                } else {
                    claim(payload.code, relay, payload.claimPath, payload.chatPath)
                }
            },
            onFailure = { e ->
                _home.update { it.copy(error = e.message ?: "无法解析二维码") }
            },
        )
    }

    fun pairFromTyped(codeRaw: String, relayRaw: String) {
        val code = PairCodes.normalize(codeRaw)
        if (!PairCodes.isPlausible(code)) {
            _home.update { it.copy(error = "请输入桌面上的 6 位配对码") }
            return
        }
        val relay = normalizeRelayUrl(relayRaw)
        if (relay.isBlank()) {
            _home.update {
                it.copy(error = "手输配对必须同时填写桌面显示的中继地址。临时隧道每次启动都会变，不能只靠局域网。")
            }
            return
        }
        claim(code, relay, "/v1/pair/claim", "/v1/chat")
    }

    private fun claim(
        code: String,
        relay: String,
        claimPath: String,
        chatPath: String,
    ) {
        viewModelScope.launch {
            _home.update {
                it.copy(pairing = true, error = null, lastCode = PairCodes.display(code))
            }
            val resp =
                withContext(Dispatchers.IO) {
                    api.claim(
                        relay = relay,
                        claimPath = claimPath,
                        code = code,
                        deviceName = deviceLabel(),
                        platform = "Android",
                    )
                }
            if (!resp.ok) {
                _home.update {
                    it.copy(pairing = false, error = describeClaimError(resp))
                }
                return@launch
            }
            val token = resp.body.optString("token")
            val deviceId = resp.body.optString("device_id")
            val returnedRelay =
                resp.body.optString("relay_url").ifBlank { relay }.let(::normalizeRelayUrl)
            val host = resp.body.optString("host").ifBlank { "desktop" }
            val path = resp.body.optString("chat_path").ifBlank { chatPath }
            if (token.isBlank() || deviceId.isBlank()) {
                _home.update { it.copy(pairing = false, error = "配对响应缺少 token") }
                return@launch
            }
            val session =
                CompanionSession(
                    deviceId = deviceId,
                    token = token,
                    relayUrl = returnedRelay,
                    hostName = host,
                    chatPath = path.ifBlank { "/v1/chat" },
                    pairedAt = System.currentTimeMillis(),
                )
            store.save(session)
            _home.update {
                it.copy(
                    session = session,
                    pairing = false,
                    connected = false,
                    hostName = host,
                    error = null,
                )
            }
            startPolling()
        }
    }

    fun unpair() {
        leaveDesktop()
        pollJob?.cancel()
        pollJob = null
        store.clear()
        _home.value = HomeUi()
        _chat.value = ChatUi()
        _desktop.value = DesktopUi()
    }

    fun enterDesktop(mode: DesktopMode) {
        val session = _home.value.session ?: return
        lastDesktopActivityMs = System.currentTimeMillis()
        viewModelScope.launch {
            val resp =
                withContext(Dispatchers.IO) {
                    api.desktopSession(session, "start", mode.wireName)
                }
            if (!resp.ok) {
                _desktop.update { it.copy(error = describeDesktopError(resp), active = false) }
                return@launch
            }
            val info = api.parseDesktopSession(resp.body)
            _desktop.update {
                it.copy(
                    active = true,
                    mode = DesktopMode.parse(info.mode),
                    width = info.width,
                    height = info.height,
                    seq = info.seq,
                    capture = info.capture,
                    input = info.input,
                    captureMessage = info.captureMessage,
                    error = null,
                )
            }
            startDesktopLoop()
        }
    }

    fun setDesktopMode(mode: DesktopMode) {
        val session = _home.value.session ?: return
        lastDesktopActivityMs = System.currentTimeMillis()
        _desktop.update { it.copy(mode = mode) }
        viewModelScope.launch {
            val resp =
                withContext(Dispatchers.IO) {
                    api.desktopSession(session, "mode", mode.wireName)
                }
            if (!resp.ok) {
                _desktop.update { it.copy(error = describeDesktopError(resp)) }
            }
        }
    }

    fun leaveDesktop() {
        desktopJob?.cancel()
        desktopJob = null
        val session = _home.value.session
        val wasActive = _desktop.value.active
        _desktop.update { it.copy(active = false) }
        if (session != null && wasActive) {
            viewModelScope.launch {
                withContext(Dispatchers.IO) {
                    runCatching { api.desktopSession(session, "stop", DesktopMode.BROWSE.wireName) }
                }
            }
        }
    }

    fun sendPointer(
        x: Float,
        y: Float,
        action: String,
        button: String = "left",
    ) {
        sendDesktopJson(
            JSONObject()
                .put("type", "pointer")
                .put("x", x.toDouble())
                .put("y", y.toDouble())
                .put("action", action)
                .put("button", button),
        )
    }

    fun sendScroll(
        x: Float,
        y: Float,
        dy: Float,
    ) {
        sendDesktopJson(
            JSONObject()
                .put("type", "scroll")
                .put("x", x.toDouble())
                .put("y", y.toDouble())
                .put("dy", dy.toDouble()),
        )
    }

    fun sendDesktopText(text: String) {
        val trimmed = text.trim()
        if (trimmed.isEmpty()) return
        sendDesktopJson(JSONObject().put("type", "text").put("text", trimmed.take(1024)))
    }

    fun sendDesktopKey(key: String) {
        sendDesktopJson(JSONObject().put("type", "key").put("key", key).put("action", "click"))
    }

    private fun sendDesktopJson(payload: JSONObject) {
        val session = _home.value.session ?: return
        if (!_desktop.value.mode.injectsInput) return
        lastDesktopActivityMs = System.currentTimeMillis()
        viewModelScope.launch {
            val resp = withContext(Dispatchers.IO) { api.sendDesktopInput(session, payload) }
            if (!resp.ok && resp.error != "view_only") {
                _desktop.update { it.copy(error = describeDesktopError(resp)) }
            }
        }
    }

    private fun startDesktopLoop() {
        desktopJob?.cancel()
        desktopJob =
            viewModelScope.launch {
                while (isActive) {
                    pullDesktopFrame()
                    delay(120)
                }
            }
    }

    private suspend fun pullDesktopFrame() {
        val session = _home.value.session ?: return
        val prev = _desktop.value
        val frame =
            withContext(Dispatchers.IO) {
                api.desktopFrame(session, prev.seq)
            }
        if (frame.notModified) return
        if (!frame.ok) {
            if (frame.status == 401) {
                _desktop.update { it.copy(error = "授权已失效，请重新配对") }
            } else if (frame.status != 204 && frame.error != null) {
                _desktop.update { it.copy(error = frame.error) }
            }
            return
        }
        val now = System.nanoTime()
        val fps =
            if (lastFrameAtNs > 0) {
                val dt = (now - lastFrameAtNs) / 1_000_000_000.0
                if (dt > 0) {
                    val inst = 1.0 / dt
                    if (prev.fps <= 0) inst else prev.fps * 0.7 + inst * 0.3
                } else {
                    prev.fps
                }
            } else {
                prev.fps
            }
        lastFrameAtNs = now
        _desktop.update {
            it.copy(
                jpeg = frame.jpeg,
                width = frame.width,
                height = frame.height,
                seq = frame.seq,
                fps = fps,
                capture = frame.capture.ifBlank { it.capture },
                error = null,
            )
        }
    }

    fun updateSecurity(transform: (SecurityPrefs) -> SecurityPrefs) {
        val next = transform(_security.value)
        store.saveSecurity(next)
        _security.value = next
    }

    fun markDesktopActivity() {
        lastDesktopActivityMs = System.currentTimeMillis()
    }

    fun desktopTimedOut(): Boolean {
        val window = _security.value.timeoutMinutes.coerceAtLeast(1) * 60_000L
        if (lastDesktopActivityMs == 0L) return false
        return System.currentTimeMillis() - lastDesktopActivityMs > window
    }

    fun clearError() {
        _home.update { it.copy(error = null) }
        _chat.update { it.copy(error = null) }
    }

    fun send(text: String) {
        val session = _home.value.session ?: return
        val trimmed = text.trim()
        if (trimmed.isEmpty() || _chat.value.sending) return
        viewModelScope.launch {
            _chat.update { state ->
                state.copy(
                    sending = true,
                    streaming = true,
                    canStop = true,
                    stopping = false,
                    error = null,
                    messages = state.messages + ChatLine(role = "user", text = trimmed),
                    partial = "",
                    tools = emptyList(),
                )
            }
            val resp =
                withContext(Dispatchers.IO) {
                    api.sendChatStream(session, trimmed) { ev -> applyStreamEvent(ev) }
                }
            if (!resp.ok) {
                _chat.update {
                    it.copy(
                        sending = false,
                        streaming = false,
                        canStop = false,
                        stopping = false,
                        error = describeChatError(resp),
                    )
                }
                refreshOnce()
                return@launch
            }
            applySettledReply(resp.body.optString("reply"), resp.body.optBoolean("aborted", false))
            refreshOnce()
        }
    }

    fun stop() {
        val session = _home.value.session ?: return
        val chat = _chat.value
        if (!chat.streaming && !chat.sending && !chat.canStop) return
        if (chat.stopping) return
        viewModelScope.launch {
            _chat.update { it.copy(stopping = true, error = null) }
            val resp = withContext(Dispatchers.IO) { api.stopChat(session) }
            if (!resp.ok && resp.error != "not_streaming") {
                _chat.update { it.copy(stopping = false, error = describeStopError(resp)) }
            }
        }
    }

    private fun applyStreamEvent(ev: StreamEvent) {
        when (ev.event) {
            "delta" -> {
                val piece = ev.data.optString("text")
                if (piece.isNotEmpty()) {
                    _chat.update { it.copy(partial = it.partial + piece, streaming = true, canStop = true) }
                }
            }
            "tool" -> {
                val card = parseToolCard(ev.data) ?: return
                _chat.update { it.copy(tools = upsertTool(it.tools, card), streaming = true, canStop = true) }
            }
            "settled" -> {
                applySettledReply(
                    ev.data.optString("reply"),
                    ev.data.optBoolean("aborted", false),
                )
            }
        }
    }

    private fun applySettledReply(reply: String, aborted: Boolean) {
        _chat.update { state ->
            if (!state.sending && !state.streaming) {
                return@update state
            }
            val next = state.messages.toMutableList()
            val text = reply.ifBlank { state.partial }
            val already =
                next.lastOrNull()?.let { it.role == "assistant" && it.text == text } == true
            if (!already) {
                when {
                    text.isNotBlank() -> next.add(ChatLine(role = "assistant", text = text))
                    aborted -> next.add(ChatLine(role = "assistant", text = "[已中止]"))
                }
            }
            state.copy(
                sending = false,
                streaming = false,
                canStop = false,
                stopping = false,
                partial = "",
                messages = next,
            )
        }
    }

    private fun startPolling() {
        pollJob?.cancel()
        pollJob =
            viewModelScope.launch {
                while (isActive) {
                    refreshOnce()
                    delay(2_000)
                }
            }
    }

    private suspend fun refreshOnce() {
        val session = _home.value.session ?: return
        val ping =
            withContext(Dispatchers.IO) {
                runCatching { api.health(session.relayUrl) }.getOrNull()
            }
        if (ping == null) {
            _home.update { it.copy(connected = false, latencyMs = null) }
            return
        }
        val (health, latency) = ping
        val connected = health.ok
        _home.update {
            it.copy(
                connected = connected,
                latencyMs = if (connected) latency else null,
            )
        }
        if (!connected) return
        if (_chat.value.sending) {
            // POST 进行中时仍拉快照，便于显示桌面流式片段。
        }
        val snapResp = withContext(Dispatchers.IO) { api.chatSnapshot(session) }
        if (!snapResp.ok) {
            if (snapResp.status == 401) {
                _home.update { it.copy(error = describeChatError(snapResp), connected = false) }
            }
            return
        }
        val snap = api.parseSnapshot(snapResp.body)
        _home.update {
            it.copy(
                hostName = snap.host.ifBlank { it.session?.hostName.orEmpty() },
                connected = true,
            )
        }
        if (!_chat.value.sending) {
            _chat.update {
                val messages =
                    if (snap.messages.size >= it.messages.size) snap.messages else it.messages
                val tools = if (snap.tools.isNotEmpty()) snap.tools else it.tools
                it.copy(
                    title = snap.sessionTitle,
                    messages = messages,
                    tools = tools,
                    streaming = snap.streaming,
                    canStop = snap.canStop || snap.streaming,
                    partial = snap.partial,
                    writable = snap.writable,
                    error = it.error,
                )
            }
        } else {
            _chat.update { state ->
                val tools = if (snap.tools.isNotEmpty()) snap.tools else state.tools
                val partial =
                    when {
                        state.partial.isNotBlank() -> state.partial
                        snap.partial.isNotBlank() -> snap.partial
                        else -> state.partial
                    }
                state.copy(
                    title = snap.sessionTitle.ifBlank { state.title },
                    tools = tools,
                    streaming = true,
                    canStop = true,
                    partial = partial,
                    writable = snap.writable,
                )
            }
        }
    }

    private fun deviceLabel(): String {
        val model = Build.MODEL?.trim().orEmpty()
        return if (model.isBlank()) "Android" else model.take(64)
    }
}
