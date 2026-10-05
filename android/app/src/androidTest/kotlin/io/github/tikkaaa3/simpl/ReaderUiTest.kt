package io.github.tikkaaa3.simpl

import android.graphics.Bitmap
import android.content.ContentValues
import android.os.Build
import android.provider.MediaStore
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ViewModelProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.util.UUID
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ReaderUiTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()
    private lateinit var entry: LibraryBook
    private var native: OpenBook? = null
    private var total = 0u
    private var baseline = emptySet<String>()

    @Before fun prepare() {
        baseline = loadLibrary().books.map { it.fingerprint }.toSet()
        readerIllustration(ui.activity.cacheDir)
        val file = File(ui.activity.cacheDir, "M4-reader.html").apply { writeText(readerHtml(UUID.randomUUID().toString())) }
        entry = importLibraryBook(file.absolutePath)
        openBook(entry.path).use { task -> waitLayout(task::status); native = task.result() }
        total = native!!.readerInfo().total
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].apply { query(""); filter("all"); reload(); appearance(Appearance.Light) } }
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("open:${entry.fingerprint}").fetchSemanticsNodes().isNotEmpty() }
    }
    @After fun cleanup() {
        native?.close()
        // Dispose the reader before removing its managed source.
        if (ui.onAllNodesWithContentDescription("Back to library").fetchSemanticsNodes().isNotEmpty()) ui.onNodeWithContentDescription("Back to library").performClick()
        if (entry.fingerprint !in baseline && loadLibrary().books.any { it.fingerprint == entry.fingerprint }) removeLibraryBook(entry.fingerprint)
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].appearance(Appearance.System) }
        ui.activity.getSharedPreferences("reader", 0).edit().clear().commit()
    }
    private fun open() { ui.onNodeWithTag("open:${entry.fingerprint}").performClick(); page(1u) }
    private fun page(number: UInt) {
        try {
            ui.waitUntil(30_000) { ui.onAllNodes(readerPageMatcher("Page ${native!!.page(number).first().layout.label} of $total")).fetchSemanticsNodes().isNotEmpty() }
        } catch (error: ComposeTimeoutException) {
            ui.onRoot(useUnmergedTree = true).printToLog("ReaderNavigation")
            throw error
        }
        ui.waitForIdle()
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("layoutProgress").fetchSemanticsNodes().isEmpty() }
    }
    private fun jump(value: String) {
        ui.readerJump(value)
    }
    private fun follow(label: String) {
        val node = ui.onNode(hasText(label, substring = true) and SemanticsMatcher.keyIsDefined(SemanticsActions.CustomActions), useUnmergedTree = true)
        val actions = node.fetchSemanticsNode().config[SemanticsActions.CustomActions]
        ui.runOnIdle { check(actions.first { it.label == "Follow $label" }.action()) }
    }

    @Test fun turnsJumpScrollZoomImmersiveAndStopRestoreTheSameCanonicalPage() {
        open(); screenshot("default-light")
        ui.onNodeWithContentDescription("Next").performClick(); page(2u)
        ui.onNodeWithTag("paperViewport").performTouchInput { swipeLeft() }; page(3u)
        ui.onNodeWithTag("paperViewport").performTouchInput { click(Offset(width * 0.05f, height * 0.2f)) }; page(2u)
        jump("4"); page(4u)
        ui.onNodeWithTag("paperViewport").performTouchInput { pinch(Offset(width * .4f, height * .4f), Offset(width * .6f, height * .6f), Offset(width * .15f, height * .2f), Offset(width * .85f, height * .8f)) }
        ui.onNodeWithTag("paperViewport").assert(SemanticsMatcher("zoom increased") { it.config[SemanticsProperties.StateDescription].substringAfter("Zoom ").substringBefore(';').toFloat() > 1.2f })
        page(4u)
        ui.onNodeWithTag("paperViewport").performTouchInput { swipeUp() }
        ui.waitForIdle()
        ui.activityRule.scenario.moveToState(Lifecycle.State.CREATED)
        val stored = native!!.readerInfo().restored!!
        assertEquals(4u, stored.page); assertTrue(stored.within > 0f)
        ui.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
        ui.readerTool("Fit width")
        ui.onNodeWithTag("paperViewport").assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "Zoom 1.0; page 4"))
        ui.onNodeWithTag("paperViewport").performTouchInput { click(center) }
        // A text tap first excludes a dictionary double tap.
        ui.waitUntil(10_000) { ui.onAllNodesWithContentDescription("Next").fetchSemanticsNodes().isEmpty() }
        ui.onNodeWithContentDescription("Next").assertDoesNotExist()
        screenshot("immersive")
        ui.onNodeWithTag("paperViewport").performTouchInput { click(center) }
        page(4u)
        ui.activityRule.scenario.recreate(); page(4u)
        ui.onNodeWithContentDescription("Back to library").performClick()
        ui.waitUntil(10_000) { loadLibrary().books.first { it.fingerprint == entry.fingerprint }.current == 4u }
        ui.onNodeWithTag("open:${entry.fingerprint}").performClick(); page(4u)
    }

    @Test fun contentsLinksThemesAndPerBookTypographyKeepPageIdentity() {
        open()
        // A real glyph tap exercises caret hit-testing; the note test also
        // checks the accessible link actions after multibyte source text.
        ui.onNode(hasText("a later passage", substring = true) and SemanticsMatcher.keyIsDefined(SemanticsActions.CustomActions), useUnmergedTree = true)
            .performTouchInput { click(Offset(8f, 12f)) }
        val far = native!!.contents().first { it.label == "A later passage" }.location.page
        page(far)
        screenshot("structure-rtl")
        ui.onNodeWithContentDescription("Return from link").performClick(); page(1u)
        ui.onNodeWithContentDescription("Contents").performClick(); ui.onNodeWithText("A later passage").performClick(); page(far)
        ui.readerTool("Reading options")
        for (theme in listOf("Soft", "Clear", "Compact", "Default")) {
            // Theme cards scroll sideways on a phone.
            ui.onNodeWithTag("themePicker").performScrollToNode(hasText(theme))
            ui.onNodeWithText(theme).performClick()
            ui.waitUntil(30_000) { ui.onAllNodesWithTag("layoutProgress").fetchSemanticsNodes().isEmpty() }
            ui.onNodeWithText("Light paper").performClick()
            dismissSheet(); screenshot("${theme.lowercase()}-light-structure")
            ui.readerTool("Reading options"); ui.onNodeWithText("Dark paper").performClick()
            dismissSheet(); screenshot("${theme.lowercase()}-dark-structure")
            ui.readerTool("Reading options")
        }
        ui.onNodeWithContentDescription("Increase font size").performClick()
        ui.waitUntil(10_000) { native!!.readerInfo().options.size == 22.toUShort() }
        ui.onNodeWithText("Dark paper").performClick()
        // Dismiss the sheet with the system back button; the reader remains open.
        dismissSheet()
        page(far); screenshot("compact-dark")
        ui.onNodeWithContentDescription("Back to library").performClick()
        ui.onNodeWithTag("open:${entry.fingerprint}").performClick(); page(far)
        assertEquals(22.toUShort(), native!!.readerInfo().options.size)
        ui.readerTool("Reading options"); ui.onNodeWithText("Reset typography").performClick()
        ui.waitUntil(10_000) { native!!.readerInfo().options.size == 20.toUShort() }
        dismissSheet()
    }

    @Test fun auxiliaryFootnotesPrintedLabelsAndCrossChapterReturn() {
        val file = File(ui.activity.cacheDir, "M4-source.epub").apply { writeBytes(readerEpub()) }
        val source = importLibraryBook(file.absolutePath)
        try {
            ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].open(source) }
            ui.waitUntil(30_000) { ui.onAllNodes(readerPageMatcher("Page iv · 1 of 2")).fetchSemanticsNodes().isNotEmpty() }
            follow("the harbour note")
            ui.onNodeWithText("Note").assertExists()
            ui.onNode(hasText("A supplementary note outside", substring = true), useUnmergedTree = true).assertExists()
            ui.onNode(hasText("This second note is separate", substring = true), useUnmergedTree = true).assertDoesNotExist()
            screenshot("auxiliary-note")
            ui.onNodeWithText("Return to reading").performClick()
            follow("Follow the dawn")
            ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page 42 · 2 of 2")).fetchSemanticsNodes().isNotEmpty() }
            ui.onNodeWithContentDescription("Return from link").performClick()
            ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page iv · 1 of 2")).fetchSemanticsNodes().isNotEmpty() }
            jump("missing-label")
            ui.waitUntil(10_000) { ui.onAllNodesWithText("Enter a page number or a printed page label from this book").fetchSemanticsNodes().isNotEmpty() }
            ui.onNodeWithText("OK").performClick(); cancelReaderJump()
            ui.onNode(readerPageMatcher("Page iv · 1 of 2")).assertExists()
            jump("42")
            ui.waitUntil(10_000) { ui.onAllNodes(readerPageMatcher("Page 42 · 2 of 2")).fetchSemanticsNodes().isNotEmpty() }
            ui.onNodeWithContentDescription("Back to library").performClick()
        } finally { if (source.fingerprint !in baseline) removeLibraryBook(source.fingerprint) }
    }

    private fun screenshot(name: String) {
        ui.waitForIdle()
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("layoutProgress").fetchSemanticsNodes().isEmpty() }
        val bitmap = ui.onRoot().captureToImage().asAndroidBitmap()
        if (Build.VERSION.SDK_INT >= 29) {
            // Gradle uninstalls test APKs; MediaStore retains visual QA artifacts.
            val resolver = ui.activity.contentResolver
            val uri = requireNotNull(resolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, ContentValues().apply {
                put(MediaStore.Images.Media.DISPLAY_NAME, "$name.png"); put(MediaStore.Images.Media.MIME_TYPE, "image/png")
                put(MediaStore.Images.Media.RELATIVE_PATH, "Pictures/simPl-M4")
            }))
            resolver.openOutputStream(uri)!!.use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        } else {
            val directory = File(ui.activity.getExternalFilesDir(null), "m4-ui").apply { mkdirs() }
            File(directory, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
    private fun dismissSheet() {
        androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK)
        ui.waitForIdle()
    }
}
