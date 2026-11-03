package com.pilight.android.ui

import android.app.Activity
import android.app.KeyguardManager
import android.content.Context
import android.content.pm.ActivityInfo
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.DesktopWindows
import androidx.compose.material.icons.outlined.Fullscreen
import androidx.compose.material.icons.outlined.Keyboard
import androidx.compose.material.icons.outlined.MoreVert
import androidx.compose.material.icons.outlined.OpenWith
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.TouchApp
import androidx.compose.material.icons.outlined.Visibility
import androidx.compose.material.icons.outlined.Wifi
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.pilight.android.CompanionViewModel
import com.pilight.android.data.DesktopMode
import com.pilight.android.ui.theme.AccentBlue
import com.pilight.android.ui.theme.Connected
import com.pilight.android.ui.theme.PurpleBright
import com.pilight.android.ui.theme.PurplePrimary
import com.pilight.android.ui.theme.SurfaceCard
import com.pilight.android.ui.theme.SurfaceElevated
import kotlin.math.roundToInt

@Composable
fun DesktopScreen(
    vm: CompanionViewModel,
    onBack: () -> Unit,
    onSettings: () -> Unit,
    onFullscreenChange: (Boolean) -> Unit,
) {
    val home by vm.home.collectAsStateWithLifecycle()
    val security by vm.security.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val activity = context as? Activity
    var unlocked by remember { mutableStateOf(!security.biometricLock) }
    var mode by remember { mutableStateOf(DesktopMode.BROWSE) }
    var showHint by remember { mutableStateOf(true) }
    var showKeyboard by remember { mutableStateOf(false) }
    var pointer by remember { mutableStateOf(Offset(0.55f, 0.42f)) }
    var timedOut by remember { mutableStateOf(false) }

    val unlockLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            unlocked = result.resultCode == Activity.RESULT_OK
            if (unlocked) vm.markDesktopActivity()
        }

    LaunchedEffect(security.biometricLock) {
        if (!security.biometricLock) {
            unlocked = true
            vm.markDesktopActivity()
            return@LaunchedEffect
        }
        val km = context.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
        if (!km.isDeviceSecure) {
            unlocked = true
            vm.markDesktopActivity()
            return@LaunchedEffect
        }
        @Suppress("DEPRECATION")
        val intent = km.createConfirmDeviceCredentialIntent("远程桌面", "验证后进入远程桌面")
        if (intent != null) {
            unlockLauncher.launch(intent)
        } else {
            unlocked = true
            vm.markDesktopActivity()
        }
    }

    LaunchedEffect(mode) {
        onFullscreenChange(mode == DesktopMode.FULLSCREEN)
        showHint = mode != DesktopMode.FULLSCREEN
        activity?.requestedOrientation =
            if (mode == DesktopMode.FULLSCREEN) {
                ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
            } else {
                ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
            }
        vm.markDesktopActivity()
    }

    LaunchedEffect(home.latencyMs, home.connected) {
        if (vm.desktopTimedOut()) {
            timedOut = true
        }
    }

    DisposableEffect(Unit) {
        vm.markDesktopActivity()
        onDispose {
            activity?.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
            onFullscreenChange(false)
        }
    }

    SystemBarsHidden(hidden = mode == DesktopMode.FULLSCREEN)

    if (!unlocked) {
        Column(
            modifier = Modifier.fillMaxSize().padding(24.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text("远程桌面已锁定", fontWeight = FontWeight.SemiBold)
            Text("完成系统锁屏验证后进入。", color = MaterialTheme.colorScheme.secondary)
            TextButton(onClick = onBack) { Text("返回") }
        }
        return
    }

    if (timedOut) {
        Column(
            modifier = Modifier.fillMaxSize().padding(24.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text("远程桌面已超时", fontWeight = FontWeight.SemiBold)
            Text("超过 ${security.timeoutLabel} 无操作，已断开预览。", color = MaterialTheme.colorScheme.secondary)
            TextButton(
                onClick = {
                    timedOut = false
                    vm.markDesktopActivity()
                },
            ) { Text("重新进入") }
            TextButton(onClick = onBack) { Text("返回") }
        }
        return
    }

    val host = home.hostName.ifBlank { home.session?.hostName.orEmpty().ifBlank { "desktop" } }
    val latency = home.latencyMs?.let { "${it}ms" } ?: "—"

    if (mode == DesktopMode.FULLSCREEN) {
        FullscreenStage(
            host = host,
            latency = latency,
            connected = home.connected,
            onExit = { mode = DesktopMode.BROWSE },
        )
        return
    }

    Column(modifier = Modifier.fillMaxSize().statusBarsPadding()) {
        DesktopTopBar(
            mode = mode,
            host = host,
            connected = home.connected,
            onBack = onBack,
        )
        when (mode) {
            DesktopMode.BROWSE -> {
                BrowseStatusRow(connected = home.connected, latency = latency)
            }
            DesktopMode.TOUCH -> {
                TouchStatusCard()
                ConnectionChip(connected = home.connected, latency = latency)
            }
            DesktopMode.FULLSCREEN -> Unit
        }
        Box(
            modifier =
                Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            HostPreview(
                host = host,
                relay = home.session?.relayUrl.orEmpty(),
                connected = home.connected,
                latency = latency,
                interactive = mode == DesktopMode.TOUCH,
                pointer = pointer,
                onPointer = {
                    pointer = it
                    vm.markDesktopActivity()
                },
            )
            if (mode == DesktopMode.BROWSE && showHint) {
                ModeHintPopup(
                    modifier = Modifier.align(Alignment.BottomEnd).padding(12.dp),
                    onDismiss = { showHint = false },
                )
            }
            if (mode == DesktopMode.TOUCH) {
                TouchControls(
                    modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = 8.dp),
                    showKeyboard = showKeyboard,
                    onToggleKeyboard = { showKeyboard = !showKeyboard },
                )
            }
        }
        if (mode == DesktopMode.TOUCH && showKeyboard) {
            MappedKeyboardBar(onClose = { showKeyboard = false })
        }
        if (mode == DesktopMode.TOUCH) {
            TouchBottomBar(onFullscreen = { mode = DesktopMode.FULLSCREEN }, onSettings = onSettings)
        } else {
            ModeBar(mode = mode, onMode = { mode = it })
        }
    }
}

