package com.pilight.android.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DesktopModeTest {
    @Test
    fun labelsAreBrowseTouchFullscreen() {
        assertEquals(listOf("浏览", "触控", "全屏"), DesktopMode.labels)
        assertEquals("浏览=只看不点", DesktopMode.BROWSE.hint)
        assertEquals("触控=点按映射鼠标", DesktopMode.TOUCH.hint)
        assertEquals("全屏=横屏沉浸", DesktopMode.FULLSCREEN.hint)
    }

    @Test
    fun parseAcceptsLabelOrName() {
        assertEquals(DesktopMode.TOUCH, DesktopMode.parse("触控"))
        assertEquals(DesktopMode.FULLSCREEN, DesktopMode.parse("fullscreen"))
        assertEquals(DesktopMode.BROWSE, DesktopMode.parse("unknown"))
    }

    @Test
    fun noDeprecatedModeNameInLabels() {
        val banned = "游" + "戏模式"
        DesktopMode.entries.forEach { mode ->
            assertFalse(mode.label.contains(banned))
            assertFalse(mode.hint.contains(banned))
        }
    }

    @Test
    fun securityDefaultsMatchPrototype() {
        val prefs = SecurityPrefs()
        assertFalse(prefs.biometricLock)
        assertEquals(30, prefs.timeoutMinutes)
        assertEquals("30分钟", prefs.timeoutLabel)
        assertEquals(RelayRegion.ASIA_SINGAPORE, prefs.relayRegion)
        assertTrue(TimeoutOptions.minutes.contains(30))
    }
}
