package com.pilight.android.ui

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.AttachFile
import androidx.compose.material.icons.outlined.ChatBubbleOutline
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.HourglassTop
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.Menu
import androidx.compose.material.icons.outlined.Mic
import androidx.compose.material.icons.outlined.MicOff
import androidx.compose.material.icons.outlined.SmartToy
import androidx.compose.material.icons.outlined.Stop
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.pilight.android.CompanionViewModel
import com.pilight.android.data.ChatLine
import com.pilight.android.data.SpeechInputListener
import com.pilight.android.data.SystemSpeechInput
import com.pilight.android.data.ToolCard
import com.pilight.android.ui.theme.Connected
import com.pilight.android.ui.theme.PurpleBright
import com.pilight.android.ui.theme.PurpleDeep
import com.pilight.android.ui.theme.PurplePrimary
import com.pilight.android.ui.theme.SurfaceCard
import com.pilight.android.ui.theme.SurfaceElevated

@Composable
fun ChatScreen(
    vm: CompanionViewModel,
    onNeedPair: () -> Unit,
    onOpenDesktop: () -> Unit,
) {
    val home by vm.home.collectAsStateWithLifecycle()
    val chat by vm.chat.collectAsStateWithLifecycle()
    val context = LocalContext.current
    var draft by rememberSaveable { mutableStateOf("") }
    var notice by rememberSaveable { mutableStateOf<String?>(null) }
    var listening by remember { mutableStateOf(false) }
    val listState = rememberLazyListState()
    val speech =
        remember {
            SystemSpeechInput(
                context,
                object : SpeechInputListener {
                    override fun onPartial(text: String) {
                        draft = text
                    }

                    override fun onFinal(text: String) {
                        draft = ""
                        notice = null
                        vm.send(text)
                    }

                    override fun onError(message: String) {
                        notice = message
                    }

                    override fun onUnavailable(message: String) {
                        notice = message
                    }

                    override fun onEnded() {
                        listening = false
                    }
                },
            )
        }
    DisposableEffect(speech) {
        onDispose { speech.destroy() }
    }
    val micPermission =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
            if (ok) {
                listening = true
                notice = "正在听…"
                speech.start()
            } else {
                notice = "需要麦克风权限才能语音输入。可继续打字发送。"
            }
        }

    if (home.session == null) {
        Column(
            modifier = Modifier.fillMaxSize().padding(24.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text("还没有配对的桌面", fontWeight = FontWeight.SemiBold)
            Text(
                "先完成扫码或输码配对，才能把消息写入桌面当前 Pi 会话。",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.secondary,
            )
            TextButton(onClick = onNeedPair) { Text("去配对") }
        }
        return
    }

    LaunchedEffect(chat.messages.size, chat.tools.size, chat.partial, chat.streaming, chat.sending) {
        val extra = if (chat.streaming || chat.partial.isNotBlank() || chat.sending || chat.tools.isNotEmpty()) 1 else 0
        val count = chat.messages.size + chat.tools.size + extra
        if (count > 0) {
            listState.animateScrollToItem(count - 1)
        }
    }

    Column(modifier = Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BrandMark(size = 32.dp)
            Spacer(Modifier.size(8.dp))
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("pi-light", fontWeight = FontWeight.SemiBold, fontSize = 18.sp)
                    Spacer(Modifier.size(6.dp))
                    Box(
                        modifier =
                            Modifier
                                .size(7.dp)
                                .clip(CircleShape)
                                .background(if (home.connected) Connected else MaterialTheme.colorScheme.secondary),
                    )
                }
                Text(
                    "Android 聊天 · 远程桌面助手",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.secondary,
                )
            }
            TextButton(onClick = onOpenDesktop) {
                Column(horizontalAlignment = Alignment.End) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Box(
                            modifier =
                                Modifier
                                    .size(8.dp)
                                    .clip(CircleShape)
                                    .background(if (home.connected) Connected else MaterialTheme.colorScheme.secondary),
                        )
                        Spacer(Modifier.size(6.dp))
                        Text(if (home.connected) "已连接" else "未连接", color = MaterialTheme.colorScheme.onSurface)
                    }
                    Text(
                        home.hostName.ifBlank { home.session?.hostName.orEmpty() },
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.secondary,
                    )
                }
            }
        }

        Row(
            modifier =
                Modifier
                    .padding(horizontal = 16.dp, vertical = 8.dp)
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(SurfaceCard)
                    .padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text("当前桌面会话", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.secondary)
                Text(
                    chat.title.ifBlank { "当前 Pi 会话" },
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Column(horizontalAlignment = Alignment.End) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(
                        modifier = Modifier.size(7.dp).clip(CircleShape)
                            .background(if (home.connected) Connected else MaterialTheme.colorScheme.secondary),
                    )
                    Spacer(Modifier.size(6.dp))
                    Text(
                        if (home.connected) "在线" else "连接中",
                        style = MaterialTheme.typography.labelLarge,
                        color = if (home.connected) Connected else MaterialTheme.colorScheme.secondary,
                    )
                }
                Text(
                    home.latencyMs?.let { "${it} ms" } ?: "延迟 —",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.secondary,
                )
            }
        }

        LazyColumn(
            state = listState,
            modifier = Modifier.weight(1f).fillMaxWidth(),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            itemsIndexed(chat.messages, key = { idx, line -> "$idx:${line.role}:${line.text.take(48)}" }) { _, line ->
                MessageBlock(line)
            }
            items(chat.tools, key = { it.id }) { card ->
                SessionToolCard(card)
            }
            if (chat.streaming || chat.partial.isNotBlank() || chat.sending) {
                item {
                    if (chat.partial.isBlank()) {
                        Text("思考中…", color = MaterialTheme.colorScheme.secondary)
                    } else {
                        MessageBlock(ChatLine(role = "assistant", text = chat.partial), thinking = true)
                    }
                }
            }
        }

        notice?.let {
            Text(
                it,
                color = PurplePrimary,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(horizontal = 16.dp),
            )
        }
        chat.error?.let {
            Text(
                it,
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(horizontal = 16.dp),
            )
        }

        Row(
            modifier =
                Modifier
                    .padding(horizontal = 12.dp)
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(SurfaceCard)
                    .padding(horizontal = 6.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(
                onClick = {
                    if (listening) {
                        speech.cancel()
                        notice = null
                    } else if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) ==
                        PackageManager.PERMISSION_GRANTED
                    ) {
                        listening = true
                        notice = "正在听…"
                        speech.start()
                    } else {
                        micPermission.launch(Manifest.permission.RECORD_AUDIO)
                    }
                },
                enabled = chat.writable && !chat.sending,
            ) {
                Icon(
                    if (listening) Icons.Outlined.MicOff else Icons.Outlined.Mic,
                    contentDescription = if (listening) "停止语音" else "语音输入",
                    tint = if (listening) PurpleBright else PurplePrimary,
                )
            }
            TextField(
                value = draft,
                onValueChange = { draft = it },
                modifier = Modifier.weight(1f),
                placeholder = {
                    Text(if (listening) "正在听…" else "输入消息或使用 / 唤起命令…")
                },
                enabled = !chat.sending && chat.writable,
                colors =
                    TextFieldDefaults.colors(
                        focusedContainerColor = Color.Transparent,
                        unfocusedContainerColor = Color.Transparent,
                        disabledContainerColor = Color.Transparent,
                        focusedIndicatorColor = Color.Transparent,
                        unfocusedIndicatorColor = Color.Transparent,
                    ),
            )
            val busy = chat.sending || chat.streaming
            if (busy) {
                FilledIconButton(
                    onClick = { vm.stop() },
                    enabled = !chat.stopping && (chat.canStop || chat.streaming),
                    colors = IconButtonDefaults.filledIconButtonColors(containerColor = PurpleBright),
                ) {
                    Icon(Icons.Outlined.Stop, contentDescription = "停止")
                }
            } else {
                FilledIconButton(
                    onClick = {
                        val text = draft
                        draft = ""
                        notice = null
                        vm.send(text)
                    },
                    enabled = draft.isNotBlank() && chat.writable,
                    colors = IconButtonDefaults.filledIconButtonColors(containerColor = PurpleBright),
                ) {
                    Icon(Icons.AutoMirrored.Outlined.Send, contentDescription = "发送")
                }
            }
        }

        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 6.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                modifier =
                    Modifier
                        .size(8.dp)
                        .clip(CircleShape)
                        .background(if (home.connected) Connected else MaterialTheme.colorScheme.secondary),
            )
            Spacer(Modifier.size(6.dp))
            Text(
                "远程桌面已连接 · ${home.hostName.ifBlank { home.session?.hostName }}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary,
                modifier = Modifier.weight(1f),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val latency = home.latencyMs?.let { "延迟 ${it}ms" } ?: "延迟 —"
            Text(latency, style = MaterialTheme.typography.bodySmall, color = PurplePrimary)
            Spacer(Modifier.size(8.dp))
            Icon(Icons.Outlined.Lock, contentDescription = null, modifier = Modifier.size(14.dp), tint = MaterialTheme.colorScheme.secondary)
        }
    }
}

