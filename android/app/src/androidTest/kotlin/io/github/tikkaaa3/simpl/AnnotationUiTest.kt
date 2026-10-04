package io.github.tikkaaa3.simpl

import android.content.ClipboardManager
import android.content.Context
import android.content.ContentValues
import android.content.Intent
import android.app.Instrumentation
import android.provider.MediaStore
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.util.UUID
import org.junit.After
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AnnotationUiTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()
    private val imported = mutableListOf<String>()
    private lateinit var book: LibraryBook

    @After fun cleanup() {
        if (ui.onAllNodesWithContentDescription("Back to library").fetchSemanticsNodes().isNotEmpty()) ui.onNodeWithContentDescription("Back to library").performClick()
        imported.forEach { fingerprint -> if (loadLibrary().books.any { it.fingerprint == fingerprint }) removeLibraryBook(fingerprint) }
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].appearance(Appearance.System) }
        ui.activity.getSharedPreferences("reader", 0).edit().clear().commit()
    }
    private fun open(pdf: Boolean = false) {
        val id = UUID.randomUUID().toString()
        val file = File(ui.activity.cacheDir, "M6-$id.${if (pdf) "pdf" else "html"}")
        if (pdf) file.writeBytes(pdfFixture(lines = 6, identity = id)) else file.writeText("<html><head><title>M6 $id</title></head><body><p>Harbour lighthouse keeper watches the boats. Café é 👩‍👩‍👧‍👦 العربية עברית.</p><p>${(0..300).joinToString(" ") { "Passage$it brings the tide home." }}</p></body></html>")
        book = importLibraryBook(file.absolutePath); imported += book.fingerprint
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].apply { query(""); filter("all"); reload(); appearance(Appearance.Light) } }
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("open:${book.fingerprint}").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithTag("open:${book.fingerprint}").performClick()
        ui.waitUntil(30_000) { ui.onAllNodesWithText("Next").fetchSemanticsNodes().isNotEmpty() }
        if (pdf) ui.waitUntil(30_000) { ui.onAllNodesWithText("Select page text").fetchSemanticsNodes().any { !it.config.contains(androidx.compose.ui.semantics.SemanticsProperties.Disabled) } }
        else ui.waitUntil(30_000) { ui.onAllNodesWithTag("row:0:0", useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        ui.waitForIdle()
    }
    private fun waitCount(highlights: Int, bookmarks: Int = 0) = ui.waitUntil(10_000) {
        loadAnnotations(book.fingerprint).let { it.highlights.size == highlights && it.bookmarks.size == bookmarks }
    }
    private fun selectReaderWord() {
        ui.onNodeWithTag("row:0:0", useUnmergedTree = true).performTouchInput { longClick(Offset(24f, 12f)) }
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("readerSelection").fetchSemanticsNodes().isNotEmpty() }
    }
    private fun dismissSheet() { ui.activityRule.scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }; ui.waitForIdle() }
    private fun screenshot(name: String) {
        if (android.os.Build.VERSION.SDK_INT < 29) {
            File(ui.activity.getExternalFilesDir(null), "$name.png").outputStream().use { ui.onRoot().captureToImage().asAndroidBitmap().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
            return
        }
        val uri = requireNotNull(ui.activity.contentResolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, "$name.png"); put(MediaStore.MediaColumns.MIME_TYPE, "image/png")
            put(MediaStore.MediaColumns.RELATIVE_PATH, "Pictures/simPl-M6")
        }))
        ui.activity.contentResolver.openOutputStream(uri)!!.use { ui.onRoot().captureToImage().asAndroidBitmap().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
    }

    @Test fun reflowHandlesNotesBookmarksEditDeleteAndRecreationPersist() {
        open(); selectReaderWord()
        ui.onNodeWithTag("selectionEnd", useUnmergedTree = true).performTouchInput { swipe(center, center + Offset(170f, 0f), 600) }
        ui.onNodeWithText("Copy").performClick()
        ui.waitUntil(10_000) { ui.onAllNodesWithText("Copied").fetchSemanticsNodes().isNotEmpty() }
        ui.runOnIdle {
            val copied = (ui.activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).primaryClip!!.getItemAt(0).text.toString()
            assertTrue(copied, copied.startsWith("Harbour")); assertTrue(copied, copied.contains("lighthouse")); assertFalse(copied, copied.contains("Café"))
        }
        val shared = java.util.concurrent.atomic.AtomicReference<Intent>()
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val monitor = object : Instrumentation.ActivityMonitor() {
            override fun onStartActivity(intent: Intent): Instrumentation.ActivityResult? {
                if (intent.action != Intent.ACTION_CHOOSER) return null
                shared.set(intent); return Instrumentation.ActivityResult(0, null)
            }
        }
        instrumentation.addMonitor(monitor)
        try {
            ui.onNodeWithText("Share").performClick()
            ui.waitUntil(10_000) { shared.get() != null }
            val sent = androidx.core.content.IntentCompat.getParcelableExtra(shared.get(), Intent.EXTRA_INTENT, Intent::class.java)!!
            assertEquals("text/plain", sent.type); assertTrue(sent.getStringExtra(Intent.EXTRA_TEXT)!!.startsWith("Harbour"))
        } finally { instrumentation.removeMonitor(monitor) }
        ui.onNodeWithText("Add note").performClick(); ui.onNodeWithTag("annotationNote").performTextInput("The first harbour note"); ui.onNodeWithText("Save").performClick()
        waitCount(1)
        ui.onNodeWithContentDescription("Bookmark page").performClick(); waitCount(1, 1)
        screenshot("reflow-highlight")
        ui.activityRule.scenario.recreate()
        ui.waitUntil(30_000) { ui.onAllNodesWithContentDescription("Annotations").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithContentDescription("Annotations").performClick(); ui.onNodeWithText("Notes", substring = false).performClick()
        ui.onNodeWithText("The first harbour note").assertExists(); screenshot("notes-sheet")
        ui.onNodeWithText("Edit").performClick(); ui.onNodeWithTag("annotationNote").performTextReplacement("Edited harbour note")
        ui.onNodeWithContentDescription("Highlight blue").performClick(); ui.onNodeWithText("Save").performClick()
        ui.waitUntil(10_000) { loadAnnotations(book.fingerprint).highlights.single().note == "Edited harbour note" }
        assertEquals(AnnotationColor.BLUE, loadAnnotations(book.fingerprint).highlights.single().color)
        // The editor's keyboard can still animate the sheet after the native save.
        // Use the accessible tab action, then wait for the list's visible action.
        ui.onNodeWithText("Highlights", substring = false)
            .performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.OnClick) { it() }
        ui.onNodeWithTag("annotationList").performScrollToNode(hasText("Delete", substring = false))
        ui.onNodeWithText("Delete", substring = false).performClick()
        ui.onNodeWithTag("annotationDeleteConfirm").performClick(); waitCount(0, 1)
        ui.onNodeWithText("Bookmarks", substring = false).performClick(); ui.onNodeWithText("Page 1", substring = false).performClick()
        ui.onNodeWithTag("annotationList").assertDoesNotExist()
        ui.onNodeWithContentDescription("Bookmark page").performClick(); waitCount(0)
    }

    @Test fun reflowSelectionSurvivesPageEdgesAndUsesSourceBytes() {
        open(); selectReaderWord()
        ui.onNodeWithText("Next").performClick()
        ui.waitUntil(30_000) { ui.onAllNodesWithText("Page 2", substring = true).fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithTag("paperViewport").performTouchInput { click(Offset(width * .3f, height * .15f)) }
        ui.onNodeWithText("Copy").performClick()
        ui.waitUntil(10_000) { ui.onAllNodesWithText("Copied").fetchSemanticsNodes().isNotEmpty() }
        ui.runOnIdle {
            val text = (ui.activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).primaryClip!!.getItemAt(0).text.toString()
            assertTrue(text.startsWith("Harbour")); assertTrue(text.contains("Passage")); assertTrue(text.contains("👩‍👩‍👧‍👦"))
        }
        ui.onNodeWithContentDescription("Highlight pink").performClick(); waitCount(1)
        val id = loadAnnotations(book.fingerprint).highlights.single().id
        openBook(book.path).use { task -> while (task.status() == LayoutStatus.RUNNING) Thread.sleep(10); task.result()!!.use { native ->
            val target = native.annotationTarget(id, false)
            assertEquals(1u, target.location.page)
            assertTrue(native.selectionText(target.selection!!).contains("Passage"))
        } }
    }

    @Test fun pdfHandlesCrossPageHighlightsNavigationAndBookmarksPersist() {
        open(pdf = true)
        val layer = openPdfDocument(book.path).use { it.text(1u, 1080u) }
        val first = layer.glyphs[0].bounds!!
        ui.onNodeWithTag("pdfPage").performTouchInput { longClick(Offset((first.left + first.right) / 2 * width, (first.top + first.bottom) / 2 * height)) }
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("pdfSelection").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithTag("selectionEnd", useUnmergedTree = true).performTouchInput { swipe(center, center + Offset(120f, 0f), 600) }
        ui.onNodeWithContentDescription("Highlight green").performClick(); waitCount(1)
        ui.onNodeWithText("Select page text").performClick(); ui.onNodeWithText("Next").performClick()
        ui.waitUntil(30_000) { ui.onAllNodesWithText("Select page text").fetchSemanticsNodes().any { !it.config.contains(androidx.compose.ui.semantics.SemanticsProperties.Disabled) } }
        ui.onNodeWithTag("pdfPage").performTouchInput { click(Offset(width * .2f, height * .1f)) }
        ui.onNodeWithText("Add note").performClick(); ui.onNodeWithTag("annotationNote").performTextInput("Across PDF pages"); ui.onNodeWithText("Save").performClick(); waitCount(2)
        val entry = loadAnnotations(book.fingerprint).highlights.first { it.note != null }
        openPdfDocument(book.path).use { native ->
            val mark = native.pdfMarks().first { it.id == entry.id }
            assertEquals(1u, mark.from.page); assertEquals(2u, mark.to.page)
        }
        ui.onNodeWithContentDescription("Bookmark page").performClick(); waitCount(2, 1); screenshot("pdf-highlight")
        ui.activityRule.scenario.recreate()
        ui.waitUntil(30_000) { ui.onAllNodesWithContentDescription("Annotations").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithContentDescription("Annotations").performClick(); ui.onNodeWithText("Notes", substring = false).performClick()
        ui.onNodeWithText("Across PDF pages").performClick()
        ui.waitUntil(30_000) { ui.onAllNodesWithText("Page 1 of 3").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithTag("pdfSelection").assertExists()
        ui.onNodeWithText("Clear").performClick()
        ui.onNodeWithContentDescription("Annotations").performClick(); ui.onNodeWithText("Bookmarks", substring = false).performClick()
        ui.onNodeWithText("Page 2", substring = false).performClick()
        ui.waitUntil(30_000) { ui.onAllNodesWithText("Page 2 of 3").fetchSemanticsNodes().isNotEmpty() }
    }

    @Test fun heldPdfHandleScrollsAndTurnsAcrossThePageEdge() {
        open(pdf = true); ui.onNodeWithText("Select page text").performClick()
        // The last glyph of this short PDF is visible; the viewport keeps the
        // pointer capture when its page Canvas is replaced.
        val handle = ui.onNodeWithTag("selectionEnd", useUnmergedTree = true).fetchSemanticsNode().boundsInRoot.center
        val viewport = ui.onNodeWithTag("pdfViewport").fetchSemanticsNode().boundsInRoot
        ui.onNodeWithTag("pdfViewport").performTouchInput { down(handle - viewport.topLeft); moveTo(Offset(width * .5f, height - 8f)) }
        try {
            ui.waitUntil(10_000) { ui.onAllNodesWithText("Page 2 of 3").fetchSemanticsNodes().isNotEmpty() }
        } finally { ui.onNodeWithTag("pdfViewport").performTouchInput { up() } }
        ui.onNodeWithContentDescription("Highlight blue").performClick(); waitCount(1)
        openPdfDocument(book.path).use { native ->
            val mark = native.pdfMarks().single()
            assertEquals(1u, mark.from.page); assertTrue(mark.to.page >= 2u)
        }
    }

    @Test fun heldReflowHandleContinuesAcrossACanonicalParagraphCut() {
        open(); selectReaderWord()
        val handle = ui.onNodeWithTag("selectionEnd", useUnmergedTree = true).fetchSemanticsNode().boundsInRoot.center
        val viewport = ui.onNodeWithTag("paperViewport").fetchSemanticsNode().boundsInRoot
        ui.onNodeWithTag("paperViewport").performTouchInput { down(handle - viewport.topLeft); moveTo(Offset(width * .5f, height - 8f)) }
        try {
            ui.waitUntil(10_000) { ui.onAllNodesWithText("Page 2", substring = true).fetchSemanticsNodes().isNotEmpty() }
        } finally { ui.onNodeWithTag("paperViewport").performTouchInput { up() } }
        ui.onNodeWithContentDescription("Highlight green").performClick(); waitCount(1)
        val quote = loadAnnotations(book.fingerprint).highlights.single().quote
        assertTrue(quote, quote.startsWith("Harbour"))
        // Page one ends before Passage50; the unchanged second paragraph spans
        // multiple canonical pages and selection must reach its next visible cut.
        assertTrue(quote, quote.contains("Passage50"))
    }

    @Test fun overlappingPaintComponentsKeepDesktopColorOrderAndAlpha() {
        val yellow = AnnotationColor.YELLOW; val blue = AnnotationColor.BLUE
        assertEquals(listOf((0..10) to yellow, (3..26) to blue, (20..30) to yellow),
            coloredRanges(listOf((0..4) to yellow, (3..24) to blue, (20..30) to yellow, (4..10) to yellow, (24..26) to blue)))
        assertEquals(.42f, yellow.tint().alpha, .01f); assertEquals(.38f, blue.tint().alpha, .01f)
    }
}
