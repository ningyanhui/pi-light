package com.pilight.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.pilight.android.BuildConfig
import com.pilight.android.CompanionViewModel

@Composable
fun SettingsScreen(
    vm: CompanionViewModel,
    onPair: () -> Unit,
) {
    val home by vm.home.collectAsStateWithLifecycle()
    Column(
        modifier =
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("设置", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
        Text(
            "本客户端不内嵌 Pi。配对成功后经 Cloudflare Tunnel 中继访问家里的桌面会话。",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.secondary,
        )

        HorizontalDivider()

        ListItem(
            headlineContent = { Text("应用版本") },
            supportingContent = { Text("${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})") },
        )
        ListItem(
            headlineContent = { Text("构建类型") },
            supportingContent = { Text("${BuildConfig.BUILD_TYPE} · companion M6-lite") },
        )
        ListItem(
            headlineContent = { Text("Pi sidecar") },
            supportingContent = { Text("未捆绑 · 由 Windows desktop-rust 托管") },
        )
        ListItem(
            headlineContent = { Text("当前桌面") },
            supportingContent = {
                Text(
                    home.session?.let { "${it.hostName} · ${it.relayUrl}" } ?: "未配对",
                )
            },
        )
        ListItem(
            headlineContent = { Text("相机") },
            supportingContent = { Text("仅用于扫描桌面配对二维码；也可手输配对码") },
        )

        if (home.session == null) {
            TextButton(onClick = onPair) { Text("去配对") }
        } else {
            OutlinedButton(
                onClick = { vm.unpair() },
                modifier = Modifier.fillMaxWidth(),
                colors = ButtonDefaults.outlinedButtonColors(contentColor = MaterialTheme.colorScheme.error),
            ) {
                Text("解除本机配对")
            }
            Text(
                "这只清除手机上的 token。若要从桌面撤销授权，请到设置 → 远程访问 → 已授权设备。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary,
            )
        }

        HorizontalDivider()
        Text("诚实限制", fontWeight = FontWeight.SemiBold)
        Text("• 远程依赖公网中继；临时 trycloudflare 主机名每次启动都会变。")
        Text("• 同一局域网并不能代替公网验收。演示请用蜂窝网手机 + 家宽桌面。")
        Text("• 远程桌面三档（浏览 / 触控 / 全屏）尚未开放。")
        Text("• 默认可写桌面当前 Pi 会话，不是只读观察。")
    }
}