@Composable
private fun DesktopTopBar(
    mode: DesktopMode,
    host: String,
    connected: Boolean,
    onBack: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = onBack) {
            Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = "返回")
        }
        when (mode) {
            DesktopMode.BROWSE -> {
                BrandMark(size = 22.dp)
                Spacer(Modifier.size(8.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text("远程桌面 · 公网中继", fontWeight = FontWeight.SemiBold)
                    Text(
                        "pi-light · ${if (connected) "已连接" else "未连接"} · $host",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.secondary,
                    )
                }
                Icon(Icons.Outlined.Wifi, contentDescription = null, tint = PurplePrimary)
            }
            DesktopMode.TOUCH -> {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Windows 桌面 · $host", fontWeight = FontWeight.SemiBold)
                    Text("触控 · 映射鼠标键盘", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.secondary)
                }
                Icon(Icons.Outlined.DesktopWindows, contentDescription = null, tint = PurplePrimary)
            }
            DesktopMode.FULLSCREEN -> Unit
        }
        IconButton(onClick = { }) {
            Icon(Icons.Outlined.MoreVert, contentDescription = "更多")
        }
    }
}

@Composable
private fun BrowseStatusRow(connected: Boolean, latency: String) {
    Row(
        modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(8.dp).clip(CircleShape).background(if (connected) Connected else MaterialTheme.colorScheme.secondary))
        Spacer(Modifier.size(8.dp))
        Text(
            if (connected) "已连接中继 · 桌面预览  ·  延迟 $latency" else "中继未就绪",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.secondary,
        )
    }
}

