package com.pilight.android.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

val PurplePrimary = Color(0xFFB794F6)
val PurpleBright = Color(0xFF8B5CF6)
val PurpleDeep = Color(0xFF2A1648)
val Bg = Color(0xFF0B0714)
val Surface = Color(0xFF161022)
val SurfaceCard = Color(0xFF1C1333)
val Connected = Color(0xFF4ADE80)
val Muted = Color(0xFF9CA3AF)

private val DarkColors =
    darkColorScheme(
        primary = PurplePrimary,
        onPrimary = Color(0xFF1A0B2E),
        secondary = Muted,
        background = Bg,
        surface = Surface,
        surfaceVariant = SurfaceCard,
        onBackground = Color(0xFFF5F3FF),
        onSurface = Color(0xFFF5F3FF),
        onSurfaceVariant = Muted,
        outline = Color(0xFF3B2A63),
    )

@Composable
fun PiLightTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = DarkColors,
        content = content,
    )
}
