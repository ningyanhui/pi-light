package com.pilight.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.pilight.android.CompanionViewModel
import com.pilight.android.data.ChatLine
import com.pilight.android.ui.theme.Connected
import com.pilight.android.ui.theme.PurpleBright
import com.pilight.android.ui.theme.PurpleDeep
import com.pilight.android.ui.theme.PurplePrimary
import com.pilight.android.ui.theme.SurfaceCard

@Composable
fun ChatScreen(
    vm: CompanionViewModel,
    onNeedPair: () -> Unit,
) {
    val home by vm.home.collectAsStateWithLifecycle()
    val chat by vm.chat.collectAsStateWithLifecycle()
    var draft by rememberSaveable { mutableStateOf("") }
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
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("pi-light", fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                Text(
                    if (home.connected) "● 已连接" else "○ 未连接",
                    color = if (home.connected) Connected else MaterialTheme.colorScheme.secondary,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Text(
                "Android 聊天 · 远程桌面助手",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary,
            )
            val latency = home.latencyMs?.let { "延迟 ${it}ms" } ?: "延迟 —"
            Text(
                "${home.hostName.ifBlank { home.session?.hostName }}  ·  $latency",
                style = MaterialTheme.typography.bodySmall,
                color = PurplePrimary,
            )
        }

        Box(
            modifier =
                Modifier
                    .padding(horizontal = 16.dp)
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(14.dp))
                    .background(SurfaceCard)
                    .padding(12.dp),
        ) {
            Column {
                Text("会话", color = MaterialTheme.colorScheme.secondary, style = MaterialTheme.typography.bodySmall)
                Text(chat.title, fontWeight = FontWeight.Medium)
            }
        }

        LazyColumn(
            state = listState,
            modifier = Modifier.weight(1f).fillMaxWidth(),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            items(chat.messages) { line ->
                MessageBubble(line)
            }
            if (chat.streaming || chat.partial.isNotBlank() || chat.sending) {
                item {
                    MessageBubble(
                        ChatLine(
                            role = "assistant",
                            text = chat.partial.ifBlank { "思考中…" },
                        ),
                        thinking = chat.partial.isBlank(),
                    )
                }
            }
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
            modifier = Modifier.fillMaxWidth().padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedTextField(
                value = draft,
                onValueChange = { draft = it },
                modifier = Modifier.weight(1f),
                placeholder = { Text("输入消息或使用 / 唤起命令…") },
                enabled = !chat.sending && chat.writable,
            )
            Spacer(Modifier.size(8.dp))
            FilledIconButton(
                onClick = {
                    val text = draft
                    draft = ""
                    vm.send(text)
                },
                enabled = draft.isNotBlank() && !chat.sending && chat.writable,
            ) {
                Icon(Icons.AutoMirrored.Outlined.Send, contentDescription = "发送")
            }
        }
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.Outlined.Lock, contentDescription = null, modifier = Modifier.size(14.dp), tint = MaterialTheme.colorScheme.secondary)
            Spacer(Modifier.size(6.dp))
            Text(
                "经中继加密 · 写入桌面当前 Pi 会话",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary,
            )
        }
    }
}

@Composable
private fun MessageBubble(
    line: ChatLine,
    thinking: Boolean = false,
) {
    val mine = line.role == "user"
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = if (mine) Arrangement.End else Arrangement.Start,
    ) {
        if (!mine) {
            Box(
                modifier =
                    Modifier
                        .padding(end = 8.dp)
                        .size(28.dp)
                        .clip(CircleShape)
                        .background(PurpleDeep),
                contentAlignment = Alignment.Center,
            ) {
                Text("π", color = PurplePrimary, fontWeight = FontWeight.Bold)
            }
        }
        Box(
            modifier =
                Modifier
                    .fillMaxWidth(0.82f)
                    .clip(RoundedCornerShape(14.dp))
                    .background(if (mine) PurpleBright.copy(alpha = 0.25f) else SurfaceCard)
                    .padding(12.dp),
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
