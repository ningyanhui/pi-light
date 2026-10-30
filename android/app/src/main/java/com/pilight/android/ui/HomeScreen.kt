package com.pilight.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Bolt
import androidx.compose.material.icons.outlined.Computer
import androidx.compose.material.icons.outlined.DesktopWindows
import androidx.compose.material.icons.outlined.Hub
import androidx.compose.material.icons.outlined.Shield
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.pilight.android.CompanionViewModel
import com.pilight.android.ui.theme.Connected
import com.pilight.android.ui.theme.PurpleBright
import com.pilight.android.ui.theme.PurpleDeep
import com.pilight.android.ui.theme.PurplePrimary
import com.pilight.android.ui.theme.SurfaceCard

@Composable
fun HomeScreen(
    vm: CompanionViewModel,
    onPair: () -> Unit,
    onChat: () -> Unit,
    onDesktop: () -> Unit,
) {
    val home by vm.home.collectAsStateWithLifecycle()
    Column(
        modifier =
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                modifier =
                    Modifier
                        .size(40.dp)
                        .clip(RoundedCornerShape(12.dp))
                        .background(PurpleDeep),
                contentAlignment = Alignment.Center,
            ) {
                Text("π", color = PurplePrimary, fontWeight = FontWeight.Bold, fontSize = 20.sp)
            }
            Spacer(Modifier.size(10.dp))
            Column {
                Text("π-light", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
                Text("Android companion", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.secondary)
            }
        }

        if (home.session == null) {
            EmptyDeviceCard(onPair = onPair, error = home.error)
        } else {
            DeviceCard(
                host = home.hostName.ifBlank { home.session?.hostName.orEmpty().ifBlank { "desktop" } },
                connected = home.connected,
                latencyMs = home.latencyMs,
                onChat = onChat,
                onDesktop = onDesktop,
            )
            home.error?.let { err ->
                Text(err, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }
            ShieldNote()
        }
    }
}

@Composable
private fun EmptyDeviceCard(
    onPair: () -> Unit,
    error: String?,
) {
    Column(
        modifier =
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(20.dp))
                .background(SurfaceCard)
                .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text("尚未配对桌面", fontWeight = FontWeight.SemiBold, fontSize = 18.sp)
        Text(
            "用手机扫描桌面「远程访问」页的二维码，或输入 6 位配对码和公网中继地址。不需要同一局域网。",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.secondary,
        )
        error?.let {
            Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
        }
        Button(
            onClick = onPair,
            modifier = Modifier.fillMaxWidth(),
            colors = ButtonDefaults.buttonColors(containerColor = PurpleBright),
        ) {
            Text("配对桌面")
        }
    }
}

@Composable
private fun DeviceCard(
    host: String,
    connected: Boolean,
    latencyMs: Long?,
    onChat: () -> Unit,
    onDesktop: () -> Unit,
) {
    Column(
        modifier =
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(22.dp))
                .background(PurpleDeep)
                .padding(18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Outlined.Computer, contentDescription = null, tint = PurplePrimary, modifier = Modifier.size(36.dp))
            Spacer(Modifier.size(10.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text("我的电脑", fontWeight = FontWeight.SemiBold, fontSize = 18.sp)
                Text(host, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.secondary)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        if (connected) "●  已连接" else "○  连接中",
                        color = if (connected) Connected else MaterialTheme.colorScheme.secondary,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Outlined.Hub, contentDescription = null, tint = PurplePrimary, modifier = Modifier.size(16.dp))
            Spacer(Modifier.size(6.dp))
            val latency = latencyMs?.let { "latency ${it}ms" } ?: "latency —"
            Text("via 中继  ·  $latency", style = MaterialTheme.typography.bodySmall, color = PurplePrimary)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            Button(
                onClick = onChat,
                modifier = Modifier.weight(1f),
                colors = ButtonDefaults.buttonColors(containerColor = PurpleBright),
            ) {
                Icon(Icons.Outlined.Bolt, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(Modifier.size(6.dp))
                Text("进入会话")
            }
            OutlinedButton(onClick = onDesktop, modifier = Modifier.weight(1f)) {
                Icon(Icons.Outlined.DesktopWindows, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(Modifier.size(6.dp))
                Text("远程桌面")
            }
        }
    }
}

@Composable
fun ShieldNote() {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(16.dp))
                .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(16.dp))
                .padding(14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Outlined.Shield, contentDescription = null, tint = PurplePrimary)
        Spacer(Modifier.size(10.dp))
        Column {
            Text("已通过中继服务器连接", fontWeight = FontWeight.Medium)
            Text("所有数据已加密传输", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.secondary)
        }
    }
}
