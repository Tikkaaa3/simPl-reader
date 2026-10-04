package io.github.tikkaaa3.simpl

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import androidx.compose.ui.test.*
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.core.content.IntentCompat
import androidx.lifecycle.ViewModelProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.util.concurrent.atomic.AtomicReference
import org.junit.*
import org.junit.Assert.*
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class BackupTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()
    private lateinit var previous: File
    private lateinit var options: DictionaryOptions
    private lateinit var appearance: Map<String, *>
    private fun fixture(name: String) = File(ui.activity.cacheDir, name).apply {
        InstrumentationRegistry.getInstrumentation().context.assets.open(name).use { input -> outputStream().use(input::copyTo) }
    }
    private fun uri(name: String): Uri = DocumentsContract.buildDocumentUri("io.github.tikkaaa3.simpl.test.documents", name)
    @Before fun prepare() {
        options = OfflineDictionary.options.value
        appearance = ui.activity.getSharedPreferences("interface", 0).all.toMap()
        previous = File(ui.activity.cacheDir, "p3-previous.zip")
        createBackup(previous.absolutePath, true, true)
        grantFixtureDocuments()
    }
    @After fun cleanup() {
        restoreBackup(previous.absolutePath); previous.delete(); OfflineDictionary.resetCache(); OfflineDictionary.configure(options)
        val editor = ui.activity.getSharedPreferences("interface", 0).edit().clear()
        appearance.forEach { (key, value) -> if (value is String) editor.putString(key, value) }; editor.commit()
    }
    private fun restoreFixture(light: Boolean = false): LibraryBook {
        val file = fixture(if (light) "p3-windows-light.zip" else "p3-windows.zip")
        restoreBackup(file.absolutePath); file.delete(); OfflineDictionary.resetCache()
        return loadLibrary().books.single()
    }
    private fun backupScreen() {
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].restored() }
        ui.waitUntil(10_000) { ui.onAllNodesWithContentDescription("Settings").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithContentDescription("Settings").performClick()
        ui.onNodeWithTag("settingsList").performScrollToNode(hasText("Backup and export"))
        ui.onNodeWithText("Backup and export").performClick()
    }
    private fun screenshot(name: String) {
        val bytes = java.io.ByteArrayOutputStream()
        ui.onRoot().captureToImage().asAndroidBitmap().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, bytes)
        saveP34Artifact(ui.activity, name, "image/png", bytes.toByteArray())
    }
    private fun monitor(action: String, chosen: Uri, operation: () -> Unit) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val monitor = object : Instrumentation.ActivityMonitor() {
            override fun onStartActivity(intent: Intent): Instrumentation.ActivityResult? =
                if (intent.action == action) Instrumentation.ActivityResult(Activity.RESULT_OK, Intent().setData(chosen).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)) else null
        }
        instrumentation.addMonitor(monitor)
        try { operation() } finally { instrumentation.removeMonitor(monitor) }
    }
    @Test fun windowsArchiveRestoresPathsPositionsNotesAndShelvesAndCreatesPortableSnapshot() {
        val book = restoreFixture()
        assertFalse(book.missing); assertTrue(File(book.path).isFile); assertTrue(book.favourite)
        assertEquals(1, loadLibrary().shelves.size)
        assertEquals("Travel note — İstanbul 😀", loadAnnotations(book.fingerprint).highlights.single().note)
        val opened = openBook(book.path)
        try {
            ui.waitUntil(30_000) { opened.status() != LayoutStatus.RUNNING }
            opened.result()!!.use { assertEquals(.5f, it.readerInfo().restored!!.within, .01f) }
        } finally { opened.close() }
        val zip = File(ui.activity.cacheDir, "p3-roundtrip.zip")
        val summary = createBackup(zip.absolutePath, true, false)
        assertTrue(summary.documents); assertEquals(summary.files, inspectBackup(zip.absolutePath).files)
        java.util.zip.ZipFile(zip).use { archive ->
            assertTrue(archive.entries().asSequence().any { it.name.startsWith("positions/") })
            assertFalse(archive.entries().asSequence().any { it.name.contains("page-maps") })
        }
        zip.delete()
    }
    @Test fun lightweightRestoreLocatesOnlyMatchingContentAndCarriesTheSavedPosition() {
        val book = restoreFixture(true); assertTrue(book.missing)
        assertNotNull(runCatching { locateLibraryBook(fixture("p3-android.zip").absolutePath, book.fingerprint) }.exceptionOrNull())
        val wrong = File(ui.activity.cacheDir, "p3-wrong.html").apply { writeText("<p>Different content</p>") }
        assertTrue(runCatching { locateLibraryBook(wrong.absolutePath, book.fingerprint) }.exceptionOrNull()!!.message!!.contains("does not match"))
        assertEquals(book.path, loadLibrary().books.single().path)
        backupScreen()
        monitor(Intent.ACTION_OPEN_DOCUMENT, uri("p3-portable.html")) {
            ui.onNodeWithText("Locate").performClick()
            ui.waitUntil(30_000) { loadLibrary().books.singleOrNull()?.missing == false }
        }
        val found = loadLibrary().books.single()
        assertFalse(found.missing); assertTrue(found.favourite)
        assertEquals(book.title, found.title)
        val task = openBook(found.path)
        try {
            ui.waitUntil(30_000) { task.status() != LayoutStatus.RUNNING }
            task.result()!!.use { assertEquals(.5f, it.readerInfo().restored!!.within, .01f) }
        } finally { task.close(); wrong.delete() }
        assertEquals(1, loadAnnotations(found.fingerprint).highlights.size)
    }
    @Test fun corruptArchivePreservesTheProfileAndOldReadersCannotWriteAfterRestore() {
        val book = restoreFixture()
        val task = openBook(book.path)
        ui.waitUntil(30_000) { task.status() != LayoutStatus.RUNNING }
        val opened = task.result()!!; task.close()
        val old = File(ui.activity.filesDir, "simPl/library.json").readBytes()
        val corrupt = File(ui.activity.cacheDir, "p3-corrupt.zip").apply { writeText("Not a backup") }
        assertNotNull(runCatching { restoreBackup(corrupt.absolutePath) }.exceptionOrNull())
        assertArrayEquals(old, File(ui.activity.filesDir, "simPl/library.json").readBytes())
        restoreFixture()
        assertNotNull(runCatching { opened.saveOptions(LayoutOptions(ReadingFont.THEME, 24u, 48u, 0u)) }.exceptionOrNull())
        opened.close(); corrupt.delete()
    }
    @Test fun safCreateAndRestoreUseVerifiedArchivesAndExplicitReplacementConfirmation() {
        restoreFixture(); backupScreen()
        screenshot("backup.png")
        monitor(Intent.ACTION_CREATE_DOCUMENT, uri("p3-backup.zip")) {
            ui.onNodeWithText("Create backup").performClick()
            ui.activityRule.scenario.recreate()
            ui.waitUntil(30_000) { ui.onAllNodesWithText("Backup saved:", substring = true).fetchSemanticsNodes().isNotEmpty() }
        }
        val copied = File(ui.activity.cacheDir, "p3-provider.zip")
        ui.activity.contentResolver.openInputStream(uri("p3-backup.zip"))!!.use { input -> copied.outputStream().use(input::copyTo) }
        assertTrue(inspectBackup(copied.absolutePath).documents); copied.delete()
        monitor(Intent.ACTION_OPEN_DOCUMENT, uri("p3-windows.zip")) {
            ui.onNodeWithText("Restore backup").performClick()
            ui.waitUntil(30_000) { ui.onAllNodesWithText("Restore this backup?").fetchSemanticsNodes().isNotEmpty() }
        }
        ui.onNodeWithText("Cancel").performClick(); ui.onNodeWithText("Restore this backup?").assertDoesNotExist()
        monitor(Intent.ACTION_OPEN_DOCUMENT, uri("p3-windows.zip")) {
            ui.onNodeWithText("Restore backup").performClick()
            ui.waitUntil(30_000) { ui.onAllNodesWithText("Restore this backup?").fetchSemanticsNodes().isNotEmpty() }
        }
        ui.onNodeWithText("Restore", useUnmergedTree = true).performClick()
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("library").fetchSemanticsNodes().isNotEmpty() }
        assertTrue(loadPortablePreferences().dark)
        assertFalse(OfflineDictionary.options.value.automatic)
    }
    @Test fun allNoteFormatsRetainUnicodeAndShareUsesAReadOnlyContentUri() {
        val book = restoreFixture()
        for (format in NotesFormat.entries) {
            val file = File(ui.activity.cacheDir, "p3-export-${format.name}")
            exportNotes(file.absolutePath, book.fingerprint, book.title, format)
            assertTrue(file.readText().contains("Travel note — İstanbul 😀"))
            if (format == NotesFormat.JSON) assertEquals(book.fingerprint, org.json.JSONObject(file.readText()).getString("fingerprint"))
            file.delete()
        }
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].restored(); ViewModelProvider(it)[LibraryViewModel::class.java].open(book) }
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pageLabel").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithContentDescription("Annotations").performClick()
        screenshot("notes-export.png")
        monitor(Intent.ACTION_CREATE_DOCUMENT, uri("p3-notes.md")) {
            ui.onNodeWithText("Export notes").performClick()
            ui.waitUntil(10_000) { ui.onAllNodesWithText("Notes saved").fetchSemanticsNodes().isNotEmpty() }
        }
        ui.onNodeWithText("OK").performClick()
        val text = ui.activity.contentResolver.openInputStream(uri("p3-notes.md"))!!.bufferedReader().use { it.readText() }
        assertTrue(text.startsWith("# Portable Harbour"))
        val shared = AtomicReference<Intent>()
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val observer = object : Instrumentation.ActivityMonitor() {
            override fun onStartActivity(intent: Intent): Instrumentation.ActivityResult? {
                if (intent.action != Intent.ACTION_CHOOSER) return null
                shared.set(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_INTENT, Intent::class.java)); return Instrumentation.ActivityResult(0, null)
            }
        }
        instrumentation.addMonitor(observer)
        try {
            ui.onNodeWithText("JSON").performClick(); ui.onNodeWithText("Share notes").performClick()
            ui.waitUntil(10_000) { shared.get() != null }
            val sent = shared.get(); assertEquals("application/json", sent.type)
            val stream = IntentCompat.getParcelableExtra(sent, Intent.EXTRA_STREAM, Uri::class.java)!!
            assertEquals("content", stream.scheme); assertEquals(0, sent.flags and Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
            assertTrue(sent.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION != 0)
            val data = ui.activity.contentResolver.openInputStream(stream)!!.bufferedReader().use { it.readText() }
            assertTrue(data.contains("Travel note — İstanbul 😀"))
        } finally { instrumentation.removeMonitor(observer) }
    }
}