@Composable
private fun SessionRow(
    title: String,
    preview: String,
    selected: Boolean,
    connected: Boolean,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(10.dp))
                .background(if (selected) PurpleDeep else Color.Transparent)
                .padding(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Outlined.ChatBubbleOutline, contentDescription = null, tint = PurplePrimary)
        Spacer(Modifier.size(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(title, fontWeight = FontWeight.SemiBold)
            Text(
                preview,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Box(
            modifier =
                Modifier
                    .size(8.dp)
                    .clip(CircleShape)
                    .background(if (connected) Connected else PurplePrimary),
        )
    }
}

@Composable
private fun MessageBlock(
    line: ChatLine,
    thinking: Boolean = false,
) {
    val mine = line.role == "user"
    if (looksLikeJson(line.text) && !mine) {
        ToolCard(
            title = "工具输出",
            body = line.text,
            statusLabel = "已完成",
            statusTint = Connected,
            statusIcon = Icons.Outlined.CheckCircle,
        )
        return
    }
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = if (mine) Arrangement.End else Arrangement.Start,
        verticalAlignment = Alignment.Top,
    ) {
        if (!mine) {
            Box(
                modifier =
                    Modifier
                        .padding(end = 8.dp)
                        .size(32.dp)
                        .clip(CircleShape)
                        .background(PurpleDeep),
                contentAlignment = Alignment.Center,
            ) {
                Icon(Icons.Outlined.SmartToy, contentDescription = null, tint = PurplePrimary, modifier = Modifier.size(18.dp))
            }
        }
        Box(
            modifier =
                Modifier
                    .fillMaxWidth(if (mine) 0.86f else 0.9f)
                    .clip(RoundedCornerShape(16.dp))
                    .background(if (mine) PurpleBright.copy(alpha = 0.22f) else SurfaceCard)
                    .padding(14.dp),
        ) {
            Text(
                line.text,
                color =
                    if (thinking) {
                        MaterialTheme.colorScheme.secondary
                    } else {
                        MaterialTheme.colorScheme.onSurface
                    },
            )
        }
    }
}

