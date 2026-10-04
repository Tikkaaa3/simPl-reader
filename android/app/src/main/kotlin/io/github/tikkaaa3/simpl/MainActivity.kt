package io.github.tikkaaa3.simpl

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.github.tikkaaa3.simpl.core.buildInfo

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val core = describeCore()
        setContent {
            SimplTheme {
                CoreStatus(core)
            }
        }
    }
}

/** One line about the loaded Rust core, or why it could not be loaded. */
private fun describeCore(): String =
    try {
        val info = buildInfo()
        val profile = if (info.debug) "debug" else "release"
        "Rust core ${info.coreVersion} · ${info.os}/${info.arch} · $profile"
    } catch (error: Throwable) {
        "Rust core unavailable: ${error.message ?: error::class.java.simpleName}"
    }

@Composable
private fun SimplTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme(),
        content = content,
    )
}

@Composable
private fun CoreStatus(core: String) {
    Surface(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier.fillMaxSize().safeDrawingPadding().padding(24.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text("simPl", style = MaterialTheme.typography.headlineMedium)
            Text(core, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(top = 8.dp))
        }
    }
}

@Preview
@Composable
private fun CoreStatusPreview() {
    SimplTheme { CoreStatus("Rust core 0.0.0 · android/aarch64 · debug") }
}
