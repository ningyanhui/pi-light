package com.pilight.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.pilight.android.BuildConfig

@Composable
fun SettingsScreen() {
    Column(
        modifier =
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            text = "设置",
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.SemiBold,
        )
        Text(
            text = "占位页：后续里程碑再接入 Host 或同构协议。",
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
            supportingContent = { Text("debug · shell / placeholder") },
        )
        ListItem(
            headlineContent = { Text("Pi sidecar") },
            supportingContent = { Text("未捆绑 · 本客户端不启动 Pi") },
        )
        ListItem(
            headlineContent = { Text("Host 配对") },
            supportingContent = { Text("未实现（Option C 不需要）") },
        )
        ListItem(
            headlineContent = { Text("桌面客户端") },
            supportingContent = { Text("Windows · desktop-rust · egui + 内嵌 Pi") },
        )

        HorizontalDivider()

        Text(
            text =
                "M6′ 维持 Option C：桌面 Host 未提供可配对的共享面，本页不接入 Host。" +
                    "完整会话请用 Windows desktop-rust 客户端。",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.secondary,
        )
    }
}
