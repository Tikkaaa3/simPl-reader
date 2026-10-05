package io.github.tikkaaa3.simpl

import android.provider.DocumentsContract
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.work.*
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.util.UUID
import java.util.concurrent.TimeUnit
import org.junit.*
import org.junit.Assert.*
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class DictionaryTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()
    private val store get() = OfflineDictionary.store
    private val imported = mutableListOf<String>()
    private lateinit var options: DictionaryOptions
    private val saved = mutableMapOf<UInt, ByteArray?>()
    private fun file(pack: DictionaryPackage) = File(ui.activity.filesDir, "simPl/dictionaries/${pack.url.substringAfterLast('/')}")
    @Before fun prepare() {
        options = OfflineDictionary.options.value
        store.inventory().forEach { saved[it.id] = file(it).takeIf(File::exists)?.readBytes() }
        OfflineDictionary.configure(DictionaryOptions(automatic = false))
        WorkManager.getInstance(ui.activity).cancelAllWorkByTag(DictionaryJobs.TAG).result.get()
    }
    @After fun cleanup() {
        WorkManager.getInstance(ui.activity).cancelAllWorkByTag(DictionaryJobs.TAG).result.get()
        // A stopped worker may still be unwinding its bounded blocking call.
        ui.waitUntil(10_000) { File(ui.activity.cacheDir, "dictionary-jobs").listFiles().orEmpty().isEmpty() }
        store.inventory().forEach { pack ->
            store.removePackage(pack.id)
            saved[pack.id]?.let { bytes -> file(pack).apply { parentFile!!.mkdirs(); writeBytes(bytes) } }
        }
        OfflineDictionary.configure(options)
        imported.forEach(::removeLibraryBook)
    }
    private fun fixture(pack: DictionaryPackage): File = File(ui.activity.cacheDir, "P2-${pack.id}.zip").apply {
        InstrumentationRegistry.getInstrumentation().context.assets.open(pack.url.substringAfterLast('/')).use { input -> outputStream().use(input::copyTo) }
    }
    private fun install(id: UInt = 0u) {
        val zip = fixture(store.inventory().first { it.id == id })
        try { store.installPackage(id, zip.absolutePath, DictionaryCancellation()) } finally { zip.delete() }
    }
    private fun jobs() = WorkManager.getInstance(ui.activity).getWorkInfosByTag(DictionaryJobs.TAG).get()
    private fun managerScreen() {
        ui.onNodeWithContentDescription("Settings").performClick()
        ui.onNodeWithTag("settingsList").performScrollToNode(hasText("Offline dictionaries"))
        ui.onNodeWithText("Offline dictionaries").performClick()
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("dictionaryPackage:0").fetchSemanticsNodes().isNotEmpty() }
    }
    private fun screenshot(name: String) {
        if (android.os.Build.VERSION.SDK_INT < 29) return
        val uri = requireNotNull(ui.activity.contentResolver.insert(android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
            android.content.ContentValues().apply {
                put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, "$name.png")
                put(android.provider.MediaStore.MediaColumns.MIME_TYPE, "image/png")
                put(android.provider.MediaStore.MediaColumns.RELATIVE_PATH, "Pictures/simPl-P2")
            }))
        ui.activity.contentResolver.openOutputStream(uri)!!.use {
            ui.onRoot().captureToImage().asAndroidBitmap().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
        }
    }
    private fun waitWork(id: UUID): WorkInfo {
        ui.waitUntil(180_000) { WorkManager.getInstance(ui.activity).getWorkInfoById(id).get()?.state?.isFinished == true }
        return WorkManager.getInstance(ui.activity).getWorkInfoById(id).get()!!
    }
    private fun open(pdf: Boolean = false) {
        val token = UUID.randomUUID().toString()
        val source = File(ui.activity.cacheDir, "P2-$token.${if (pdf) "pdf" else "html"}")
        if (pdf) source.writeBytes(pdfFixture(lines = 4, identity = token))
        else source.writeText("<title>P2 $token</title><p>Book books ran İstanbul. The offline dictionary stays on this device.</p>")
        val book = importLibraryBook(source.absolutePath); imported += book.fingerprint; source.delete()
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].open(book) }
        ui.waitUntil(30_000) { ui.onAllNodesWithTag(if (pdf) "pdfPage" else "row:0:0", useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        if (pdf) ui.waitUntil(30_000) { ui.onAllNodesWithTag("pdfPage").fetchSemanticsNodes().any {
            it.config.getOrElse(androidx.compose.ui.semantics.SemanticsActions.CustomActions) { emptyList() }.any { action -> action.label == "Select page text" }
        } }
        ui.waitForIdle()
    }
    @Test fun everyPinnedPackageImportsAndQueriesOfflineWithBoundedUnicodeRules() {
        for (pack in store.inventory()) {
            install(pack.id)
            val word = mapOf("en" to "book", "tr" to "kitap", "es" to "libro", "de" to "Buch", "fr" to "livre", "ja" to "本", "ko" to "책", "zh" to "書").getValue(pack.source)
            val result = store.lookup(word, pack.source, pack.target)
            assertNotNull(pack.label, result.headword); assertTrue(result.meanings.isNotEmpty())
            assertTrue(store.packageNotices(pack.id).contains("LICENSE", ignoreCase = true))
        }
        assertEquals("iyi", dictionaryQuery("İYİ", "tr")); assertEquals("book", dictionaryQuery("ＢＯＯＫ", "en"))
        assertNull(dictionaryQuery("one two three four five", "en")); assertNull(dictionaryQuery("a".repeat(257), "en"))
        assertTrue(store.lookup("ran", "en", "tr").baseForm)
        assertNull(store.lookup("zzzzzzzzzz", "en", "tr").headword)
        store.removePackage(0u)
        assertEquals(0u, store.lookup("book", "en", "tr").missingPackage)
    }
    @Test fun corruptAndCancelledInstallsPreserveExistingDataAndInvalidateRemovedCache() {
        install(); assertNotNull(store.lookup("book", "en", "tr").headword)
        val pack = store.inventory().first(); val before = file(pack).readBytes(); val zip = fixture(pack)
        try {
            zip.writeBytes(before.copyOf(before.size - 1))
            assertNotNull(runCatching { store.installPackage(0u, zip.absolutePath, DictionaryCancellation()) }.exceptionOrNull())
            zip.writeBytes(before)
            val cancelled = DictionaryCancellation().apply { cancel() }
            assertNotNull(runCatching { store.installPackage(0u, zip.absolutePath, cancelled) }.exceptionOrNull())
            assertArrayEquals(before, file(pack).readBytes())
            store.removePackage(0u); assertEquals(0u, store.lookup("book", "en", "tr").missingPackage)
            file(pack).writeBytes(before.copyOf(20)); assertEquals("invalid", store.inventory().first().state)
            assertEquals(0u, store.lookup("book", "en", "tr").missingPackage)
        } finally { zip.delete() }
    }
    @Test fun safImportUsesDurableWorkAndInvalidZipCanBeRetried() {
        grantFixtureDocuments(); store.removePackage(0u)
        val authority = "io.github.tikkaaa3.simpl.test.documents"
        DictionaryJobs.import(ui.activity, DocumentsContract.buildDocumentUri(authority, "bad.zip"))
        ui.waitUntil(10_000) { jobs().any { "dictionary-import-job" in it.tags && !it.state.isFinished } || jobs().any { it.state == WorkInfo.State.FAILED } }
        val bad = jobs().filter { "dictionary-import-job" in it.tags }.maxBy { it.tags.first { tag -> tag.startsWith("dictionary-created-") } }
        assertEquals(WorkInfo.State.FAILED, waitWork(bad.id).state)
        assertEquals("missing", store.inventory().first().state)
        DictionaryJobs.import(ui.activity, DocumentsContract.buildDocumentUri(authority, "Dictionary.zip"))
        ui.waitUntil(10_000) { jobs().any { "dictionary-import-job" in it.tags && it.id != bad.id && (it.state == WorkInfo.State.ENQUEUED || it.state == WorkInfo.State.RUNNING || it.state == WorkInfo.State.SUCCEEDED) } }
        val good = jobs().filter { "dictionary-import-job" in it.tags && it.id != bad.id && it.state != WorkInfo.State.CANCELLED }.maxBy { it.tags.first { tag -> tag.startsWith("dictionary-created-") } }
        assertEquals(WorkInfo.State.SUCCEEDED, waitWork(good.id).state)
        assertNotNull(store.lookup("book", "en", "tr").headword)
        ui.waitUntil(10_000) { ui.activity.getSharedPreferences("dictionary-grants", 0).all.isEmpty() }
    }
    @Test fun realReleaseDownloadVerifiesThenSupportsOfflineLookup() {
        store.removePackage(0u)
        val previous = jobs().map { it.id }.toSet()
        managerScreen()
        ui.onNode(hasText("Download", substring = false) and hasAnyAncestor(hasTestTag("dictionaryPackage:0"))).performClick()
        ui.waitUntil(10_000) { jobs().any { it.id !in previous } }
        val job = jobs().first { it.id !in previous }
        val progress = java.util.concurrent.atomic.AtomicLong()
        val observer = androidx.lifecycle.Observer<WorkInfo?> { info ->
            info?.progress?.getLong("bytes", 0)?.let { bytes -> progress.accumulateAndGet(bytes, ::maxOf) }
        }
        val live = WorkManager.getInstance(ui.activity).getWorkInfoByIdLiveData(job.id)
        ui.runOnUiThread { live.observeForever(observer) }
        ui.activityRule.scenario.recreate()
        val result = waitWork(job.id)
        ui.runOnUiThread { live.removeObserver(observer) }
        assertEquals(result.outputData.getString("message"), WorkInfo.State.SUCCEEDED, result.state)
        assertTrue("WorkManager must publish byte progress", progress.get() > 0)
        assertEquals("installed", store.inventory().first().state)
        assertNotNull(store.lookup("book", "en", "tr").headword)
        ui.waitUntil(10_000) { ui.onAllNodesWithText("Dictionary licenses").fetchSemanticsNodes().isNotEmpty() }
        screenshot("packages")
        ui.onNode(hasText("Dictionary licenses") and hasAnyAncestor(hasTestTag("dictionaryPackage:0"))).performClick()
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("dictionaryNotices").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithTag("dictionaryNotices").assertTextContains("WikDict", substring = true)
    }
    @Test fun pendingDownloadCancelsWithoutChangingInstalledData() {
        install(); val before = file(store.inventory().first()).readBytes()
        val request = OneTimeWorkRequestBuilder<DictionaryWorker>().setInputData(workDataOf("package" to 0))
            .setInitialDelay(1, TimeUnit.DAYS).addTag(DictionaryJobs.TAG).addTag("dictionary-package-0").build()
        val manager = WorkManager.getInstance(ui.activity)
        manager.enqueue(request).result.get(); manager.cancelWorkById(request.id).result.get()
        assertEquals(WorkInfo.State.CANCELLED, waitWork(request.id).state)
        assertArrayEquals(before, file(store.inventory().first()).readBytes())
        assertFalse(File(ui.activity.cacheDir, "dictionary-jobs/${request.id}.zip").exists())
    }
    @Test fun activeDownloadCancelsBeforeCommitAndCleansItsStage() {
        install(11u)
        val pack = store.inventory().first { it.id == 11u }; val before = file(pack).readBytes()
        val previous = jobs().map { it.id }.toSet(); DictionaryJobs.download(ui.activity, 11u)
        ui.waitUntil(10_000) { jobs().any { it.id !in previous } }
        val job = jobs().first { it.id !in previous }
        ui.waitUntil(30_000) {
            WorkManager.getInstance(ui.activity).getWorkInfoById(job.id).get()?.let {
                it.state == WorkInfo.State.RUNNING && it.progress.getLong("bytes", 0) > 0
            } == true
        }
        WorkManager.getInstance(ui.activity).cancelWorkById(job.id).result.get()
        assertEquals(WorkInfo.State.CANCELLED, waitWork(job.id).state)
        ui.waitUntil(10_000) { !File(ui.activity.cacheDir, "dictionary-jobs/${job.id}.zip").exists() }
        assertArrayEquals(before, file(pack).readBytes())
        assertNotNull(store.lookup("書", "zh", "en").headword)
    }
    @Test fun doubleTapAndManualSelectionShowTheSharedCardAndLanguageSettingsPersist() {
        install(); open()
        ui.onNodeWithTag("row:0:0", useUnmergedTree = true).performTouchInput { doubleClick(Offset(24f, 12f)) }
        ui.waitUntil(10_000) { ui.onAllNodesWithText("Close dictionary").fetchSemanticsNodes().isNotEmpty() }
        ui.waitUntil(10_000) { ui.onAllNodesWithText("WikDict · Offline").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithText("book", substring = false).assertExists()
        ui.onNodeWithText("WikDict · Offline").assertExists()
        screenshot("reflow-card")
        ui.onNodeWithText("Close dictionary").performClick()
        ui.onNodeWithContentDescription("Clear selection").performClick()
        ui.onNodeWithTag("row:0:0", useUnmergedTree = true).performTouchInput { longClick(Offset(24f, 12f)) }
        ui.onNodeWithContentDescription("Dictionary").performClick()
        ui.waitUntil(10_000) { ui.onAllNodesWithText("Close dictionary").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithTag("dictionaryFrom").performClick(); ui.onNodeWithTag("language:ko").performClick()
        assertEquals("en", OfflineDictionary.options.value.target)
        ui.onNodeWithTag("dictionaryAutomatic").performClick()
        ui.activityRule.scenario.recreate()
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("dictionaryFrom").fetchSemanticsNodes().isNotEmpty() }
        assertEquals("ko", OfflineDictionary.options.value.source); assertTrue(OfflineDictionary.options.value.automatic)
        assertEquals("ko", ui.activity.getSharedPreferences("dictionary", 0).getString("source", null))
    }
    @Test fun automaticCardAndPdfDoubleTapRespectSourceSelections() {
        install(); OfflineDictionary.configure(DictionaryOptions(automatic = true)); open()
        ui.onNodeWithTag("row:0:0", useUnmergedTree = true).performTouchInput { longClick(Offset(24f, 12f)) }
        ui.waitUntil(10_000) { ui.onAllNodesWithText("WikDict · Offline").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithText("Close dictionary").assertDoesNotExist()
        ui.onNodeWithContentDescription("Back to library").performClick(); open(pdf = true)
        val book = loadLibrary().books.first { it.fingerprint == imported.last() }
        val bounds = openPdfDocument(book.path).use { native ->
            val text = native.text(1u, 1000u); val byte = text.text.substringBefore("boats").toByteArray().size.toUInt()
            requireNotNull(text.glyphs.first { it.start <= byte && it.end > byte }.bounds)
        }
        ui.onNodeWithTag("pdfPage", useUnmergedTree = true).performTouchInput {
            doubleClick(Offset(width * (bounds.left + bounds.right) / 2, height * (bounds.top + bounds.bottom) / 2))
        }
        ui.waitUntil(10_000) { ui.onAllNodesWithText("Close dictionary").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithText("WikDict · Offline").assertExists()
        screenshot("pdf-card")
        ui.onNodeWithTag("pageLabel").assert(readerPageMatcher("Page 1", substring = true))
    }
}
