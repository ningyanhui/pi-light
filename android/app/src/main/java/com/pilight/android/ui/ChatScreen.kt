package com.pilight.android.ui

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
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.AttachFile
import androidx.compose.material.icons.outlined.ChatBubbleOutline
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.Menu
import androidx.compose.material.icons.outlined.SmartToy
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.pilight.android.CompanionViewModel
import com.pilight.android.data.ChatLine
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
    var draft by rememberSaveable { mutableStateOf("") }
    var notice by rememberSaveable { mutableStateOf<String?>(null) }
    val listState = rememberLazyListState()

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

    LaunchedEffect(chat.messages.size, chat.partial, chat.streaming, chat.sending) {
        val extra = if (chat.streaming || chat.partial.isNotBlank() || chat.sending) 1 else 0
        val count = chat.messages.size + extra
        if (count > 0) {
            listState.animateScrollToItem(count - 1)
        }
    }

    Column(modifier = Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.Outlined.Menu, contentDescription = null, tint = PurplePrimary)
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

        Column(
            modifier =
                Modifier
                    .padding(horizontal = 16.dp)
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(18.dp))
                    .background(SurfaceCard)
                    .padding(12.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Outlined.Folder, contentDescription = null, tint = PurplePrimary, modifier = Modifier.size(18.dp))
                Spacer(Modifier.size(6.dp))
                Text("会话列表", fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f))
                TextButton(
                    onClick = { notice = "新会话由桌面创建。companion 写入当前 Pi 会话，不会另起一套会话树。" },
                ) {
                    Icon(Icons.Outlined.Add, contentDescription = null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.size(4.dp))
                    Text("新建会话")
                }
            }
            SessionRow(
                title = chat.title.ifBlank { "当前桌面会话" },
                preview = chat.messages.lastOrNull()?.text?.ifBlank { null }
                    ?: if (chat.streaming) "思考中…" else "写入桌面当前 Pi 会话",
                selected = true,
                connected = home.connected,
            )
            TextButton(
                onClick = { notice = "companion 只镜像桌面当前会话，没有独立的会话树。" },
                modifier = Modifier.align(Alignment.End),
            ) {
                Text("查看所有会话")
            }
        }

        Row(
            modifier =
                Modifier
                    .padding(horizontal = 16.dp, vertical = 8.dp)
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(14.dp))
                    .background(SurfaceElevated)
                    .padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("✦", color = PurplePrimary)
            Spacer(Modifier.size(8.dp))
            Text("模型", color = MaterialTheme.colorScheme.secondary, style = MaterialTheme.typography.bodySmall)
            Spacer(Modifier.size(10.dp))
            Text("桌面当前会话", fontWeight = FontWeight.Medium)
            Spacer(Modifier.weight(1f))
            Text("▾", color = MaterialTheme.colorScheme.secondary)
        }

        LazyColumn(
            state = listState,
            modifier = Modifier.weight(1f).fillMaxWidth(),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            items(chat.messages) { line ->
                MessageBlock(line)
            }
            if (chat.sending) {
                item {
                    ToolCard(
                        title = "写入桌面会话",
                        body = "POST /v1/chat",
                        done = false,
                    )
                }
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
                    .clip(RoundedCornerShape(28.dp))
                    .background(SurfaceCard)
                    .padding(horizontal = 6.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = { notice = "附件通道尚未对 companion 开放，消息仍走 /v1/chat 文本。" }) {
                Icon(Icons.Outlined.AttachFile, contentDescription = "附件", tint = PurplePrimary)
            }
            TextField(
                value = draft,
                onValueChange = { draft = it },
                modifier = Modifier.weight(1f),
                placeholder = { Text("输入消息或使用 / 唤起命令…") },
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
            FilledIconButton(
                onClick = {
                    val text = draft
                    draft = ""
                    notice = null
                    vm.send(text)
                },
                enabled = draft.isNotBlank() && !chat.sending && chat.writable,
                colors = IconButtonDefaults.filledIconButtonColors(containerColor = PurpleBright),
            ) {
                Icon(Icons.AutoMirrored.Outlined.Send, contentDescription = "发送")
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
                .clip(RoundedCornerShape(14.dp))
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
        ToolCard(title = "工具输出", body = line.text, done = true)
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
private fun ToolCard(
    title: String,
    body: String,
    done: Boolean,
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
                Icons.Outlined.CheckCircle,
                contentDescription = null,
                tint = if (done) Connected else MaterialTheme.colorScheme.secondary,
                modifier = Modifier.size(16.dp),
            )
            Spacer(Modifier.size(4.dp))
            Text(if (done) "已完成" else "进行中", color = if (done) Connected else MaterialTheme.colorScheme.secondary, fontSize = 12.sp)
        }
        Spacer(Modifier.height(8.dp))
        Text(body, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
    }
}

private fun looksLikeJson(text: String): Boolean {
    val t = text.trim()
    return (t.startsWith("{") && t.endsWith("}")) || (t.startsWith("[") && t.endsWith("]"))
}
