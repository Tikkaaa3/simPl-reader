package io.github.tikkaaa3.simpl

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.sp
import androidx.core.view.WindowCompat

private val Geist = FontFamily(Font(R.font.geist_ui))
private val Literata = FontFamily(Font(R.font.literata_regular))
private val Dark = darkColorScheme(
    primary = Color(0xff58a6ff), onPrimary = Color(0xff0d1117),
    primaryContainer = Color(0xff193858), onPrimaryContainer = Color(0xffcce4ff),
    secondary = Color(0xff58a6ff), onSecondary = Color(0xff0d1117),
    secondaryContainer = Color(0xff193858), onSecondaryContainer = Color(0xffcce4ff),
    background = Color(0xff0d1117), onBackground = Color(0xffe6edf3),
    surface = Color(0xff0d1117), onSurface = Color(0xffe6edf3),
    surfaceVariant = Color(0xff161b22), onSurfaceVariant = Color(0xff8b949e),
    surfaceContainer = Color(0xff161b22), surfaceContainerHigh = Color(0xff21262d),
    outline = Color(0xff30363d), outlineVariant = Color(0xff30363d),
)
private val Light = lightColorScheme(
    primary = Color(0xff0969da), onPrimary = Color.White,
    primaryContainer = Color(0xffddedff), onPrimaryContainer = Color(0xff12395b),
    secondary = Color(0xff0969da), onSecondary = Color.White,
    secondaryContainer = Color(0xffddedff), onSecondaryContainer = Color(0xff12395b),
    background = Color.White, onBackground = Color(0xff101010),
    surface = Color.White, onSurface = Color(0xff101010),
    surfaceVariant = Color(0xfff6f6f6), onSurfaceVariant = Color(0xff656d76),
    surfaceContainer = Color(0xfff6f6f6), surfaceContainerHigh = Color(0xffeeeeee),
    outline = Color(0xffd0d7de), outlineVariant = Color(0xffe5e5e5),
)
private val Type = Typography(
    headlineLarge = TextStyle(fontFamily = Literata, fontSize = 32.sp, lineHeight = 42.sp),
    headlineMedium = TextStyle(fontFamily = Literata, fontSize = 26.sp, lineHeight = 36.sp),
    headlineSmall = TextStyle(fontFamily = Literata, fontSize = 22.sp, lineHeight = 30.sp),
    titleLarge = TextStyle(fontFamily = Geist, fontSize = 22.sp, lineHeight = 28.sp),
    titleMedium = TextStyle(fontFamily = Geist, fontSize = 16.sp, lineHeight = 24.sp),
    titleSmall = TextStyle(fontFamily = Geist, fontSize = 14.sp, lineHeight = 20.sp),
    bodyLarge = TextStyle(fontFamily = Geist, fontSize = 16.sp, lineHeight = 24.sp),
    bodyMedium = TextStyle(fontFamily = Geist, fontSize = 14.sp, lineHeight = 22.sp),
    bodySmall = TextStyle(fontFamily = Geist, fontSize = 12.sp, lineHeight = 18.sp),
    labelLarge = TextStyle(fontFamily = Geist, fontSize = 14.sp, lineHeight = 20.sp),
    labelMedium = TextStyle(fontFamily = Geist, fontSize = 12.sp, lineHeight = 16.sp),
    labelSmall = TextStyle(fontFamily = Geist, fontSize = 11.sp, lineHeight = 16.sp),
)

@Composable
fun SimplTheme(appearance: Appearance, content: @Composable () -> Unit) {
    val dark = when (appearance) { Appearance.System -> isSystemInDarkTheme(); Appearance.Dark -> true; Appearance.Light -> false }
    val view = LocalView.current
    if (!view.isInEditMode) SideEffect {
        (view.context as? android.app.Activity)?.let { activity ->
            WindowCompat.getInsetsController(activity.window, view).apply {
                isAppearanceLightStatusBars = !dark
                isAppearanceLightNavigationBars = !dark
            }
        }
    }
    MaterialTheme(colorScheme = if (dark) Dark else Light, typography = Type, content = content)
}
