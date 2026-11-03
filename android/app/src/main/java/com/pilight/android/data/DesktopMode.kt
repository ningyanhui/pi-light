package com.pilight.android.data

/** 远程桌面三档。命名钉死为浏览 / 触控 / 全屏，禁止已废弃的误标。 */
enum class DesktopMode(
    val label: String,
    val hint: String,
) {
    BROWSE("浏览", "浏览=只看不点"),
    TOUCH("触控", "触控=点按映射鼠标"),
    FULLSCREEN("全屏", "全屏=横屏沉浸"),
    ;

    companion object {
        val labels: List<String> = entries.map { it.label }

        fun parse(raw: String): DesktopMode =
            entries.find { it.name.equals(raw, ignoreCase = true) || it.label == raw } ?: BROWSE
    }
}

enum class RelayRegion(
    val id: String,
    val label: String,
) {
    ASIA_SINGAPORE("asia-singapore", "亚太（新加坡）"),
    ASIA_TOKYO("asia-tokyo", "亚太（东京）"),
    US_WEST("us-west", "美国西部"),
    EUROPE("europe", "欧洲"),
    ;

    companion object {
        fun fromId(id: String): RelayRegion = entries.find { it.id == id } ?: ASIA_SINGAPORE
    }
}

data class SecurityPrefs(
    val biometricLock: Boolean = false,
    val timeoutMinutes: Int = 30,
    val relayRegionId: String = RelayRegion.ASIA_SINGAPORE.id,
) {
    val relayRegion: RelayRegion get() = RelayRegion.fromId(relayRegionId)
    val timeoutLabel: String get() = "${timeoutMinutes}分钟"
}

object TimeoutOptions {
    val minutes: List<Int> = listOf(5, 15, 30, 60)
}
