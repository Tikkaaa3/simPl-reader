package io.github.tikkaaa3.simpl

import android.content.pm.ActivityInfo
import android.view.KeyEvent
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.tikkaaa3.simpl.core.*
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID
import org.junit.*
import org.junit.Assert.*
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class MobileReaderTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()
    private lateinit var book: LibraryBook
    private val imported = mutableListOf<String>()

    @Before fun prepare() {
        ReadAloud.stop()
        ui.activityRule.scenario.onActivity { it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT }
        readerIllustration(ui.activity.cacheDir)
        book = importLibraryBook(File(ui.activity.cacheDir, "mobile-${UUID.randomUUID()}.html").apply {
            writeText(readerHtml("Mobile reader ${UUID.randomUUID()}"))
        }.absolutePath).also { imported += it.fingerprint }
        open(book)
    }
    @After fun cleanup() {
        ui.activityRule.scenario.onActivity { it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT }
        if (ui.onAllNodesWithTag("readerTools").fetchSemanticsNodes().isNotEmpty()) back()
        if (ui.onAllNodesWithContentDescription("Back to library").fetchSemanticsNodes().isNotEmpty())
            ui.onNodeWithContentDescription("Back to library").performClick()
        imported.forEach { if (loadLibrary().books.any { book -> book.fingerprint == it }) removeLibraryBook(it) }
    }
    private fun open(entry: LibraryBook) {
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].open(entry) }
        ui.waitUntil(30_000) { ui.onAllNodesWithContentDescription("Reader tools").fetchSemanticsNodes().any {
            !it.config.contains(SemanticsProperties.Disabled)
        } }
        ui.waitForIdle()
    }
    private fun bounds(tag: String): Rect {
        val node = ui.onNodeWithTag(tag).fetchSemanticsNode()
        return Rect(node.positionInRoot, Size(node.size.width.toFloat(), node.size.height.toFloat()))
    }
    private fun fits() {
        ui.waitForIdle()
        val page = bounds("bookPage"); val viewport = bounds("paperViewport")
        assertTrue("Full page $page must fit viewport $viewport", page.left >= viewport.left - 2 && page.top >= viewport.top - 2 &&
            page.right <= viewport.right + 2 && page.bottom <= viewport.bottom + 2)
        ui.onNodeWithTag("pageScroll").assert(SemanticsMatcher("no hidden vertical content") {
            it.config.getOrElse(SemanticsProperties.VerticalScrollAxisRange) { error("Missing scroll range") }.maxValue() <= 1f
        })
    }
    private fun key(code: Int, meta: Int = 0) {
        ui.runOnIdle {
            ui.activity.window.callback.dispatchKeyEvent(KeyEvent(0, 0, KeyEvent.ACTION_DOWN, code, 0, meta))
            ui.activity.window.callback.dispatchKeyEvent(KeyEvent(0, 0, KeyEvent.ACTION_UP, code, 0, meta))
        }
        ui.waitForIdle()
    }
    private fun back() { InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(KeyEvent.KEYCODE_BACK); ui.waitForIdle() }
    private fun screenshot(name: String) {
        ui.mainClock.advanceTimeBy(1_000)
        ui.waitForIdle()
        android.os.SystemClock.sleep(750)
        val bytes = ByteArrayOutputStream()
        requireNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
            .compress(android.graphics.Bitmap.CompressFormat.PNG, 100, bytes)
        saveP34Artifact(ui.activity, "mobile-$name.png", "image/png", bytes.toByteArray())
    }

    @Test fun oneSmallBottomToolbarLeavesTheWholePageVisibleAndFullscreenNeedsNoScroll() {
        fits()
        val toolbar = bounds("readerToolbar"); val viewport = bounds("paperViewport")
        assertTrue("One 56 dp toolbar: $toolbar", toolbar.height <= 56 * ui.activity.resources.displayMetrics.density + 2)
        assertTrue("Reading area should dominate the screen", viewport.height > toolbar.height * 5)
        ui.onNodeWithContentDescription("Contents").assertIsDisplayed()
        screenshot("portrait")
        ui.openReaderTools(); screenshot("tools"); back()
        ui.onNodeWithTag("paperViewport").performTouchInput { click(center) }
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("readerToolbar").fetchSemanticsNodes().isEmpty() }
        fits(); screenshot("fullscreen")
        ui.onNodeWithTag("paperViewport").performTouchInput { click(center) }
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("readerToolbar").fetchSemanticsNodes().isNotEmpty() }
    }

    @Test fun bottomPageInputJumpsDirectlyAndInvalidInputCanBeCorrected() {
        ui.onNodeWithTag("jumpPage").performClick().performTextReplacement("3")
        ui.onNodeWithContentDescription("Go").assertIsDisplayed().performClick()
        ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page 3 of", true)).fetchSemanticsNodes().isNotEmpty() }
        fits()
        ui.readerJump("999999")
        ui.waitUntil(10_000) { ui.onAllNodesWithText("Enter a page number or a printed page label from this book").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithText("OK").performClick()
        ui.readerJump("2")
        ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page 2 of", true)).fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithText("Go to page").assertDoesNotExist()
    }

    @Test fun zoomButtonsAndPinchKeepTheReadingPointInsteadOfTheTopLeftCorner() {
        val viewport = bounds("paperViewport")
        repeat(8) { key(KeyEvent.KEYCODE_PLUS, KeyEvent.META_CTRL_ON) }
        val page = bounds("bookPage")
        assertTrue(page.width > viewport.width)
        assertEquals(.5f, (viewport.center.x - page.left) / page.width, .02f)
        assertEquals(.5f, (viewport.center.y - page.top) / page.height, .02f)
        key(KeyEvent.KEYCODE_MINUS, KeyEvent.META_CTRL_ON)
        val reduced = bounds("bookPage")
        assertEquals(.5f, (viewport.center.x - reduced.left) / reduced.width, .02f)
        assertEquals(.5f, (viewport.center.y - reduced.top) / reduced.height, .02f)
        ui.openReaderTools()
        ui.onNodeWithContentDescription("Zoom in").performClick()
        ui.onNodeWithContentDescription("Zoom out").performClick()
        back()
        val buttons = bounds("bookPage")
        assertEquals(.5f, (viewport.center.x - buttons.left) / buttons.width, .02f)
        assertEquals(.5f, (viewport.center.y - buttons.top) / buttons.height, .02f)
        screenshot("centered-zoom")
        ui.onNodeWithTag("pageScroll").performTouchInput { swipe(center, center - Offset(100f, 140f), 600) }
        val panned = bounds("bookPage")
        val readingPoint = Offset((viewport.center.x - panned.left) / panned.width, (viewport.center.y - panned.top) / panned.height)
        key(KeyEvent.KEYCODE_PLUS, KeyEvent.META_CTRL_ON)
        val enlarged = bounds("bookPage")
        assertEquals(readingPoint.x, (viewport.center.x - enlarged.left) / enlarged.width, .02f)
        assertEquals(readingPoint.y, (viewport.center.y - enlarged.top) / enlarged.height, .02f)
        ui.readerTool("Fit page"); fits()
        val before = bounds("bookPage"); val currentViewport = bounds("paperViewport")
        val focus = Offset(currentViewport.width * .43f, currentViewport.height * .44f)
        val screenFocus = focus + currentViewport.topLeft
        val source = Offset((screenFocus.x - before.left) / before.width, (screenFocus.y - before.top) / before.height)
        ui.onNodeWithTag("paperViewport").performTouchInput {
            pinch(focus - Offset(35f, 35f), focus + Offset(35f, 35f), focus - Offset(90f, 90f), focus + Offset(90f, 90f))
        }
        ui.waitForIdle()
        val after = bounds("bookPage")
        assertEquals(source.x, (screenFocus.x - after.left) / after.width, .04f)
        assertEquals(source.y, (screenFocus.y - after.top) / after.height, .04f)
    }

    @Test fun pageFitTracksLandscapeAndTypographyAndToolsRemainReachable() {
        ui.readerTool("Reading options")
        repeat(4) { ui.onNodeWithContentDescription("Increase font size").performClick() }
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("layoutProgress").fetchSemanticsNodes().isEmpty() }
        back(); fits(); screenshot("larger-type")
        ui.activityRule.scenario.onActivity { it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
        ui.waitUntil(15_000) { bounds("paperViewport").width > bounds("paperViewport").height }
        ui.onNodeWithTag("annotationList").assertDoesNotExist()
        fits(); screenshot("landscape")
        // In landscape the fitted page is narrow; its surrounding space still
        // uses the new viewport's tap zones after rotation.
        ui.onNodeWithTag("paperViewport").performTouchInput { click(Offset(width * .7f, height * .5f)) }
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("readerToolbar").fetchSemanticsNodes().isEmpty() }
        fits()
        ui.onNodeWithTag("paperViewport").performTouchInput { click(Offset(width * .7f, height * .5f)) }
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("readerToolbar").fetchSemanticsNodes().isNotEmpty() }
        ui.openReaderTools()
        for (label in listOf("Contents", "Find in book", "Annotations", "Reading options", "Bookmark page", "Switch book"))
            ui.onNode(hasText(label) and hasClickAction()).assertExists()
        back()
    }

    @Test fun pdfUsesTheSameCompactControlsAndZoomsAroundItsCenter() {
        val pdf = importLibraryBook(File(ui.activity.cacheDir, "mobile-${UUID.randomUUID()}.pdf").apply { writeBytes(pdfFixture()) }.absolutePath)
        imported += pdf.fingerprint; open(pdf)
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pdfRendering").fetchSemanticsNodes().isEmpty() }
        val viewport = bounds("pdfViewport")
        repeat(6) { key(KeyEvent.KEYCODE_PLUS, KeyEvent.META_CTRL_ON) }
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pdfRendering").fetchSemanticsNodes().isEmpty() }
        val page = bounds("pdfPage")
        assertTrue("PDF zoom must enlarge the page", page.width > viewport.width * 1.5f)
        assertEquals(.5f, (viewport.center.x - page.left) / page.width, .025f)
        assertEquals(.5f, (viewport.center.y - page.top) / page.height, .025f)
        ui.readerJump("2")
        ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page 2 of 3")).fetchSemanticsNodes().isNotEmpty() }
        screenshot("pdf")
        ui.readerTool("Book")
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("bookPage").fetchSemanticsNodes().isNotEmpty() }
        fits(); screenshot("pdf-book")
    }

    @Test fun narrowPhonesKeepLargeTouchTargetsAndAnEditablePageNumber() {
        val density = ui.activity.resources.displayMetrics.density
        ui.activityRule.scenario.onActivity { it.setContent {
            CompositionLocalProvider(LocalDensity provides Density(density, 1.6f)) {
                SimplTheme(Appearance.Light) {
                    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.BottomCenter) {
                        Box(Modifier.width(320.dp)) { ReaderBottomBar(12u, 256u, label = "iv", numeric = false, enabled = true,
                            color = MaterialTheme.colorScheme.surface, text = MaterialTheme.colorScheme.onSurface,
                            back = {}, previous = {}, next = {}, jump = { _, done -> done() }, contents = {}, tools = {}) }
                    }
                }
            }
        } }
        ui.waitForIdle()
        for (label in listOf("Back to library", "Previous", "Next", "Reader tools")) {
            val target = ui.onNodeWithContentDescription(label).fetchSemanticsNode().boundsInRoot
            assertTrue("$label touch target $target", target.width >= 48 * density - 1 && target.height >= 48 * density - 1)
        }
        val field = bounds("jumpPage")
        assertTrue("Editable page target must remain wide enough: $field", field.width >= 48 * density - 1)
        ui.onNodeWithTag("jumpPage").assertTextEquals("12")
        ui.onNode(readerPageMatcher("Page iv · 12 of 256")).assertExists()
        ui.onNodeWithTag("jumpPage").performClick().performTextReplacement("15")
        ui.onNodeWithTag("jumpPage").performImeAction()
        screenshot("narrow-large-font")
    }
}
