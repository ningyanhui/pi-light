package com.pilight.android.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// Legacy token names remain for source compatibility. Values match the shared
// indigo palette in docs/ui-system.md (desktop theme.rs accent / surfaces).
val PurplePrimary = Color(0xFF7C8CFF)
val PurpleBright = Color(0xFF9AA6FF)
val PurpleDeep = Color(0xFF20263E)
val Bg = Color(0xFF0A0C10)
val Surface = Color(0xFF11151B)
val SurfaceCard = Color(0xFF161B23)
val SurfaceElevated = Color(0xFF1B212B)
val Connected = Color(0xFF34D399)
val Muted = Color(0xFFA3ADBD)
val AccentBlue = Color(0xFF7C8CFF)

private val DarkColors =
    darkColorScheme(
        primary = PurplePrimary,
        onPrimary = Color(0xFF0A0C10),
        primaryContainer = PurpleDeep,
        onPrimaryContainer = Color(0xFFC7CEFF),
        secondary = Muted,
        tertiary = Color(0xFF22D3EE),
        background = Bg,
        surface = Surface,
        surfaceVariant = SurfaceCard,
        onBackground = Color(0xFFE8EDF5),
        onSurface = Color(0xFFE8EDF5),
        onSurfaceVariant = Muted,
        outline = Color(0xFF232A35),
        error = Color(0xFFF87171),
    )

private val AppTypography =
    Typography(
        headlineLarge = TextStyle(fontSize = 30.sp, lineHeight = 36.sp, fontWeight = FontWeight.SemiBold),
        headlineMedium = TextStyle(fontSize = 26.sp, lineHeight = 32.sp, fontWeight = FontWeight.SemiBold),
        titleLarge = TextStyle(fontSize = 20.sp, lineHeight = 26.sp, fontWeight = FontWeight.SemiBold),
        titleMedium = TextStyle(fontSize = 16.sp, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold),
        bodyLarge = TextStyle(fontSize = 15.sp, lineHeight = 22.sp),
        bodyMedium = TextStyle(fontSize = 14.sp, lineHeight = 20.sp),
        bodySmall = TextStyle(fontSize = 12.sp, lineHeight = 17.sp),
        labelLarge = TextStyle(fontSize = 13.sp, lineHeight = 18.sp, fontWeight = FontWeight.Medium),
    )

private val AppShapes =
    Shapes(
        extraSmall = RoundedCornerShape(6.dp),
        small = RoundedCornerShape(8.dp),
        medium = RoundedCornerShape(12.dp),
        large = RoundedCornerShape(16.dp),
        extraLarge = RoundedCornerShape(20.dp),
    )

@Composable
fun PiLightTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = DarkColors,
        typography = AppTypography,
        shapes = AppShapes,
        content = content,
    )
}
