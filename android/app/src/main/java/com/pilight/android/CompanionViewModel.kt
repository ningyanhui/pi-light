package com.pilight.android

import android.app.Application
import android.os.Build
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.pilight.android.data.ChatLine
import com.pilight.android.data.CompanionApi
import com.pilight.android.data.CompanionSession
import com.pilight.android.data.CompanionStore
import com.pilight.android.data.PairCodes
import com.pilight.android.data.PairPayload
import com.pilight.android.data.describeChatError
import com.pilight.android.data.describeClaimError
import com.pilight.android.data.normalizeRelayUrl
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

data class ChatUi(
    val title: String = "会话",
    val messages: List<ChatLine> = emptyList(),
    val streaming: Boolean = false,
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

    private var pollJob: Job? = null

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
        pollJob?.cancel()
        pollJob = null
        store.clear()
        _home.value = HomeUi()
        _chat.value = ChatUi()
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
                    error = null,
                    messages = state.messages + ChatLine(role = "user", text = trimmed),
                    partial = "",
                )
            }
            val resp = withContext(Dispatchers.IO) { api.sendChat(session, trimmed) }
            if (!resp.ok) {
                _chat.update {
                    it.copy(sending = false, streaming = false, error = describeChatError(resp))
                }
                refreshOnce()
                return@launch
            }
            val reply = resp.body.optString("reply")
            _chat.update { state ->
                val next = state.messages.toMutableList()
                if (reply.isNotBlank()) {
                    next.add(ChatLine(role = "assistant", text = reply))
                }
                state.copy(
                    sending = false,
                    streaming = false,
                    partial = "",
                    messages = next,
                )
            }
            refreshOnce()
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
                it.copy(
                    title = snap.sessionTitle,
                    messages = snap.messages,
                    streaming = snap.streaming,
                    partial = snap.partial,
                    writable = snap.writable,
                    error = it.error,
                )
            }
        } else if (snap.partial.isNotBlank() || snap.streaming) {
            _chat.update { it.copy(partial = snap.partial, streaming = true) }
        }
    }

    private fun deviceLabel(): String {
        val model = Build.MODEL?.trim().orEmpty()
        return if (model.isBlank()) "Android" else model.take(64)
    }
}
