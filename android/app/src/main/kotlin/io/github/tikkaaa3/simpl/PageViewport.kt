package io.github.tikkaaa3.simpl

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import kotlin.math.max
import kotlin.math.min

/** Page geometry in viewport pixels, including the space around a fitted page.
 * A top-aligned page starts at the viewport top instead of being centered vertically. */
internal data class PageGeometry(val viewportWidth: Float, val viewportHeight: Float,
    val logicalWidth: Float, val logicalHeight: Float, val scale: Float, val topAligned: Boolean = false) {
    val width = logicalWidth * scale
    val height = logicalHeight * scale
    val inset = Offset(max(0f, (viewportWidth - width) / 2), if (topAligned) 0f else max(0f, (viewportHeight - height) / 2))
    val maximum = Offset(max(0f, width - viewportWidth), max(0f, height - viewportHeight))
    val center = Offset(viewportWidth / 2, viewportHeight / 2)
    fun source(point: Offset, scroll: Offset) = (point + scroll - inset) / scale
    fun screen(point: Offset, scroll: Offset) = point * scale + inset - scroll
    fun clamp(scroll: Offset) = Offset(scroll.x.coerceIn(0f, maximum.x), scroll.y.coerceIn(0f, maximum.y))
    fun anchor(scroll: Offset, focus: Offset = center) = PageAnchor(source(focus, scroll), focus)
    companion object {
        fun fitted(width: Float, height: Float, pageWidth: Float, pageHeight: Float, zoom: Float, fitWidth: Boolean, topAligned: Boolean = false) =
            PageGeometry(width, height, pageWidth, pageHeight,
                (if (fitWidth) width.coerceAtLeast(1f) / pageWidth else min(width.coerceAtLeast(1f) / pageWidth, height.coerceAtLeast(1f) / pageHeight)) * zoom, topAligned)
    }
}

internal data class PageAnchor(val source: Offset, val focus: Offset) {
    fun scroll(geometry: PageGeometry) = geometry.clamp(source * geometry.scale + geometry.inset - focus)
}

/** Keeps the old coordinates until the new scroll bounds have been measured. */
internal class PagePlacement {
    var geometry: PageGeometry? = null
    var revision = -1
    var scroll = Offset.Zero
    var pending: PageAnchor? = null
    fun zoomAt(previousFocus: Offset, focus: Offset = previousFocus) {
        geometry?.let { pending = PageAnchor(it.source(previousFocus, scroll), focus) }
    }
    fun resized(next: PageGeometry): Offset =
        pending?.scroll(next) ?: geometry?.let { previous ->
            PageAnchor(previous.source(previous.center, scroll), next.center).scroll(next)
        } ?: Offset.Zero
}

@Composable
internal fun TrackPageScroll(placement: PagePlacement, geometry: PageGeometry, revision: Int,
    horizontal: ScrollState, vertical: ScrollState) {
    LaunchedEffect(placement, geometry, revision) {
        snapshotFlow { Offset(horizontal.value.toFloat(), vertical.value.toFloat()) }.collect { scroll ->
            if (placement.geometry == geometry && placement.revision == revision) placement.scroll = scroll
        }
    }
}

@Composable
internal fun PageScrollIndicator(geometry: PageGeometry, scroll: () -> Offset, color: Color) {
    if (geometry.maximum.x > 1f || geometry.maximum.y > 1f) Canvas(Modifier.fillMaxSize()) {
        val offset = scroll()
        val inset = 5 * density
        val thickness = 2 * density
        if (geometry.maximum.y > 1f) {
            val track = (size.height - 2 * inset).coerceAtLeast(1f)
            val length = (track * geometry.viewportHeight / geometry.height).coerceAtLeast(24 * density).coerceAtMost(track)
            val top = inset + (track - length) * (offset.y / geometry.maximum.y).coerceIn(0f, 1f)
            drawRoundRect(color.copy(alpha = .45f), Offset(size.width - inset - thickness, top), Size(thickness, length), CornerRadius(thickness))
        }
        if (geometry.maximum.x > 1f) {
            val track = (size.width - 2 * inset).coerceAtLeast(1f)
            val length = (track * geometry.viewportWidth / geometry.width).coerceAtLeast(24 * density).coerceAtMost(track)
            val left = inset + (track - length) * (offset.x / geometry.maximum.x).coerceIn(0f, 1f)
            drawRoundRect(color.copy(alpha = .45f), Offset(left, size.height - inset - thickness), Size(length, thickness), CornerRadius(thickness))
        }
    }
}
