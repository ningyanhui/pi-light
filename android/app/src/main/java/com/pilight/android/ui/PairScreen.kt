package com.pilight.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.QrCodeScanner
import androidx.compose.material.icons.outlined.Schedule
import androidx.compose.material.icons.outlined.Shield
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
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
import com.pilight.android.ui.theme.PurpleBright
import com.pilight.android.ui.theme.PurpleDeep
import com.pilight.android.ui.theme.PurplePrimary
import com.pilight.android.ui.theme.SurfaceCard

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

    Column(modifier = Modifier.fillMaxSize()) {
        TopAppBar(
            title = { Text("配对桌面") },
            navigationIcon = {
                IconButton(onClick = onBack) {
                    Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = "返回")
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
                    .padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(
                    modifier =
                        Modifier
                            .size(52.dp)
                            .clip(CircleShape)
                            .background(PurpleDeep),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text("π", color = PurplePrimary, fontWeight = FontWeight.Bold, fontSize = 22.sp)
                }
                Spacer(Modifier.size(12.dp))
                Column {
                    Text("pi-light", fontWeight = FontWeight.SemiBold, fontSize = 20.sp)
                    Text("companion", color = MaterialTheme.colorScheme.secondary)
                }
            }

            Text("扫码配对", fontWeight = FontWeight.SemiBold, color = PurplePrimary)
            Text(
                "使用桌面版 pi-light「远程访问」页上的二维码。二维码里带有配对码和公网中继地址。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary,
            )
            OutlinedButton(onClick = onScan, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Outlined.QrCodeScanner, contentDescription = null)
                Spacer(Modifier.size(8.dp))
                Text("扫描二维码")
            }

            Text("或输入配对码", fontWeight = FontWeight.SemiBold)
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
                "手输配对必须同时填写桌面显示的公网中继 URL。临时隧道主机名每次启动都会变；同一 Wi‑Fi 不能代替公网中继。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary,
            )
            Button(
                onClick = { vm.pairFromTyped(code, relay) },
                enabled = !home.pairing,
                modifier = Modifier.fillMaxWidth(),
                colors = ButtonDefaults.buttonColors(containerColor = PurpleBright),
            ) {
                Text(if (home.pairing) "正在提交…" else "提交配对")
            }

            if (home.pairing || home.lastCode != null) {
                Column(
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .background(SurfaceCard, androidx.compose.foundation.shape.RoundedCornerShape(16.dp))
                            .padding(16.dp),
                ) {
                    Text("配对码", color = MaterialTheme.colorScheme.secondary)
                    Text(
                        home.lastCode ?: "—",
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.Bold,
                        fontSize = 32.sp,
                    )
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Outlined.Shield, contentDescription = null, tint = PurplePrimary, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.size(6.dp))
                        Text("经加密验证 · 安全配对", style = MaterialTheme.typography.bodySmall, color = PurplePrimary)
                    }
                    Spacer(Modifier.height(8.dp))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Outlined.Schedule, contentDescription = null, tint = PurplePrimary, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.size(6.dp))
                        Column {
                            Text("等待桌面确认", fontWeight = FontWeight.Medium, color = PurplePrimary)
                            Text(
                                if (home.pairing) "配对码已提交，正在经中继 claim…" else "桌面校验配对码后会自动授权。",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.secondary,
                            )
                        }
                    }
                }
            }

            home.error?.let {
                Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }

            Text("注意", fontWeight = FontWeight.SemiBold)
            Text("• 无需同一局域网")
            Text("• 经加密中继连接")
            Text("• 默认可写桌面当前 Pi 会话")
        }
    }
}
