package io.github.tikkaaa3.simpl

import android.content.ClipboardManager
import android.content.ContentValues
import android.content.Context
import android.graphics.Bitmap
import android.os.Build
import android.os.Debug
import android.os.SystemClock
import android.provider.MediaStore
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.*
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.util.UUID
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** File-backed, self-authored pages with enough text to exercise glyph geometry. */
internal fun pdfFixture(pages: Int = 3, lines: Int = 24, identity: String = UUID.randomUUID().toString()): ByteArray {
    val objects = mutableListOf(
        "<</Type/Catalog/Pages 2 0 R>>",
        "<</Type/Pages/Kids[${(0 until pages).joinToString(" ") { "${4 + it * 2} 0 R" }}]/Count $pages>>",
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>",
    )
    repeat(pages) { page ->
        val content = buildString {
            append("BT /F1 18 Tf 32 730 Td (Lighthouse keeper page ${page + 1}) Tj /F1 9 Tf ")
            repeat(lines) { line -> append("0 -${if (lines > 100) 2 else 24} Td (The harbour tide brings boats home. Passage ${page + 1}, line $line. $identity) Tj ") }
            append("ET 0.12 0.36 0.6 rg 32 20 220 30 re f")
        }
        objects += "<</Type/Page/Parent 2 0 R/MediaBox[0 0 540 780]/Contents ${5 + page * 2} 0 R/Resources<</Font<</F1 3 0 R>>>>>>"
        objects += "<</Length ${content.length}>>\nstream\n$content\nendstream"
    }
    val pdf = StringBuilder("%PDF-1.4\n")
    val offsets = objects.mapIndexed { index, body -> pdf.length.also { pdf.append("${index + 1} 0 obj\n$body\nendobj\n") } }
    val xref = pdf.length
    pdf.append("xref\n0 ${objects.size + 1}\n0000000000 65535 f \n")
    offsets.forEach { pdf.append("%010d 00000 n \n".format(java.util.Locale.ROOT, it)) }
    pdf.append("trailer\n<</Root 1 0 R/Size ${objects.size + 1}>>\nstartxref\n$xref\n%%EOF\n")
    return pdf.toString().toByteArray(Charsets.US_ASCII)
}

