package com.pilight.android.data

import android.content.Context
import org.json.JSONObject

class CompanionStore(context: Context) {
    private val prefs =
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun load(): CompanionSession? {
        val raw = prefs.getString(KEY, null) ?: return null
        return runCatching {
            val obj = JSONObject(raw)
            val token = obj.optString("token")
            val relay = obj.optString("relayUrl")
            val deviceId = obj.optString("deviceId")
            if (token.isBlank() || relay.isBlank() || deviceId.isBlank()) {
                null
            } else {
                CompanionSession(
                    deviceId = deviceId,
                    token = token,
                    relayUrl = normalizeRelayUrl(relay),
                    hostName = obj.optString("hostName").ifBlank { "desktop" },
                    chatPath = obj.optString("chatPath").ifBlank { "/v1/chat" },
                    pairedAt = obj.optLong("pairedAt", 0L),
                )
            }
        }.getOrNull()
    }

    fun save(session: CompanionSession) {
        val obj =
            JSONObject()
                .put("deviceId", session.deviceId)
                .put("token", session.token)
                .put("relayUrl", session.relayUrl)
                .put("hostName", session.hostName)
                .put("chatPath", session.chatPath)
                .put("pairedAt", session.pairedAt)
        prefs.edit().putString(KEY, obj.toString()).apply()
    }

    fun clear() {
        prefs.edit().remove(KEY).apply()
    }

    companion object {
        private const val PREFS = "pi_light_companion"
        private const val KEY = "session"
    }
}
