package io.github.tikkaaa3.simpl

import androidx.compose.ui.geometry.Offset
import org.junit.Assert.*
import org.junit.Test

class PageViewportTest {
    @Test fun wholePagesFitPortraitLandscapeAndTallContentWithoutHiddenOverflow() {
        for ((width, height) in listOf(1080f to 2100f, 2100f to 920f, 720f to 1280f)) {
            for (pageHeight in listOf(1018f, 3200f)) {
                val page = PageGeometry.fitted(width, height, 720f, pageHeight, 1f, false)
                assertTrue(page.width <= width + .01f)
                assertTrue(page.height <= height + .01f)
                assertTrue(page.maximum.x < .01f && page.maximum.y < .01f)
                assertEquals(width / 2, page.screen(Offset(360f, pageHeight / 2), Offset.Zero).x, .01f)
                assertEquals(height / 2, page.screen(Offset(360f, pageHeight / 2), Offset.Zero).y, .01f)
            }
        }
    }

    @Test fun zoomPreservesTheCenterAndAnOffCenterPinchInBothAxes() {
        val before = PageGeometry.fitted(1080f, 2100f, 720f, 1018f, 1f, false)
        val after = PageGeometry.fitted(1080f, 2100f, 720f, 1018f, 2f, false)
        for (focus in listOf(before.center, Offset(440f, 900f))) {
            val anchor = before.anchor(Offset.Zero, focus)
            val shown = after.screen(anchor.source, anchor.scroll(after))
            assertEquals(focus.x, shown.x, .01f)
            assertEquals(focus.y, shown.y, .01f)
        }
        val moved = PageAnchor(before.source(Offset(440f, 900f), Offset.Zero), Offset(480f, 940f))
        val shown = after.screen(moved.source, moved.scroll(after))
        assertEquals(moved.focus.x, shown.x, .01f)
        assertEquals(moved.focus.y, shown.y, .01f)
    }

    @Test fun zoomingOutRetainsTheReadingPointAndClampsAtThePageEdges() {
        val before = PageGeometry.fitted(1080f, 2100f, 720f, 1018f, 2f, false)
        val after = PageGeometry.fitted(1080f, 2100f, 720f, 1018f, 1.5f, false)
        val anchor = before.anchor(Offset(480f, 400f))
        val point = after.screen(anchor.source, anchor.scroll(after))
        assertEquals(after.center.x, point.x, .01f)
        assertEquals(after.center.y, point.y, .01f)
        val fitted = PageGeometry.fitted(1080f, 2100f, 720f, 1018f, 1f, false)
        assertEquals(Offset.Zero, anchor.scroll(fitted))
        assertTrue(PageGeometry.fitted(1080f, 0f, 720f, 1018f, 1f, false).scale > 0f)
    }
}