@Composable
private fun TouchStatusCard() {
    Row(
        modifier =
            Modifier
                .padding(horizontal = 16.dp, vertical = 4.dp)
                .fillMaxWidth()
                .clip(RoundedCornerShape(16.dp))
                .background(SurfaceCard)
                .padding(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BrandMark(size = 28.dp)
        Spacer(Modifier.size(10.dp))
        Column {
            Text("触控 · 映射鼠标键盘", fontWeight = FontWeight.Medium)
            Text("点按映射指针 · 流式远程桌面壳", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.secondary)
        }
    }
}

@Composable
private fun ConnectionChip(connected: Boolean, latency: String) {
    Row(
        modifier = Modifier.padding(horizontal = 20.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(8.dp).clip(CircleShape).background(if (connected) Connected else MaterialTheme.colorScheme.secondary))
        Spacer(Modifier.size(8.dp))
        Text(
            if (connected) "已连接 · 中继预览 · 延迟 $latency" else "未连接",
            color = if (connected) Connected else MaterialTheme.colorScheme.secondary,
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

@Composable
private fun HostPreview(
    host: String,
    relay: String,
    connected: Boolean,
    latency: String,
    interactive: Boolean,
    pointer: Offset,
    onPointer: (Offset) -> Unit,
) {
    BoxWithConstraints(
        modifier =
            Modifier
                .fillMaxSize()
                .clip(RoundedCornerShape(16.dp))
                .background(Color(0xFF101010))
                .then(
                    if (interactive) {
                        Modifier.pointerInput(Unit) {
                            detectTapGestures { pos ->
                                onPointer(
                                    Offset(
                                        (pos.x / size.width).coerceIn(0.05f, 0.95f),
                                        (pos.y / size.height).coerceIn(0.08f, 0.92f),
                                    ),
                                )
                            }
                        }.pointerInput(Unit) {
                            detectDragGestures { change, _ ->
                                val pos = change.position
                                onPointer(
                                    Offset(
                                        (pos.x / size.width).coerceIn(0.05f, 0.95f),
                                        (pos.y / size.height).coerceIn(0.08f, 0.92f),
                                    ),
                                )
                            }
                        }
                    } else {
                        Modifier
                    },
                ),
    ) {
        Column(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .padding(12.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(Color(0xFF161616))
                    .border(1.dp, Color(0xFF2A2A2A), RoundedCornerShape(10.dp)),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth().background(Color(0xFF1C1C1C)).padding(horizontal = 10.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                BrandMark(size = 16.dp)
                Spacer(Modifier.size(6.dp))
                Text("pi-light", fontWeight = FontWeight.Medium, fontSize = 13.sp)
                Spacer(Modifier.weight(1f))
                Box(Modifier.size(8.dp).clip(CircleShape).background(if (connected) Connected else Color.Gray))
                Spacer(Modifier.size(4.dp))
                Text(if (connected) "已连接" else "离线", fontSize = 11.sp, color = MaterialTheme.colorScheme.secondary)
                Spacer(Modifier.size(8.dp))
                Text("— □ ×", fontSize = 11.sp, color = MaterialTheme.colorScheme.secondary)
            }
            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("pi-light", fontWeight = FontWeight.SemiBold, fontSize = 18.sp)
                Text("Lightweight remote access", color = MaterialTheme.colorScheme.secondary, fontSize = 12.sp)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                    InfoTile("系统", host, Modifier.weight(1f))
                    InfoTile("中继", relay.ifBlank { "—" }.removePrefix("https://").take(22), Modifier.weight(1f))
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                    InfoTile("延迟", latency, Modifier.weight(1f))
                    Column(
                        modifier =
                            Modifier
                                .weight(1f)
                                .clip(RoundedCornerShape(10.dp))
                                .background(Color(0xFF111111))
                                .padding(10.dp),
                    ) {
                        Text("服务", color = MaterialTheme.colorScheme.secondary, fontSize = 11.sp)
                        Text("配对 /v1/pair", color = Connected, fontSize = 12.sp)
                        Text("聊天 /v1/chat", color = Connected, fontSize = 12.sp)
                        Text("像素流 未接入", color = MaterialTheme.colorScheme.secondary, fontSize = 12.sp)
                    }
                }
                Column {
                    Text("预览通道", color = MaterialTheme.colorScheme.secondary, fontSize = 11.sp)
                    Spacer(Modifier.height(4.dp))
                    LinearProgressIndicator(
                        progress = { if (connected) 0.22f else 0f },
                        modifier = Modifier.fillMaxWidth().height(6.dp).clip(CircleShape),
                        color = AccentBlue,
                        trackColor = Color(0xFF2A2A2A),
                    )
                }
            }
        }
        if (interactive) {
            Icon(
                Icons.Outlined.TouchApp,
                contentDescription = "触控指针",
                tint = AccentBlue,
                modifier =
                    Modifier
                        .align(Alignment.TopStart)
                        .offset {
                            IntOffset(
                                (maxWidth.toPx() * pointer.x).roundToInt() - 12,
                                (maxHeight.toPx() * pointer.y).roundToInt() - 8,
                            )
                        }
                        .size(42.dp),
            )
        }
    }
}

@Composable
private fun InfoTile(title: String, value: String, modifier: Modifier = Modifier) {
    Column(
        modifier =
            modifier
                .clip(RoundedCornerShape(10.dp))
                .background(Color(0xFF111111))
                .padding(10.dp),
    ) {
        Text(title, color = MaterialTheme.colorScheme.secondary, fontSize = 11.sp)
        Text(value, fontSize = 13.sp, maxLines = 2)
    }
}

@Composable
private fun ModeHintPopup(
    modifier: Modifier = Modifier,
    onDismiss: () -> Unit,
) {
    Column(
        modifier =
            modifier
                .clip(RoundedCornerShape(14.dp))
                .background(SurfaceElevated)
                .clickable(onClick = onDismiss)
                .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        DesktopMode.entries.forEach { item ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(modeIcon(item), contentDescription = null, tint = PurplePrimary, modifier = Modifier.size(18.dp))
                Spacer(Modifier.size(8.dp))
                Text(item.hint, style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

@Composable
private fun ModeBar(
    mode: DesktopMode,
    onMode: (DesktopMode) -> Unit,
) {
    NavigationBar(containerColor = Color(0xFF0E0E12), modifier = Modifier.navigationBarsPadding()) {
        DesktopMode.entries.forEach { item ->
            NavigationBarItem(
                selected = mode == item,
                onClick = { onMode(item) },
                icon = { Icon(modeIcon(item), contentDescription = item.label) },
                label = { Text(item.label) },
                colors =
                    NavigationBarItemDefaults.colors(
                        selectedIconColor = Color.White,
                        selectedTextColor = Color.White,
                        indicatorColor = AccentBlue,
                        unselectedIconColor = MaterialTheme.colorScheme.secondary,
                        unselectedTextColor = MaterialTheme.colorScheme.secondary,
                    ),
            )
        }
    }
}

@Composable
private fun TouchBottomBar(
    onFullscreen: () -> Unit,
    onSettings: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .padding(horizontal = 28.dp, vertical = 10.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TextButton(onClick = onFullscreen) {
            Icon(Icons.Outlined.Fullscreen, contentDescription = null)
            Spacer(Modifier.size(6.dp))
            Text("全屏")
        }
        TextButton(onClick = onSettings) {
            Icon(Icons.Outlined.Settings, contentDescription = null)
            Spacer(Modifier.size(6.dp))
            Text("设置")
        }
    }
}

@Composable
private fun TouchControls(
    modifier: Modifier = Modifier,
    showKeyboard: Boolean,
    onToggleKeyboard: () -> Unit,
) {
    Row(modifier = modifier, verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Box(
            modifier =
                Modifier
                    .size(56.dp)
                    .clip(CircleShape)
                    .background(SurfaceElevated)
                    .clickable(onClick = onToggleKeyboard),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Outlined.Keyboard, contentDescription = "键盘", tint = if (showKeyboard) PurpleBright else Color.White)
        }
        Column(
            modifier =
                Modifier
                    .clip(CircleShape)
                    .background(SurfaceElevated)
                    .padding(horizontal = 18.dp, vertical = 10.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text("滚动", color = MaterialTheme.colorScheme.secondary, fontSize = 12.sp)
            Text("⌃", fontSize = 16.sp)
            Text("右键", modifier = Modifier.padding(vertical = 4.dp))
            Text("⌄", fontSize = 16.sp)
        }
    }
}

@Composable
private fun MappedKeyboardBar(onClose: () -> Unit) {
    var draft by remember { mutableStateOf("") }
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp)
                .clip(RoundedCornerShape(16.dp))
                .background(SurfaceCard)
                .padding(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TextField(
            value = draft,
            onValueChange = { draft = it },
            modifier = Modifier.weight(1f),
            placeholder = { Text("映射键盘（桌面输入通道尚未开通）") },
            colors =
                TextFieldDefaults.colors(
                    focusedContainerColor = Color.Transparent,
                    unfocusedContainerColor = Color.Transparent,
                    focusedIndicatorColor = Color.Transparent,
                    unfocusedIndicatorColor = Color.Transparent,
                ),
        )
        TextButton(onClick = onClose) { Text("收起") }
    }
}

@Composable
private fun FullscreenStage(
    host: String,
    latency: String,
    connected: Boolean,
    onExit: () -> Unit,
) {
    Box(
        modifier =
            Modifier
                .fillMaxSize()
                .background(Color(0xFF7EB6F0)),
    ) {
        Box(
            modifier =
                Modifier
                    .fillMaxSize()
                    .background(Color(0x33447ACC)),
        )
        Row(
            modifier =
                Modifier
                    .align(Alignment.TopStart)
                    .statusBarsPadding()
                    .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BrandMark(size = 28.dp)
            Spacer(Modifier.size(8.dp))
            Text("pi-light", fontWeight = FontWeight.SemiBold, color = Color.White)
        }
        Row(
            modifier =
                Modifier
                    .align(Alignment.TopEnd)
                    .statusBarsPadding()
                    .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier =
                    Modifier
                        .clip(RoundedCornerShape(20.dp))
                        .background(Color(0x99000000))
                        .padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.Outlined.Wifi, contentDescription = null, tint = Color.White, modifier = Modifier.size(14.dp))
                Spacer(Modifier.size(4.dp))
                Text("延迟:$latency", color = Color.White, fontSize = 12.sp)
                Spacer(Modifier.size(6.dp))
                Icon(
                    Icons.Outlined.Close,
                    contentDescription = "关闭",
                    tint = Color.White,
                    modifier = Modifier.size(14.dp).clickable(onClick = onExit),
                )
            }
        }
        TextButton(
            onClick = onExit,
            modifier =
                Modifier
                    .align(Alignment.TopEnd)
                    .statusBarsPadding()
                    .padding(top = 52.dp, end = 16.dp)
                    .clip(RoundedCornerShape(20.dp))
                    .background(Color(0xE6FFFFFF)),
        ) {
            Icon(Icons.Outlined.OpenWith, contentDescription = null, tint = Color.DarkGray)
            Spacer(Modifier.size(6.dp))
            Text("退出全屏", color = Color.DarkGray)
        }
        Column(
            modifier = Modifier.align(Alignment.Center),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(host, color = Color.White, fontWeight = FontWeight.SemiBold, fontSize = 20.sp)
            Text(
                if (connected) "全屏沉浸 · 中继预览" else "中继未连接",
                color = Color.White.copy(alpha = 0.85f),
            )
        }
    }
}

@Composable
private fun SystemBarsHidden(hidden: Boolean) {
    val view = LocalView.current
    val window = (view.context as? Activity)?.window
    DisposableEffect(hidden, window) {
        val w = window
        if (w != null) {
            val controller = WindowInsetsControllerCompat(w, view)
            if (hidden) {
                WindowCompat.setDecorFitsSystemWindows(w, false)
                controller.hide(WindowInsetsCompat.Type.systemBars())
                controller.systemBarsBehavior =
                    WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            } else {
                controller.show(WindowInsetsCompat.Type.systemBars())
            }
        }
        onDispose {
            window?.let { w ->
                WindowInsetsControllerCompat(w, view).show(WindowInsetsCompat.Type.systemBars())
            }
        }
    }
}

private fun modeIcon(mode: DesktopMode): ImageVector =
    when (mode) {
        DesktopMode.BROWSE -> Icons.Outlined.Visibility
        DesktopMode.TOUCH -> Icons.Outlined.TouchApp
        DesktopMode.FULLSCREEN -> Icons.Outlined.Fullscreen
    }
