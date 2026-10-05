package io.github.tikkaaa3.simpl

import android.os.Debug
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.util.UUID
import org.junit.*
import org.junit.Assert.*
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PdfBookTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()
    private val imported = mutableListOf<String>()
    private fun book(pages: Int = 3, lines: Int = 24): LibraryBook {
        val file = File(ui.activity.cacheDir, "p4-${UUID.randomUUID()}.pdf").apply { writeBytes(pdfFixture(pages, lines, UUID.randomUUID().toString())) }
        return importLibraryBook(file.absolutePath).also { imported += it.fingerprint; file.delete() }
    }
    @Before fun prepare() { ReadAloud.stop() }
    @After fun cleanup() { imported.forEach(::removeLibraryBook) }
    private fun open(book: LibraryBook) {
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].open(book) }
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pdfPage").fetchSemanticsNodes().isNotEmpty() }
        ui.openReaderTools()
        ui.waitUntil(30_000) { ui.onAllNodesWithText("Book").fetchSemanticsNodes().any { !it.config.contains(androidx.compose.ui.semantics.SemanticsProperties.Disabled) } }
        androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK)
    }
    private fun ready(page: UInt, bookView: Boolean) {
        ui.waitUntil(120_000) { ui.onAllNodesWithTag("pageLabel").fetchSemanticsNodes().any { node ->
            node.config.getOrElse(androidx.compose.ui.semantics.SemanticsProperties.StateDescription) { "" }.startsWith("Page $page of")
        } && ui.onAllNodesWithTag(if (bookView) "bookPreparation" else "pdfRendering").fetchSemanticsNodes().isEmpty() }
    }
    @Test fun modeSwitchKeepsPhysicalPageAndBookChoiceSurvivesRecreationAndReopen() {
        val book = book(); open(book)
        ui.onNodeWithContentDescription("Next").performClick(); ready(2u, false)
        ui.readerTool("Book"); ready(2u, true)
        ui.openReaderTools()
        ui.onNodeWithText("Reading options").assertExists(); ui.onNodeWithText("Document").assertExists()
        androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK)
        val bytes = java.io.ByteArrayOutputStream()
        ui.onRoot().captureToImage().asAndroidBitmap().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, bytes)
        saveP34Artifact(ui.activity, "pdf-book.png", "image/png", bytes.toByteArray())
        ui.onNodeWithContentDescription("Next").performClick(); ready(3u, true)
        ui.activityRule.scenario.recreate(); ready(3u, true)
        ui.readerTool("Document"); ready(3u, false)
        ui.readerTool("Book"); ready(3u, true)
        ui.onNodeWithContentDescription("Back to library").performClick()
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].open(book) }
        ready(3u, true); assertTrue(pdfBookMode(book.path, book.fingerprint))
        ui.readerTool("Document"); ready(3u, false)
        assertFalse(pdfBookMode(book.path, book.fingerprint))
    }
    @Test fun conversionCacheIsReusableAndCorruptCacheIsRebuiltWithoutTruncation() {
        val book = book(8, 32)
        fun prepared(): BookPreparation {
            val task = openBook(book.path)
            try {
                ui.waitUntil(90_000) { task.status() != LayoutStatus.RUNNING }
                task.result()!!.use { assertEquals(8u, it.readerInfo().total); assertTrue(it.readerInfo().sourcePages) }
                return task.progress()
            } finally { task.close() }
        }
        val first = prepared(); assertFalse(first.cached); assertEquals(8u, first.completed)
        val document = openPdfDocument(book.path)
        val task = openBook(book.path)
        try {
            document.toggleBookmark(2u)
            ui.waitUntil(90_000) { task.status() != LayoutStatus.RUNNING }
            task.result()!!.use { native ->
                assertTrue(native.readerAnnotations().bookmarks.isEmpty())
                native.toggleBookmark(3u)
                assertEquals(1, native.readerAnnotations().bookmarks.size)
                assertEquals(1, document.documentAnnotations().bookmarks.size)
                assertEquals(2, loadAnnotations(book.fingerprint).bookmarks.size)
            }
        } finally { task.close(); document.close() }
        assertTrue(prepared().cached)
        val directory = File(ui.activity.cacheDir, "simPl/pdf-books")
        val cache = directory.listFiles()!!.single { it.name.endsWith("${book.fingerprint}.json") }
        cache.writeText("corrupt cache")
        assertFalse(prepared().cached); assertTrue(prepared().cached)
    }
    @Test fun cancelledPreparationCanRetryAndCopyRestrictedPdfStaysInDocumentMode() {
        val book = book(256, 120)
        val task = openBook(book.path)
        try {
            ui.waitUntil(30_000) { task.progress().completed > 0u || task.status() != LayoutStatus.RUNNING }
            task.cancel(); ui.waitUntil(30_000) { task.status() != LayoutStatus.RUNNING }
            assertEquals(LayoutStatus.CANCELLED, task.status()); assertNull(task.result())
        } finally { task.close() }
        val cache = File(ui.activity.cacheDir, "simPl/pdf-books").listFiles().orEmpty().filter { it.name.contains(book.fingerprint) }
        assertTrue(cache.isEmpty())
        open(book); ui.readerTool("Book")
        ui.waitUntil(10_000) { ui.onAllNodesWithText("Cancel preparation").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithText("Cancel preparation").performClick()
        ready(1u, false); ui.readerTool("Book"); ready(1u, true)
        ui.onNodeWithContentDescription("Back to library").performClick()
        val file = File(ui.activity.cacheDir, "p4-restricted.pdf").apply {
            InstrumentationRegistry.getInstrumentation().context.assets.open("copy-restricted.pdf").use { input -> outputStream().use(input::copyTo) }
        }
        val restricted = importLibraryBook(file.absolutePath); imported += restricted.fingerprint
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].open(restricted) }
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pdfPage").fetchSemanticsNodes().isNotEmpty() }
        ui.openReaderTools()
        ui.onNodeWithText("Book").assertIsNotEnabled()
        val forbidden = openBook(restricted.path)
        try {
            ui.waitUntil(30_000) { forbidden.status() != LayoutStatus.RUNNING }
            assertNotNull(runCatching { forbidden.result() }.exceptionOrNull())
        } finally { forbidden.close(); file.delete() }
    }
    @Test fun largeBookPreparationMeasuresColdAndCachedTimeAndSampledMemory() {
        val book = book(512, 240)
        Runtime.getRuntime().gc()
        val memory = Debug.MemoryInfo(); Debug.getMemoryInfo(memory); val baseline = memory.totalPss
        var peak = baseline; var completed = 0u; var cold = 0L; var warm = 0L
        repeat(2) { run ->
            val started = android.os.SystemClock.elapsedRealtime()
            val task = openBook(book.path)
            try {
                ui.waitUntil(300_000) {
                    Debug.getMemoryInfo(memory); peak = maxOf(peak, memory.totalPss)
                    val progress = task.progress(); assertTrue(progress.completed >= completed || run == 1); completed = progress.completed
                    task.status() != LayoutStatus.RUNNING
                }
                val ms = android.os.SystemClock.elapsedRealtime() - started
                task.result()!!.use { native ->
                    assertEquals(512u, native.readerInfo().total)
                    assertTrue(native.page(512u).flatMap { it.rows }.any { it.text.orEmpty().contains("Passage 512") })
                }
                if (run == 0) { cold = ms; assertFalse(task.progress().cached) } else { warm = ms; assertTrue(task.progress().cached) }
            } finally { task.close() }
        }
        val record = org.json.JSONObject().put("device", android.os.Build.MODEL).put("abi", android.os.Build.SUPPORTED_ABIS[0])
            .put("sdk", android.os.Build.VERSION.SDK_INT).put("pages", 512).put("linesPerPage", 240)
            .put("coldPreparationMs", cold).put("cachedPreparationMs", warm).put("baselinePssKiB", baseline).put("peakSampledPssKiB", peak)
            .put("scope", "Conversion and canonical atlas; emulator measurement, not physical-phone performance")
        android.util.Log.i("simPl-P4", record.toString())
        saveP34Artifact(ui.activity, "pdf-book-measurement.json", "application/json", record.toString().toByteArray())
    }
}

internal fun saveP34Artifact(context: android.content.Context, name: String, mime: String, bytes: ByteArray) {
    if (android.os.Build.VERSION.SDK_INT >= 29) {
        val resolver = context.contentResolver
        val collection = if (mime == "image/png") android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI else android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI
        val uri = requireNotNull(resolver.insert(collection, android.content.ContentValues().apply {
            put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, name); put(android.provider.MediaStore.MediaColumns.MIME_TYPE, mime)
            put(android.provider.MediaStore.MediaColumns.RELATIVE_PATH, if (mime == "image/png") "Pictures/simPl-P34" else "Download/simPl-P34")
        }))
        resolver.openOutputStream(uri)!!.use { it.write(bytes) }
    } else File(context.getExternalFilesDir(null), name).writeBytes(bytes)
}
