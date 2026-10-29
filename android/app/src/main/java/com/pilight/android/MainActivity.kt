package com.pilight.android

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import com.pilight.android.ui.PiLightApp
import com.pilight.android.ui.theme.PiLightTheme

/**
 * pi-light Android — Option C 占位壳（M6′ 维持 C）。
 * 不内嵌 Pi；完整会话以 Windows desktop-rust 为准。
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            PiLightTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background,
                ) {
                    PiLightApp()
                }
            }
        }
    }
}