@RunWith(AndroidJUnit4::class)
class PdfUiTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()
    private lateinit var book: LibraryBook
    private val imported = mutableListOf<String>()
    @Before fun prepare() {
        val file = File(ui.activity.cacheDir, "M5-pages.pdf").apply { writeBytes(pdfFixture()) }
        book = importLibraryBook(file.absolutePath); imported += book.fingerprint
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].apply { query(""); filter("all"); reload(); appearance(Appearance.Light) } }
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("open:${book.fingerprint}").fetchSemanticsNodes().isNotEmpty() }
    }
    @After fun cleanup() {
        if (ui.onAllNodesWithContentDescription("Back to library").fetchSemanticsNodes().isNotEmpty()) ui.onNodeWithContentDescription("Back to library").performClick()
        imported.forEach { fingerprint -> if (loadLibrary().books.any { it.fingerprint == fingerprint }) removeLibraryBook(fingerprint) }
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].appearance(Appearance.System) }
    }
    private fun ready(page: UInt, total: Int = 3) {
        ui.waitUntil(30_000) { ui.onAllNodes(readerPageMatcher("Page $page of $total")).fetchSemanticsNodes().isNotEmpty() }
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pdfPage").fetchSemanticsNodes().any { it.config[SemanticsProperties.StateDescription] == "Page ready" } }
        ui.waitForIdle()
    }
    private fun open() { ui.onNodeWithTag("open:${book.fingerprint}").performClick(); ready(1u) }
    private fun jump(value: String) {
        ui.readerJump(value)
    }
    @Test fun physicalPagesZoomFitSwipeEdgeJumpAndStopCheckpoint() {
        open(); screenshot("document-fit")
        ui.onNodeWithContentDescription("Previous").assertIsNotEnabled()
        ui.onNodeWithContentDescription("Next").performClick(); ready(2u)
        ui.onNodeWithTag("pdfViewport").performTouchInput { swipeLeft() }; ready(3u)
        ui.onNodeWithContentDescription("Next").assertIsNotEnabled()
        ui.onNodeWithTag("pdfViewport").performTouchInput { click(Offset(width * .05f, height * .2f)) }; ready(2u)
        ui.onNodeWithTag("pdfViewport").performTouchInput { pinch(Offset(width * .4f, height * .4f), Offset(width * .6f, height * .6f), Offset(width * .15f, height * .2f), Offset(width * .85f, height * .8f)) }
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pdfRendering").fetchSemanticsNodes().isEmpty() }
        ui.onNodeWithTag("pdfViewport").performTouchInput { swipeUp() }
        // Finish the fling on Compose's test clock before ActivityScenario waits
        // for the Android looper, which cannot advance that clock itself.
        ui.waitForIdle()
        ui.activityRule.scenario.moveToState(Lifecycle.State.CREATED)
        openPdfDocument(book.path).use { native ->
            val location = native.info().restored
            assertEquals(2u, location.page); assertFalse(location.fitWidth); assertTrue(location.within > 0f)
        }
        ui.activityRule.scenario.moveToState(Lifecycle.State.RESUMED); ready(2u)
        screenshot("document-zoom")
        ui.activityRule.scenario.recreate(); ready(2u)
        ui.readerTool("Fit width")
        openPdfDocument(book.path).use { native -> ui.waitUntil(10_000) { native.info().restored.fitWidth } }
        jump("99"); ui.onNodeWithText("Enter a page number from 1 to 3").assertExists()
        ui.onNodeWithText("OK").performClick(); cancelReaderJump()
        jump("1"); ready(1u)
        ui.onNodeWithContentDescription("Back to library").performClick()
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("open:${book.fingerprint}").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithTag("open:${book.fingerprint}").performClick(); ready(1u)
    }
    @Test fun glyphLongPressDragAndClipboardCopyRespectPermissions() {
        open()
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pdfPage").fetchSemanticsNodes().any {
            it.config.getOrElse(androidx.compose.ui.semantics.SemanticsActions.CustomActions) { emptyList() }.any { action -> action.label == "Select page text" }
        } }
        val layer = openPdfDocument(book.path).use { it.text(1u, 1080u) }
        val first = layer.glyphs[0].bounds!!
        val last = layer.glyphs[9].bounds!!
        ui.onNodeWithTag("pdfPage").performTouchInput {
            val start = Offset((first.left + first.right) / 2 * width, (first.top + first.bottom) / 2 * height)
            val end = Offset((last.left + last.right) / 2 * width, (last.top + last.bottom) / 2 * height)
            down(start); advanceEventTime(800); moveTo(end); up()
        }
        ui.onNodeWithTag("pdfSelection").assertExists(); ui.onNodeWithContentDescription("Next").assertExists(); screenshot("document-selection")
        ui.onNodeWithContentDescription("Copy").performClick()
        ui.waitUntil(10_000) { ui.onAllNodesWithContentDescription("Copied").fetchSemanticsNodes().isNotEmpty() }
        ui.runOnIdle {
            val clipboard = ui.activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            assertEquals("Lighthouse", clipboard.primaryClip!!.getItemAt(0).text.toString())
        }
        ui.onNodeWithContentDescription("Clear selection").performClick(); ui.readerTool("Select page text")
        ui.onNodeWithContentDescription("Copy").performClick()
        ui.waitUntil(10_000) { ui.onAllNodesWithContentDescription("Copied").fetchSemanticsNodes().isNotEmpty() }
        ui.runOnIdle {
            val clipboard = ui.activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            assertTrue(clipboard.primaryClip!!.getItemAt(0).text.toString().contains("Passage 1, line 23"))
        }
        ui.onNodeWithContentDescription("Back to library").performClick()
        val file = File(ui.activity.cacheDir, "M5-copy-restricted.pdf").apply {
            writeBytes(InstrumentationRegistry.getInstrumentation().context.assets.open("copy-restricted.pdf").use { it.readBytes() })
        }
        val restricted = importLibraryBook(file.absolutePath); imported += restricted.fingerprint
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].open(restricted) }
        ready(1u, 1); ui.openReaderTools()
        ui.onNodeWithText("Select page text").assertIsNotEnabled()
        ui.onNodeWithContentDescription("Close reader tools").performClick()
        ui.onNodeWithTag("pdfPage").performTouchInput { longClick(center) }
        ui.onNodeWithTag("pdfSelection").assertDoesNotExist(); screenshot("document-copy-restricted")
    }
    @Test fun cacheEvictionAndLargeDocumentMeasurementsRemainBounded() {
        val lru = PdfBitmapCache(128 * 128 * 4L * 2)
        val a = Bitmap.createBitmap(128, 128, Bitmap.Config.ARGB_8888)
        val b = Bitmap.createBitmap(128, 128, Bitmap.Config.ARGB_8888)
        val c = Bitmap.createBitmap(128, 128, Bitmap.Config.ARGB_8888)
        lru.put(PdfRasterKey(1u, 128u), a); lru.put(PdfRasterKey(2u, 128u), b)
        assertSame(a, lru.get(PdfRasterKey(1u, 128u)))
        lru.put(PdfRasterKey(3u, 128u), c)
        assertNull(lru.get(PdfRasterKey(2u, 128u))); assertSame(a, lru.get(PdfRasterKey(1u, 128u)))
        assertEquals(1, lru.evictions); assertTrue(lru.bytes <= lru.limit); lru.clear()
        val file = File(ui.activity.cacheDir, "M5-large.pdf").apply { writeBytes(pdfFixture(512, 240)) }
        val large = importLibraryBook(file.absolutePath); imported += large.fingerprint
        Runtime.getRuntime().gc()
        val baselineMemory = Debug.MemoryInfo(); Debug.getMemoryInfo(baselineMemory)
        val store = ViewModelStore()
        lateinit var model: PdfViewModel
        ui.runOnIdle { model = PdfViewModel(ui.activity.application, SavedStateHandle()); store.put("measurement", model); model.open(large) }
        try {
            ui.waitUntil(30_000) { model.state.value.info != null }
            val opening = model.state.value.firstOpenMs
            var peakPss = 0
            val times = mutableListOf<Long>()
            for (page in listOf(1u, 256u, 512u, 256u, 1u)) {
                ui.runOnIdle { model.jump(page.toString()); model.render(4000f) }
                ui.waitUntil(30_000) { model.state.value.rasterKey?.page == page && !model.state.value.rendering }
                assertTrue(model.state.value.cacheBytes <= 24L * 1024 * 1024)
                times += model.state.value.renderMs
                val memory = Debug.MemoryInfo(); Debug.getMemoryInfo(memory); peakPss = maxOf(peakPss, memory.totalPss)
            }
            ui.runOnIdle { model.render(4000f) }
            assertTrue(model.state.value.cacheEvictions > 0)
            assertTrue(model.state.value.cacheHits > 0)
            val record = "{\"device\":\"${Build.MODEL}\",\"abi\":\"${Build.SUPPORTED_ABIS[0]}\",\"sdk\":${Build.VERSION.SDK_INT},\"pages\":512,\"sourceBytes\":${file.length()},\"openMs\":$opening,\"firstRasterMs\":${model.state.value.firstRasterMs},\"renderMs\":$times,\"baselinePssKiB\":${baselineMemory.totalPss},\"peakSampledPssKiB\":$peakPss,\"cacheBytes\":${model.state.value.cacheBytes},\"hits\":${model.state.value.cacheHits},\"evictions\":${model.state.value.cacheEvictions}}"
            android.util.Log.i("simPl-M5", record)
            saveArtifact("large-pdf.json", "application/json", record.toByteArray())
        } finally { ui.runOnIdle { store.clear() } }
    }
    private fun screenshot(name: String) {
        ui.waitForIdle(); val bytes = java.io.ByteArrayOutputStream()
        ui.onRoot().captureToImage().asAndroidBitmap().compress(Bitmap.CompressFormat.PNG, 100, bytes)
        saveArtifact("$name.png", "image/png", bytes.toByteArray())
    }
    private fun saveArtifact(name: String, mime: String, bytes: ByteArray) {
        if (Build.VERSION.SDK_INT >= 29) {
            val resolver = ui.activity.contentResolver
            val collection = if (mime == "image/png") MediaStore.Images.Media.EXTERNAL_CONTENT_URI else MediaStore.Downloads.EXTERNAL_CONTENT_URI
            val uri = requireNotNull(resolver.insert(collection, ContentValues().apply {
                put(MediaStore.MediaColumns.DISPLAY_NAME, name); put(MediaStore.MediaColumns.MIME_TYPE, mime)
                put(MediaStore.MediaColumns.RELATIVE_PATH, if (mime == "image/png") "Pictures/simPl-M5" else "Download/simPl-M5")
            }))
            resolver.openOutputStream(uri)!!.use { it.write(bytes) }
        } else File(ui.activity.getExternalFilesDir(null), name).writeBytes(bytes)
    }
}
