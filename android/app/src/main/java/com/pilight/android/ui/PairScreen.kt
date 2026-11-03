package com.pilight.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.MoreVert
import androidx.compose.material.icons.outlined.QrCode2
import androidx.compose.material.icons.outlined.Schedule
import androidx.compose.material.icons.outlined.Shield
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.pilight.android.CompanionViewModel
import com.pilight.android.data.PairCodes
import com.pilight.android.data.normalizeRelayUrl
import com.pilight.android.ui.theme.PurpleBright
import com.pilight.android.ui.theme.PurplePrimary
import com.pilight.android.ui.theme.SurfaceCard
import org.json.JSONObject

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PairScreen(
    vm: CompanionViewModel,
    onBack: () -> Unit,
    onScan: () -> Unit,
    onPaired: () -> Unit,
) {
    val home by vm.home.collectAsStateWithLifecycle()
    var code by rememberSaveable { mutableStateOf("") }
    var relay by rememberSaveable { mutableStateOf("") }

    LaunchedEffect(home.session?.deviceId) {
        if (home.session != null && !home.pairing) {
            onPaired()
        }
    }

    val previewJson = rememberPairPreview(code, relay, home.lastCode)
    val displayCode = home.lastCode ?: PairCodes.display(code).ifBlank { "—— ——" }

    Column(modifier = Modifier.fillMaxSize()) {
        TopAppBar(
            title = {
                Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                    Text("配对桌面", fontWeight = FontWeight.SemiBold)
                }
            },
            navigationIcon = {
                IconButton(onClick = onBack) {
                    Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = "返回")
                }
            },
            actions = {
                IconButton(onClick = { }) {
                    Icon(Icons.Outlined.MoreVert, contentDescription = "更多")
                }
            },
            colors =
                TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                ),
        )
        Column(
            modifier =
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 24.dp, vertical = 8.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            BrandMark(size = 72.dp, outlined = true)
            Text("pi-light", fontWeight = FontWeight.SemiBold, fontSize = 28.sp)
            Text("companion", color = MaterialTheme.colorScheme.secondary)

            Spacer(Modifier.height(4.dp))
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                Box(
                    modifier =
                        Modifier
                            .size(8.dp)
                            .clip(androidx.compose.foundation.shape.CircleShape)
                            .background(PurplePrimary),
                )
                Spacer(Modifier.size(8.dp))
                Column {
                    Text("扫码配对", fontWeight = FontWeight.SemiBold, color = PurplePrimary)
                    Text(
                        "使用桌面版 pi-light 扫描二维码，或点下方框扫描桌面码",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.secondary,
                    )
                }
            }

            Box(
                modifier =
                    Modifier
                        .size(220.dp)
                        .clip(RoundedCornerShape(20.dp))
                        .background(androidx.compose.ui.graphics.Color.White)
                        .clickable(onClick = onScan),
                contentAlignment = Alignment.Center,
            ) {
                if (previewJson != null) {
                    QrImage(payload = previewJson, modifier = Modifier.fillMaxSize().padding(16.dp))
                } else {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Icon(
                            Icons.Outlined.QrCode2,
                            contentDescription = null,
                            tint = PurpleBright,
                            modifier = Modifier.size(72.dp),
                        )
                        Text("点此扫描桌面二维码", color = PurpleBright, fontWeight = FontWeight.Medium)
                    }
                }
            }

            Text("配对码", color = MaterialTheme.colorScheme.secondary)
            Text(
                displayCode,
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Bold,
                fontSize = 42.sp,
                letterSpacing = 2.sp,
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.Shield, contentDescription = null, tint = PurplePrimary, modifier = Modifier.size(16.dp))
                Spacer(Modifier.size(6.dp))
                Text("经加密验证 · 安全配对", style = MaterialTheme.typography.bodySmall, color = PurplePrimary)
            }

            Row(
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(16.dp))
                        .background(SurfaceCard)
                        .padding(14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.Outlined.Schedule, contentDescription = null, tint = PurplePrimary)
                Spacer(Modifier.size(10.dp))
                Column {
                    Text(
                        if (home.pairing) "等待桌面确认" else "等待桌面确认",
                        fontWeight = FontWeight.Medium,
                    )
                    Text(
                        if (home.pairing) {
                            "配对码已提交，正在经中继 claim…"
                        } else {
                            "配对码已显示，等待确认中…"
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.secondary,
                    )
                }
            }

            Text("或手输配对码与中继", fontWeight = FontWeight.SemiBold, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(
                value = code,
                onValueChange = { code = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("6 位配对码") },
                placeholder = { Text("746 382") },
                singleLine = true,
            )
            OutlinedTextField(
                value = relay,
                onValueChange = { relay = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("中继地址") },
                placeholder = { Text("https://….trycloudflare.com") },
                singleLine = true,
            )
            Text(
                "手输必须同时填写桌面显示的公网中继 URL。临时隧道主机名每次启动都会变。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary,
                modifier = Modifier.fillMaxWidth(),
            )
            Button(
                onClick = { vm.pairFromTyped(code, relay) },
                enabled = !home.pairing,
                modifier = Modifier.fillMaxWidth(),
                colors = ButtonDefaults.buttonColors(containerColor = PurpleBright),
                shape = RoundedCornerShape(14.dp),
            ) {
                Text(if (home.pairing) "正在提交…" else "提交配对")
            }

            home.error?.let {
                Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }

            Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text("注意", fontWeight = FontWeight.SemiBold)
                Text("• 无需同一局域网")
                Text("• 经加密中继连接")
                Text("• 默认可写桌面当前 Pi 会话")
            }
            Spacer(Modifier.height(12.dp))
        }
    }
}

@Composable
private fun rememberPairPreview(code: String, relay: String, lastCode: String?): String? {
    return remember(code, relay, lastCode) {
        val normalized = PairCodes.normalize(code.ifBlank { lastCode.orEmpty() })
        val url = normalizeRelayUrl(relay)
        if (!PairCodes.isPlausible(normalized) || url.isBlank()) {
            null
        } else {
            JSONObject()
                .put("v", 1)
                .put("app", "pi-light")
                .put("code", normalized)
                .put("claim", "/v1/pair/claim")
                .put("chat", "/v1/chat")
                .put("protocol", "pi-light-companion/1")
                .put("relay", url)
                .toString()
        }
    }
}
