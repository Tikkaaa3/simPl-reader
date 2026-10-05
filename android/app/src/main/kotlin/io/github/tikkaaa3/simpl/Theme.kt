package io.github.tikkaaa3.simpl

import android.content.Context
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.edit
import androidx.core.view.WindowCompat
import io.github.tikkaaa3.simpl.core.ReaderPalette
import io.github.tikkaaa3.simpl.core.ReaderTheme
import io.github.tikkaaa3.simpl.core.readerThemes
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

private val Geist = FontFamily(Font(R.font.geist_ui))
private val Literata = FontFamily(Font(R.font.literata_regular))

/** Mirrors the shared Default palette, used only if the core failed to start. */
private val FallbackDark = ReaderPalette(paper = 0x101010ffu, background = 0x0c0c0cffu, raised = 0x161616ffu, border = 0x272727ffu,
    text = 0xedededffu, secondary = 0xb5b5b5ffu, muted = 0x858585ffu, accent = 0x58a6ffffu, danger = 0xffb4abffu)
private val FallbackLight = ReaderPalette(paper = 0xf6f6f6ffu, background = 0xffffffffu, raised = 0xffffffffu, border = 0xe5e5e5ffu,
    text = 0x101010ffu, secondary = 0x525252ffu, muted = 0x737373ffu, accent = 0x0969daffu, danger = 0xa32b2bffu)

/** The bundled reading themes; like desktop, the chosen one also colors the app chrome. */
internal object ReadingThemes {
    val all: List<ReaderTheme> by lazy { runCatching { readerThemes() }.getOrDefault(emptyList()) }
    private val chosen = MutableStateFlow("default")
    val selected = chosen.asStateFlow()

    fun initialize(context: Context) { chosen.value = context.getSharedPreferences("reader", 0).getString("theme", "default")!! }
    fun select(context: Context, id: String) {
        context.getSharedPreferences("reader", 0).edit { putString("theme", id) }
        chosen.value = id
    }
    fun palette(id: String, dark: Boolean): ReaderPalette {
        val theme = all.firstOrNull { it.id == id } ?: all.firstOrNull()
        return (if (dark) theme?.dark else theme?.light) ?: if (dark) FallbackDark else FallbackLight
    }
}

/** Chrome roles follow desktop: canvas is `background`, panels are `raised`, quiet text is `secondary`. */
internal fun simplColors(palette: ReaderPalette, dark: Boolean): ColorScheme {
    val background = rgbaColor(palette.background); val paper = rgbaColor(palette.paper); val raised = rgbaColor(palette.raised)
    val text = rgbaColor(palette.text); val accent = rgbaColor(palette.accent); val border = rgbaColor(palette.border)
    val secondary = rgbaColor(palette.secondary); val danger = rgbaColor(palette.danger)
    // Panels must separate from the canvas even when a theme's raised tone equals it.
    val panel = if (raised == background) paper else raised
    val tint = lerp(background, accent, if (dark) .16f else .10f)
    val onAccent = if (dark) background else Color.White
    val base = if (dark) darkColorScheme() else lightColorScheme()
    return base.copy(
        primary = accent, onPrimary = onAccent, primaryContainer = tint, onPrimaryContainer = accent, inversePrimary = accent,
        secondary = accent, onSecondary = onAccent, secondaryContainer = tint, onSecondaryContainer = accent,
        tertiary = accent, onTertiary = onAccent, tertiaryContainer = tint, onTertiaryContainer = accent,
        background = background, onBackground = text, surface = background, onSurface = text,
        surfaceVariant = paper, onSurfaceVariant = secondary, surfaceTint = background,
        surfaceDim = background, surfaceBright = panel,
        surfaceContainerLowest = background, surfaceContainerLow = panel, surfaceContainer = paper,
        surfaceContainerHigh = panel, surfaceContainerHighest = lerp(panel, text, .08f),
        inverseSurface = text, inverseOnSurface = background,
        outline = border, outlineVariant = border,
        error = danger, onError = onAccent, errorContainer = lerp(background, danger, .14f), onErrorContainer = danger,
        scrim = Color.Black,
    )
}

/** Desktop's "Soft" shape scale: 4px controls, 8px cards and panels. */
private val Shapes = Shapes(
    extraSmall = RoundedCornerShape(4.dp), small = RoundedCornerShape(4.dp), medium = RoundedCornerShape(8.dp),
    large = RoundedCornerShape(8.dp), extraLarge = RoundedCornerShape(12.dp),
)
internal val ControlShape = RoundedCornerShape(4.dp)

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
internal fun isAppDark(appearance: Appearance) = when (appearance) { Appearance.System -> isSystemInDarkTheme(); Appearance.Dark -> true; Appearance.Light -> false }

@Composable
fun SimplTheme(appearance: Appearance, content: @Composable () -> Unit) {
    val dark = isAppDark(appearance)
    val view = LocalView.current
    if (!view.isInEditMode) SideEffect {
        (view.context as? android.app.Activity)?.let { activity ->
            WindowCompat.getInsetsController(activity.window, view).apply {
                isAppearanceLightStatusBars = !dark
                isAppearanceLightNavigationBars = !dark
            }
        }
    }
    SimplColors(dark, content)
}

/** Recolors a subtree, for example the reader when its paper differs from the app appearance. */
@Composable
internal fun SimplColors(dark: Boolean, content: @Composable () -> Unit) {
    val theme by ReadingThemes.selected.collectAsState()
    MaterialTheme(colorScheme = simplColors(ReadingThemes.palette(theme, dark), dark), shapes = Shapes, typography = Type, content = content)
}

/** Desktop's quiet button: neutral text that only tints on press. */
@Composable
internal fun quietButtonColors() = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onSurface)

/** Desktop's slim progress track: no gaps or stop dot, accent over the border tone. */
@Composable
internal fun ThinProgress(progress: Float, modifier: androidx.compose.ui.Modifier = androidx.compose.ui.Modifier, height: androidx.compose.ui.unit.Dp = 3.dp) =
    LinearProgressIndicator(progress = { progress.coerceIn(0f, 1f) }, modifier = modifier.height(height),
        color = MaterialTheme.colorScheme.primary, trackColor = MaterialTheme.colorScheme.outline,
        strokeCap = androidx.compose.ui.graphics.StrokeCap.Butt, gapSize = 0.dp, drawStopIndicator = {})