@Composable
private fun SessionToolCard(card: ToolCard) {
    val running = card.status == "running"
    val failed = card.status == "error"
    val label =
        when {
            running -> "进行中"
            failed -> "失败"
            else -> "已完成"
        }
    val tint =
        when {
            failed -> MaterialTheme.colorScheme.error
            running -> MaterialTheme.colorScheme.secondary
            else -> Connected
        }
    val icon =
        when {
            failed -> Icons.Outlined.ErrorOutline
            running -> Icons.Outlined.HourglassTop
            else -> Icons.Outlined.CheckCircle
        }
    val duration = card.durationMs?.let { " · ${it}ms" }.orEmpty()
    ToolCard(
        title = card.name.ifBlank { "tool" },
        body = (card.args ?: "").ifBlank { "id=${card.id}$duration" },
        statusLabel = label + duration,
        statusTint = tint,
        statusIcon = icon,
    )
}

@Composable
private fun ToolCard(
    title: String,
    body: String,
    statusLabel: String,
    statusTint: Color,
    statusIcon: androidx.compose.ui.graphics.vector.ImageVector,
) {
    Column(
        modifier =
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(16.dp))
                .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(16.dp))
                .background(SurfaceCard)
                .padding(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("</>", color = PurplePrimary, fontWeight = FontWeight.Bold)
            Spacer(Modifier.size(8.dp))
            Text(title, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f))
            Icon(
                statusIcon,
                contentDescription = null,
                tint = statusTint,
                modifier = Modifier.size(16.dp),
            )
            Spacer(Modifier.size(4.dp))
            Text(statusLabel, color = statusTint, fontSize = 12.sp)
        }
        if (body.isNotBlank()) {
            Spacer(Modifier.height(8.dp))
            Text(body, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
        }
    }
}

private fun looksLikeJson(text: String): Boolean {
    val t = text.trim()
    return (t.startsWith("{") && t.endsWith("}")) || (t.startsWith("[") && t.endsWith("]"))
}
