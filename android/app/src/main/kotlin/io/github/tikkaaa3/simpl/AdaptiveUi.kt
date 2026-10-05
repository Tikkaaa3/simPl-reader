package io.github.tikkaaa3.simpl

import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.window.layout.FoldingFeature
import androidx.window.layout.WindowInfoTracker
import kotlinx.coroutines.flow.map

internal val LocalFold = staticCompositionLocalOf<FoldingFeature?> { null }

@Composable
internal fun FoldAware(content: @Composable () -> Unit) {
    val activity = requireNotNull(LocalActivity.current)
    val flow = remember(activity) { WindowInfoTracker.getOrCreate(activity).windowLayoutInfo(activity).map { it.displayFeatures.filterIsInstance<FoldingFeature>() } }
    val features by flow.collectAsStateWithLifecycle(emptyList())
    CompositionLocalProvider(LocalFold provides features
        .firstOrNull { it.isSeparating || it.occlusionType == FoldingFeature.OcclusionType.FULL }, content = content)
}

/** Hinge bounds are in window coordinates. Neither pane is drawn through them. */
@Composable
internal fun AdaptivePanes(modifier: Modifier = Modifier, sideAtStart: Boolean = false, sideVisible: Boolean = true,
    /** Fills the pane beside a hinge while [side] is hidden, so half the screen is never blank. */
    idle: (@Composable () -> Unit)? = null,
    side: @Composable () -> Unit, content: @Composable (Boolean) -> Unit) {
    val fold = LocalFold.current
    val density = LocalDensity.current.density
    var origin by remember { mutableStateOf(Offset.Zero) }
    BoxWithConstraints(modifier.fillMaxSize().onGloballyPositioned { origin = it.positionInWindow() }) {
        val width = constraints.maxWidth.toFloat(); val height = constraints.maxHeight.toFloat()
        val vertical = fold?.orientation == FoldingFeature.Orientation.VERTICAL
        val start = fold?.bounds?.let { if (vertical) it.left - origin.x else it.top - origin.y } ?: 0f
        val end = fold?.bounds?.let { if (vertical) it.right - origin.x else it.bottom - origin.y } ?: 0f
        val extent = if (vertical) width else height
        val crosses = fold != null && start >= 0 && end <= extent && start < extent && end > 0
        val minimum = (if (vertical) 280 else 180) * density
        val split = crosses && start >= minimum && extent - end >= minimum
        val wide = !crosses && maxWidth >= 840.dp
        if (split) {
            val panel: @Composable () -> Unit = { Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surfaceContainer) { side() } }
            val first: @Composable () -> Unit = { if (sideAtStart) panel() else content(true) }
            val second: @Composable () -> Unit = { if (sideAtStart) content(true) else if (sideVisible) panel()
                else if (idle != null) Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surfaceContainer) { idle() } }
            if (vertical) Row(Modifier.fillMaxSize().testTag("adaptivePanes")) {
                Box(Modifier.width((start / density).dp).fillMaxHeight()) { first() }
                Spacer(Modifier.width(((end - start) / density).dp))
                Box(Modifier.weight(1f).fillMaxHeight()) { second() }
            } else Column(Modifier.fillMaxSize().testTag("adaptivePanes")) {
                Box(Modifier.height((start / density).dp).fillMaxWidth()) { first() }
                Spacer(Modifier.height(((end - start) / density).dp))
                Box(Modifier.weight(1f).fillMaxWidth()) { second() }
            }
        } else if (crosses) {
            // A narrow pane is unsuitable for the reader; use the larger side.
            val after = extent - end > start
            val offset = if (after) end else 0f
            val length = if (after) extent - end else start
            Box(if (vertical) Modifier.offset(x = (offset / density).dp).width((length / density).dp).fillMaxHeight()
                else Modifier.offset(y = (offset / density).dp).height((length / density).dp).fillMaxWidth()) { content(false) }
        } else if (wide && sideVisible) {
            // Desktop's docked panel: one tonal step from the canvas, a hairline edge, no shadow.
            Row(Modifier.fillMaxSize().testTag("adaptivePanes")) {
                if (sideAtStart) { Surface(Modifier.width(240.dp).fillMaxHeight().testTag("librarySidePanel"), color = MaterialTheme.colorScheme.surfaceContainer) { side() }; VerticalDivider() }
                Box(Modifier.weight(1f).fillMaxHeight()) { content(true) }
                if (!sideAtStart) { VerticalDivider(); Surface(Modifier.width(340.dp).fillMaxHeight().testTag("readerSidePanel"), color = MaterialTheme.colorScheme.surfaceContainer) { side() } }
            }
        } else content(wide)
    }
}
