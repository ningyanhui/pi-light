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

    fun loadSecurity(): SecurityPrefs =
        SecurityPrefs(
            biometricLock = prefs.getBoolean(SEC_BIO, false),
            timeoutMinutes = prefs.getInt(SEC_TIMEOUT, 30).let { m ->
                if (m in TimeoutOptions.minutes) m else 30
            },
            relayRegionId = prefs.getString(SEC_REGION, RelayRegion.ASIA_SINGAPORE.id)
                ?: RelayRegion.ASIA_SINGAPORE.id,
        )

    fun saveSecurity(prefsValue: SecurityPrefs) {
        this.prefs
            .edit()
            .putBoolean(SEC_BIO, prefsValue.biometricLock)
            .putInt(SEC_TIMEOUT, prefsValue.timeoutMinutes)
            .putString(SEC_REGION, prefsValue.relayRegionId)
            .apply()
    }

    companion object {
        private const val PREFS = "pi_light_companion"
        private const val KEY = "session"
        private const val SEC_BIO = "sec_biometric"
        private const val SEC_TIMEOUT = "sec_timeout_min"
        private const val SEC_REGION = "sec_relay_region"
    }
}
