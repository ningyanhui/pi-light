package com.pilight.android.ui

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.pilight.android.ui.theme.PurpleBright
import com.pilight.android.ui.theme.PurplePrimary

@Composable
fun BrandMark(
    modifier: Modifier = Modifier,
    size: Dp = 40.dp,
    outlined: Boolean = false,
) {
    Box(
        modifier =
            modifier
                .size(size)
                .clip(CircleShape)
                .background(if (outlined) Color.Transparent else PurpleBright.copy(alpha = 0.18f)),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(modifier = Modifier.size(size)) {
            val stroke = (this.size.minDimension * 0.07f).coerceAtLeast(2f)
            if (outlined) {
                drawCircle(color = PurplePrimary, style = Stroke(width = stroke))
            } else {
                drawCircle(color = PurpleBright)
            }
            val w = this.size.minDimension
            val barTop = w * 0.30f
            val barH = w * 0.11f
            val stemW = w * 0.11f
            val color = if (outlined) PurplePrimary else Color.White
            drawRoundRect(
                color = color,
                topLeft = Offset(w * 0.26f, barTop),
                size = Size(w * 0.48f, barH),
                cornerRadius = CornerRadius(barH / 2f, barH / 2f),
            )
            drawRoundRect(
                color = color,
                topLeft = Offset(w * 0.33f, barTop),
                size = Size(stemW, w * 0.40f),
                cornerRadius = CornerRadius(stemW / 2f, stemW / 2f),
            )
            drawRoundRect(
                color = color,
                topLeft = Offset(w * 0.56f, barTop),
                size = Size(stemW, w * 0.40f),
                cornerRadius = CornerRadius(stemW / 2f, stemW / 2f),
            )
        }
    }
}

@Composable
fun QrImage(
    payload: String,
    modifier: Modifier = Modifier,
    fg: Color = Color(0xFF0A0C10),
    bg: Color = Color.White,
) {
    val bmp =
        remember(payload, fg, bg) {
            encodeQrBitmap(payload, 512, fg, bg)
        }
    Image(bitmap = bmp.asImageBitmap(), contentDescription = "配对二维码", modifier = modifier)
}

fun encodeQrBitmap(
    payload: String,
    size: Int,
    fg: Color,
    bg: Color,
): Bitmap {
    val matrix =
        QRCodeWriter().encode(
            payload,
            BarcodeFormat.QR_CODE,
            size,
            size,
            mapOf(EncodeHintType.MARGIN to 1, EncodeHintType.CHARACTER_SET to "UTF-8"),
        )
    val fgC = android.graphics.Color.argb(
        (fg.alpha * 255).toInt(),
        (fg.red * 255).toInt(),
        (fg.green * 255).toInt(),
        (fg.blue * 255).toInt(),
    )
    val bgC = android.graphics.Color.argb(
        (bg.alpha * 255).toInt(),
        (bg.red * 255).toInt(),
        (bg.green * 255).toInt(),
        (bg.blue * 255).toInt(),
    )
    val pixels = IntArray(size * size)
    for (y in 0 until size) {
        for (x in 0 until size) {
            pixels[y * size + x] = if (matrix.get(x, y)) fgC else bgC
        }
    }
    return Bitmap.createBitmap(pixels, size, size, Bitmap.Config.ARGB_8888)
}
