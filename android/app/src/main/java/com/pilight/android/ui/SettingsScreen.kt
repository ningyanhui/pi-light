package com.pilight.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Fingerprint
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Language
import androidx.compose.material.icons.outlined.Schedule
import androidx.compose.material.icons.outlined.Shield
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.pilight.android.BuildConfig
import com.pilight.android.CompanionViewModel
import com.pilight.android.data.TimeoutOptions
import com.pilight.android.ui.theme.PurpleBright
import com.pilight.android.ui.theme.PurplePrimary

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    vm: CompanionViewModel,
    onPair: () -> Unit,
) {
    val home by vm.home.collectAsStateWithLifecycle()
    val security by vm.security.collectAsStateWithLifecycle()
    var pickTimeout by remember { mutableStateOf(false) }

    Column(modifier = Modifier.fillMaxSize()) {
        TopAppBar(
            title = {
                Text("安全设置", fontWeight = FontWeight.SemiBold, modifier = Modifier.fillMaxWidth())
            },
            colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background),
        )
        Column(
            modifier =
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 8.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            SectionLabel("设备安全")
            ListItem(
                leadingContent = { Icon(Icons.Outlined.Shield, contentDescription = null, tint = PurplePrimary) },
                headlineContent = { Text("撤销设备") },
                supportingContent = {
                    Text("断开此设备与帐户的关联，终止所有远程访问权限。")
                },
                trailingContent = {
                    if (home.session == null) {
                        TextButton(onClick = onPair) { Text("去配对") }
                    } else {
                        OutlinedButton(onClick = { vm.unpair() }) { Text("撤销设备") }
                    }
                },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )

            SectionLabel("远程桌面安全")
            ListItem(
                leadingContent = { Icon(Icons.Outlined.Fingerprint, contentDescription = null, tint = PurplePrimary) },
                headlineContent = { Text("远程桌面需生物识别解锁") },
                supportingContent = { Text("在建立远程桌面连接前，必须完成生物识别验证。") },
                trailingContent = {
                    Switch(
                        checked = security.biometricLock,
                        onCheckedChange = { on -> vm.updateSecurity { it.copy(biometricLock = on) } },
                        colors = SwitchDefaults.colors(checkedTrackColor = PurpleBright),
                    )
                },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )
            ListItem(
                leadingContent = { Icon(Icons.Outlined.Schedule, contentDescription = null, tint = PurplePrimary) },
                headlineContent = { Text("会话超时") },
                supportingContent = { Text("在没有活动的情况下，自动断开远程桌面会话。") },
                trailingContent = {
                    TextButton(onClick = { pickTimeout = true }) { Text(security.timeoutLabel) }
                },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )
            ListItem(
                leadingContent = { Icon(Icons.Outlined.Language, contentDescription = null, tint = PurplePrimary) },
                headlineContent = { Text("连接方式") },
                supportingContent = { Text("使用桌面端提供的公网中继地址。中继区域由桌面端和服务商决定，手机端暂不单独切换路由。") },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )
            ListItem(
                leadingContent = { Icon(Icons.Outlined.Info, contentDescription = null, tint = PurplePrimary) },
                headlineContent = { Text("连接依赖") },
                supportingContent = { Text("手机通过桌面端提供的公网中继连接；桌面端需要保持运行并在线。") },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )
            ListItem(
                leadingContent = { Icon(Icons.Outlined.Shield, contentDescription = null, tint = PurplePrimary) },
                headlineContent = { Text("提示") },
                supportingContent = { Text("这些设置仅影响远程桌面连接和行为，不影响设备本地的安全。") },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )

            HorizontalDivider(modifier = Modifier.padding(vertical = 8.dp))
            SectionLabel("关于")
            ListItem(
                headlineContent = { Text("应用版本") },
                supportingContent = { Text("${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})") },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )
            ListItem(
                headlineContent = { Text("当前桌面") },
                supportingContent = {
                    Text(home.session?.let { "${it.hostName} · ${it.relayUrl}" } ?: "未配对")
                },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )
            Text(
                "本客户端不内嵌 Pi。配对成功后经 Cloudflare Tunnel 中继访问家里的桌面会话。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            )
            Text(
                "诚实限制：远程依赖公网中继；临时 trycloudflare 主机名每次启动都会变。远程桌面是 JPEG 轮询（约 5–8 fps），浏览只看不点；触控 / 全屏映射键鼠。需要桌面侧屏幕捕获权限，不是 WebRTC。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
            )
        }
    }

    if (pickTimeout) {
        AlertDialog(
            onDismissRequest = { pickTimeout = false },
            title = { Text("会话超时") },
            text = {
                Column {
                    TimeoutOptions.minutes.forEach { m ->
                        TextButton(
                            onClick = {
                                vm.updateSecurity { it.copy(timeoutMinutes = m) }
                                pickTimeout = false
                            },
                        ) { Text("${m}分钟") }
                    }
                }
            },
            confirmButton = { TextButton(onClick = { pickTimeout = false }) { Text("关闭") } },
        )
    }
}

@Composable
private fun SectionLabel(text: String) {
    Row(modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
        Text(text, color = PurplePrimary, fontWeight = FontWeight.Medium)
        Spacer(Modifier.size(1.dp))
    }
}
