package io.github.tikkaaa3.simpl

import android.content.pm.ActivityInfo
import android.graphics.Rect
import android.view.KeyEvent
import android.view.WindowManager
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.window.layout.FoldingFeature
import androidx.window.layout.WindowLayoutInfo
import androidx.window.testing.layout.WindowLayoutInfoPublisherRule
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.util.UUID
import org.junit.*
import org.junit.Assert.*
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PolishTest {
    @get:Rule(order = 0) val windows = WindowLayoutInfoPublisherRule()
    @get:Rule(order = 1) val ui = createAndroidComposeRule<MainActivity>()
    private val books = mutableListOf<LibraryBook>()
    private lateinit var html: LibraryBook
    @Before fun prepare() {
        ReadingControls.update(ui.activity, volumeTurns = false, keepScreenOn = false)
        val id = UUID.randomUUID().toString()
        html = importLibraryBook(File(ui.activity.cacheDir, "P5-$id.html").apply {
            writeText("<meta charset='utf-8'><title>Polish journey $id</title><h1>Polish journey</h1><p>Café İ é 🛶</p><p>${"A quiet passage beside the harbour. ".repeat(300)}Late beacon</p><p>Another late beacon</p>")
        }.absolutePath)
        books += html
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].apply { query(""); filter("all"); reload(); appearance(Appearance.Light) } }
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("open:${html.fingerprint}").fetchSemanticsNodes().isNotEmpty() }
    }
    @After fun cleanup() {
        if (ui.onAllNodesWithTag("findPanel").fetchSemanticsNodes().isNotEmpty()) ui.onNodeWithText("Close search").performClick()
        if (ui.onAllNodesWithTag("quickSwitch").fetchSemanticsNodes().isNotEmpty()) ui.onNodeWithText("Cancel").performClick()
        if (ui.onAllNodesWithContentDescription("Back to library").fetchSemanticsNodes().isNotEmpty()) ui.onNodeWithContentDescription("Back to library").performClick()
        ui.activityRule.scenario.onActivity {
            it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
            ReadingControls.update(it, volumeTurns = false, keepScreenOn = false)
        }
        // Clear navigation before removing the source, including an open dialog.
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].apply { appearance(Appearance.System) } }
        books.forEach { removeLibraryBook(it.fingerprint) }
    }
    private fun open(book: LibraryBook = html) {
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].open(book) }
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pageLabel").fetchSemanticsNodes().isNotEmpty() }
        ui.waitForIdle()
    }
    private fun key(code: Int, meta: Int = 0) {
        ui.runOnIdle {
            ui.activity.window.callback.dispatchKeyEvent(KeyEvent(0, 0, KeyEvent.ACTION_DOWN, code, 0, meta))
            ui.activity.window.callback.dispatchKeyEvent(KeyEvent(0, 0, KeyEvent.ACTION_UP, code, 0, meta))
        }
        ui.waitForIdle()
    }
    private fun count(text: String) { ui.waitUntil(30_000) { ui.onAllNodesWithTag("findCount").fetchSemanticsNodes().any { node -> node.config[androidx.compose.ui.semantics.SemanticsProperties.Text].any { it.text == text } } } }
    private fun screenshot(name: String) {
        // Include modal windows and finish navigation/sheet animations before capture.
        ui.mainClock.advanceTimeBy(1_000)
        ui.waitForIdle()
        // Device rotation animates outside Compose's test clock. This delay is
        // only for visual artifacts; assertions above wait on the actual layout.
        android.os.SystemClock.sleep(750)
        val bitmap = requireNotNull(androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
        if (android.os.Build.VERSION.SDK_INT >= 29) {
            // Gradle removes installed test apps; MediaStore retains visual QA artifacts.
            val resolver = ui.activity.contentResolver
            val uri = requireNotNull(resolver.insert(android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI, android.content.ContentValues().apply {
                put(android.provider.MediaStore.Images.Media.DISPLAY_NAME, "$name.png")
                put(android.provider.MediaStore.Images.Media.MIME_TYPE, "image/png")
                put(android.provider.MediaStore.Images.Media.RELATIVE_PATH, "Pictures/simPl-P5")
            }))
            resolver.openOutputStream(uri)!!.use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        } else {
            val folder = File(ui.activity.getExternalFilesDir(null), "p5-visual").apply { mkdirs() }
            File(folder, "$name.png").outputStream().use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
    @Test fun searchNavigatesUnicodeAndLongParagraphsAndSurvivesRecreation() {
        open(); key(KeyEvent.KEYCODE_F, KeyEvent.META_CTRL_ON)
        ui.onNodeWithTag("findQuery").performTextInput("CAFÉ"); count("1 of 1 matches")
        ui.onNodeWithTag("findQuery").performTextReplacement("late beacon"); count("1 of 2 matches")
        ui.onNodeWithText("Next match").performClick(); count("2 of 2 matches")
        screenshot("phone-search")
        ui.activityRule.scenario.recreate(); count("2 of 2 matches")
        ui.activityRule.scenario.onActivity { it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
        ui.waitUntil(15_000) { ui.onAllNodesWithTag("readerSidePanel").fetchSemanticsNodes().isNotEmpty() }; count("2 of 2 matches")
        ui.activityRule.scenario.onActivity { it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT }; count("2 of 2 matches")
        ui.onNodeWithTag("findQuery").performTextReplacement("nothing matches"); count("No matches")
        ui.onNodeWithText("Close search").performClick(); ui.onNodeWithTag("findPanel").assertDoesNotExist()
    }
    @Test fun pdfSearchUsesPhysicalPagesAndGlyphSelections() {
        val pdf = importLibraryBook(File(ui.activity.cacheDir, "P5-${UUID.randomUUID()}.pdf").apply { writeBytes(pdfFixture()) }.absolutePath)
        books += pdf; open(pdf)
        ui.readerTool("Find in book")
        ui.onNodeWithTag("findQuery").performTextInput("keeper page 3"); count("1 of 1 matches")
        ui.onNodeWithTag("findHit:0").performClick()
        ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page 3 of 3")).fetchSemanticsNodes().isNotEmpty() }
        screenshot("pdf-search")
        ui.onNodeWithText("Close search").performClick()
        ui.readerTool("Book")
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("paperViewport").fetchSemanticsNodes().isNotEmpty() }
        ui.readerTool("Find in book")
        // Book conversion removes the repeated running heading; search body text.
        ui.onNodeWithTag("findQuery").performTextInput("Passage 2, line 10"); count("1 of 1 matches")
        ui.onNodeWithTag("findHit:0").performClick()
        ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page 2 of 3")).fetchSemanticsNodes().isNotEmpty() }
        screenshot("pdf-book-search")
        ui.onNodeWithText("Close search").performClick()
    }
    @Test fun quickSwitchFiltersBooksAndPersistsThePreviousReaderPosition() {
        val other = importLibraryBook(File(ui.activity.cacheDir, "P5-${UUID.randomUUID()}.txt").apply { writeText("Different harbour book ${UUID.randomUUID()}") }.absolutePath)
        books += other; open()
        ui.runOnIdle { ReadingControls.update(ui.activity, keepScreenOn = true) }
        key(KeyEvent.KEYCODE_DPAD_RIGHT)
        ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page 2 of", substring = true)).fetchSemanticsNodes().isNotEmpty() }
        key(KeyEvent.KEYCODE_K, KeyEvent.META_CTRL_ON)
        ui.onNodeWithTag("quickQuery").performTextInput(other.title)
        screenshot("quick-switch")
        ui.onNodeWithTag("quick:${other.fingerprint}").performClick()
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pageLabel").fetchSemanticsNodes().isNotEmpty() && loadLibrary().books.first { it.fingerprint == html.fingerprint }.current == 2u }
        assertTrue(ui.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON != 0)
        key(KeyEvent.KEYCODE_K, KeyEvent.META_CTRL_ON)
        ui.onNodeWithTag("quickQuery").performTextInput(html.title)
        ui.onNodeWithTag("quick:${html.fingerprint}").performClick()
        ui.waitUntil(30_000) { ui.onAllNodes(readerPageMatcher("Page 2 of", substring = true)).fetchSemanticsNodes().isNotEmpty() }
    }
    @Test fun keysRespectEditingAndVolumeOptInAndScreenFlagEndsWithTheReader() {
        open()
        assertNull(readerCommand(KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_DPAD_RIGHT), true, true))
        assertNull(readerCommand(KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_VOLUME_DOWN), false, false))
        ui.runOnIdle { ReadingControls.update(ui.activity, volumeTurns = true, keepScreenOn = true) }
        ui.waitForIdle()
        assertTrue(ui.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON != 0)
        key(KeyEvent.KEYCODE_VOLUME_DOWN)
        ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page 2 of", substring = true)).fetchSemanticsNodes().isNotEmpty() }
        key(KeyEvent.KEYCODE_L, KeyEvent.META_CTRL_ON)
        ui.onNodeWithTag("jumpPage").performTextReplacement("3")
        key(KeyEvent.KEYCODE_DPAD_LEFT)
        key(KeyEvent.KEYCODE_ESCAPE)
        ui.onNodeWithTag("jumpPage").assertIsNotFocused()
        // Android releases the text input connection after Compose clears focus.
        ui.waitUntil(10_000) {
            !ui.activity.getSystemService(android.view.inputmethod.InputMethodManager::class.java).isAcceptingText
        }
        ui.onNode(readerPageMatcher("Page 2 of", substring = true)).assertExists()
        key(KeyEvent.KEYCODE_W, KeyEvent.META_CTRL_ON)
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("library").fetchSemanticsNodes().isNotEmpty() }
        assertEquals(0, ui.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        ReadingControls.initialize(ui.activity); assertTrue(ReadingControls.state.value.volumeTurns)
    }
    @Test fun tabletPanelsOpenOnDemandAndHingesDoNotCoverTheReader() {
        ui.activityRule.scenario.onActivity { it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
        ui.waitUntil(15_000) { ui.onAllNodesWithTag("librarySidePanel").fetchSemanticsNodes().isNotEmpty() }
        screenshot("tablet-library")
        open(); ui.onNodeWithTag("readerSidePanel").assertDoesNotExist()
        ui.readerTool("Annotations"); ui.onNodeWithTag("readerSidePanel").assertExists()
        screenshot("tablet-reader")
        val width = ui.activity.window.decorView.width; val height = ui.activity.window.decorView.height
        val hinge = object : FoldingFeature {
            override val bounds = Rect(width / 2 - 12, 0, width / 2 + 12, height)
            override val state = FoldingFeature.State.HALF_OPENED
            override val orientation = FoldingFeature.Orientation.VERTICAL
            override val occlusionType = FoldingFeature.OcclusionType.FULL
            override val isSeparating = true
        }
        windows.overrideWindowLayoutInfo(WindowLayoutInfo(listOf(hinge)))
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("annotationsPanel").fetchSemanticsNodes().isNotEmpty() }
        val reader = ui.onNodeWithTag("reader", useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        val panel = ui.onNodeWithTag("annotationsPanel").fetchSemanticsNode().boundsInRoot
        assertTrue(reader.right <= width / 2 - 12 + 1)
        assertTrue(panel.left >= width / 2 + 12 - 1)
        screenshot("vertical-hinge")
        windows.overrideWindowLayoutInfo(WindowLayoutInfo(emptyList()))
    }
    @Test fun tabletopAndUnfoldingKeepTheCanonicalPage() {
        open(); key(KeyEvent.KEYCODE_DPAD_RIGHT)
        ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page 2 of", substring = true)).fetchSemanticsNodes().isNotEmpty() }
        key(KeyEvent.KEYCODE_B, KeyEvent.META_CTRL_ON)
        val width = ui.activity.window.decorView.width; val height = ui.activity.window.decorView.height
        val hinge = object : FoldingFeature {
            override val bounds = Rect(0, height / 2 - 12, width, height / 2 + 12)
            override val state = FoldingFeature.State.HALF_OPENED
            override val orientation = FoldingFeature.Orientation.HORIZONTAL
            override val occlusionType = FoldingFeature.OcclusionType.FULL
            override val isSeparating = true
        }
        windows.overrideWindowLayoutInfo(WindowLayoutInfo(listOf(hinge)))
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("annotationsPanel").fetchSemanticsNodes().isNotEmpty() }
        val reader = ui.onNodeWithTag("reader", useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        val panel = ui.onNodeWithTag("annotationsPanel").fetchSemanticsNode().boundsInRoot
        assertTrue(reader.bottom <= height / 2 - 12 + 1); assertTrue(panel.top >= height / 2 + 12 - 1)
        ui.onNode(readerPageMatcher("Page 2 of", substring = true)).assertExists(); screenshot("tabletop")
        windows.overrideWindowLayoutInfo(WindowLayoutInfo(emptyList()))
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("adaptivePanes").fetchSemanticsNodes().isEmpty() }
        ui.onNode(readerPageMatcher("Page 2 of", substring = true)).assertExists()
    }
    @Test fun desktopKeysNavigateChaptersAndKeepZoomAndAnnotations() {
        ui.activityRule.scenario.onActivity { it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT }
        val epub = importLibraryBook(File(ui.activity.cacheDir, "P5-keys.epub").apply { writeBytes(readerEpub(UUID.randomUUID().toString())) }.absolutePath)
        books += epub; open(epub)
        key(KeyEvent.KEYCODE_PAGE_DOWN, KeyEvent.META_CTRL_ON)
        ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page 42 · 2 of 2")).fetchSemanticsNodes().isNotEmpty() }
        key(KeyEvent.KEYCODE_A, KeyEvent.META_CTRL_ON)
        ui.onNodeWithTag("readerSelection").assertExists()
        key(KeyEvent.KEYCODE_H, KeyEvent.META_CTRL_ON)
        ui.waitUntil(10_000) { loadAnnotations(epub.fingerprint).highlights.isNotEmpty() }
        key(KeyEvent.KEYCODE_D, KeyEvent.META_CTRL_ON)
        ui.waitUntil(10_000) { loadAnnotations(epub.fingerprint).bookmarks.size == 1 }
        key(KeyEvent.KEYCODE_PLUS, KeyEvent.META_CTRL_ON)
        ui.onNodeWithTag("paperViewport").assert(androidx.compose.ui.test.SemanticsMatcher.expectValue(androidx.compose.ui.semantics.SemanticsProperties.StateDescription, "Zoom 1.1; page 2"))
        key(KeyEvent.KEYCODE_F, KeyEvent.META_CTRL_ON or KeyEvent.META_SHIFT_ON)
        ui.onNodeWithTag("paperViewport").assert(androidx.compose.ui.test.SemanticsMatcher.expectValue(androidx.compose.ui.semantics.SemanticsProperties.StateDescription, "Zoom 1.0; page 2"))
        key(KeyEvent.KEYCODE_F, KeyEvent.META_CTRL_ON or KeyEvent.META_SHIFT_ON)
        ui.onNodeWithTag("paperViewport").assert(androidx.compose.ui.test.SemanticsMatcher.expectValue(androidx.compose.ui.semantics.SemanticsProperties.StateDescription, "Zoom 1.1; page 2"))
        key(KeyEvent.KEYCODE_PAGE_UP, KeyEvent.META_CTRL_ON)
        ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page iv · 1 of 2")).fetchSemanticsNodes().isNotEmpty() }
        key(KeyEvent.KEYCODE_B, KeyEvent.META_CTRL_ON)
        ui.onNodeWithTag("annotationsPanel").assertExists()
        ui.activityRule.scenario.onActivity { it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
        ui.waitUntil(15_000) { ui.onAllNodesWithTag("readerSidePanel").fetchSemanticsNodes().isNotEmpty() }
        key(KeyEvent.KEYCODE_B, KeyEvent.META_CTRL_ON); ui.onNodeWithTag("readerSidePanel").assertDoesNotExist()
        key(KeyEvent.KEYCODE_B, KeyEvent.META_CTRL_ON); ui.onNodeWithTag("readerSidePanel").assertExists()
    }
}
