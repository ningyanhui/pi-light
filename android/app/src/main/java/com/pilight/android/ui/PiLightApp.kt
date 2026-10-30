package com.pilight.android.ui

import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ChatBubbleOutline
import androidx.compose.material.icons.outlined.Computer
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavDestination.Companion.hierarchy
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.pilight.android.CompanionViewModel

private data class Tab(
    val route: String,
    val label: String,
    val icon: androidx.compose.ui.graphics.vector.ImageVector,
)

@Composable
fun PiLightApp(vm: CompanionViewModel = viewModel()) {
    val navController = rememberNavController()
    val tabs =
        listOf(
            Tab("home", "设备", Icons.Outlined.Computer),
            Tab("sessions", "会话", Icons.Outlined.ChatBubbleOutline),
            Tab("settings", "设置", Icons.Outlined.Settings),
        )
    val navBackStackEntry by navController.currentBackStackEntryAsState()
    val current = navBackStackEntry?.destination
    val showBar = current?.route in setOf("home", "sessions", "settings")

    Scaffold(
        bottomBar = {
            if (showBar) {
                NavigationBar {
                    tabs.forEach { tab ->
                        NavigationBarItem(
                            icon = { Icon(tab.icon, contentDescription = tab.label) },
                            label = { Text(tab.label) },
                            selected = current?.hierarchy?.any { it.route == tab.route } == true,
                            onClick = {
                                navController.navigate(tab.route) {
                                    popUpTo(navController.graph.findStartDestination().id) {
                                        saveState = true
                                    }
                                    launchSingleTop = true
                                    restoreState = true
                                }
                            },
                        )
                    }
                }
            }
        },
    ) { innerPadding ->
        NavHost(
            navController = navController,
            startDestination = "home",
            modifier = Modifier.padding(innerPadding),
        ) {
            composable("home") {
                HomeScreen(
                    vm = vm,
                    onPair = { navController.navigate("pair") },
                    onChat = {
                        navController.navigate("sessions") {
                            launchSingleTop = true
                        }
                    },
                    onDesktop = { navController.navigate("desktop-soon") },
                )
            }
            composable("sessions") {
                ChatScreen(
                    vm = vm,
                    onNeedPair = { navController.navigate("pair") },
                )
            }
            composable("settings") {
                SettingsScreen(
                    vm = vm,
                    onPair = { navController.navigate("pair") },
                )
            }
            composable("pair") {
                PairScreen(
                    vm = vm,
                    onBack = { navController.popBackStack() },
                    onScan = { navController.navigate("scan") },
                    onPaired = {
                        navController.popBackStack("home", inclusive = false)
                    },
                )
            }
            composable("scan") {
                QrScanScreen(
                    onResult = { raw ->
                        vm.pairFromQr(raw)
                        navController.popBackStack()
                    },
                    onCancel = { navController.popBackStack() },
                )
            }
            composable("desktop-soon") {
                DesktopSoonScreen(onBack = { navController.popBackStack() })
            }
        }
    }
}
