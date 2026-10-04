@file:androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)

package io.github.tikkaaa3.simpl

import android.content.ComponentName
import android.content.Intent
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.SystemClock
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.util.concurrent.TimeUnit
import org.junit.*
import org.junit.Assert.*
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ReadAloudTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private var originalEngine = "null"
    private val entries = mutableListOf<LibraryBook>()

    private fun shell(command: String): String = instrumentation.uiAutomation.executeShellCommand(command).use {
        android.os.ParcelFileDescriptor.AutoCloseInputStream(it).bufferedReader().readText().trim()
    }
    @Before fun setup() {
        originalEngine = shell("settings get secure tts_default_synth")
        shell("settings put secure tts_default_synth io.github.tikkaaa3.simpl.test")
        ui.runOnUiThread { ReadAloud.stop() }
        Thread.sleep(250)
    }
    @After fun cleanup() {
        shell("input keyevent KEYCODE_WAKEUP")
        ui.runOnUiThread { ReadAloud.stop() }
        Thread.sleep(250)
        if (originalEngine == "null") shell("settings delete secure tts_default_synth")
        else shell("settings put secure tts_default_synth $originalEngine")
        ui.activityRule.scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }
        entries.forEach { runCatching { removeLibraryBook(it.fingerprint) } }
        ui.activity.getSharedPreferences("speech", 0).edit().clear().commit()
    }
    private fun book(text: String, extension: String = "html"): LibraryBook {
        val file = File(ui.activity.cacheDir, "p1-${SystemClock.elapsedRealtimeNanos()}.$extension")
        if (extension == "epub") file.writeBytes(readerEpub()) else file.writeText(text)
        val entry = importLibraryBook(file.absolutePath); entries += entry
        // Removing a catalog entry deliberately retains its saved position.
        // These repeatable fixtures must begin at the first source page.
        val opening = openBook(entry.path)
        try {
            await("fixture opening") { opening.status() != LayoutStatus.RUNNING }
            opening.result()!!.use { it.saveLocation(ReaderLocation(1u, 0u, 0u, 0f), it.readerInfo().options.size) }
        } finally { opening.close() }
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].apply { query(""); filter("all"); reload(); open(entry) } }
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("speechStart").fetchSemanticsNodes().isNotEmpty() }
        return entry
    }
    private fun await(message: String, timeout: Long = 30_000, condition: () -> Boolean) {
        val deadline = SystemClock.elapsedRealtime() + timeout
        while (!condition()) {
            val state = ReadAloud.state.value
            assertTrue("$message: active=${state.active}, preparing=${state.preparing}, error=${state.error}", SystemClock.elapsedRealtime() < deadline)
            Thread.sleep(30)
        }
    }

    @Test fun platformTtsHighlightsUnicodePausesResumesAndChangesVoiceAndSpeed() {
        book("<p>${"Café 😀 İstanbul and the boats returned before dawn. ".repeat(1000)}</p>")
        ui.onNodeWithTag("speechStart").performClick()
        await("word timings") { ReadAloud.state.value.range?.from?.byte?.let { it > 30u } == true && !ReadAloud.state.value.preparing }
        ui.onNodeWithTag("speechPause").performClick()
        val paused = ReadAloud.state.value.range
        assertFalse(ReadAloud.state.value.playing)
        Thread.sleep(250); assertEquals(paused, ReadAloud.state.value.range)
        ui.waitUntil(10_000) { ui.onAllNodesWithTag("row:0:0", useUnmergedTree = true).fetchSemanticsNodes().any {
            it.config.getOrElse(SemanticsProperties.StateDescription) { "" } == "Reading aloud"
        } }
        val image = ui.onNodeWithTag("row:0:0", useUnmergedTree = true).captureToImage().asAndroidBitmap()
        var bluePixels = 0
        for (y in 0 until image.height step 2) for (x in 0 until image.width step 2) {
            val color = image.getPixel(x, y)
            if (android.graphics.Color.blue(color) - android.graphics.Color.red(color) > 20 &&
                android.graphics.Color.green(color) - android.graphics.Color.red(color) > 10) bluePixels++
        }
        assertTrue("The retained word must be painted inside the visible text", bluePixels > 15)
        screenshot("reflow-word")
        ui.runOnUiThread { ReadAloud.options("p1-test-en", 1.25f) }
        assertEquals("p1-test-en", ui.activity.getSharedPreferences("speech", 0).getString("voice", ""))
        ui.activityRule.scenario.recreate()
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("speechPause").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithTag("speechPause").performClick()
        await("resume from retained word") { ReadAloud.state.value.range?.from?.byte?.let { it >= paused!!.from.byte } == true && !ReadAloud.state.value.preparing }
        ui.onNodeWithTag("speechStop").performClick()
        await("stop") { !ReadAloud.state.value.active && ReadAloud.state.value.range == null }
    }

    @Test fun continuesAcrossCanonicalSheetsAndEpubSections() {
        book("", "epub")
        ui.onNodeWithTag("speechStart").performClick()
        await("EPUB next section") { ReadAloud.state.value.chunk?.source?.section == 1u }
        await("end of book") { !ReadAloud.state.value.active }
        ui.onNodeWithContentDescription("Back to library").performClick()
        book("<p>${"A quiet boat came home. ".repeat(1200)}</p>")
        ui.onNodeWithTag("speechStart").performClick()
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pageLabel").fetchSemanticsNodes().any {
            it.config[SemanticsProperties.Text].any { label -> !label.text.startsWith("Page 1 of") }
        } }
        assertTrue(ReadAloud.state.value.active)
    }

    @Test fun sessionTransportAudioFocusAndScreenOffPlaybackUseTheRealService() {
        book("<p>${"The words continue while the screen sleeps. ".repeat(1000)}</p>")
        ui.onNodeWithTag("speechStart").performClick()
        await("engine ready") { ReadAloud.state.value.range != null && !ReadAloud.state.value.preparing }
        val before = ReadAloud.state.value.range!!.from.byte
        shell("input keyevent KEYCODE_SLEEP")
        await("screen-off playback advances") { (ReadAloud.state.value.range?.from?.byte ?: 0u) > before + 30u }
        val serviceDump = shell("dumpsys activity services io.github.tikkaaa3.simpl")
        assertTrue(serviceDump, serviceDump.contains("isForeground=true"))
        val sessionDump = shell("dumpsys media_session")
        assertTrue(sessionDump, sessionDump.contains("package=io.github.tikkaaa3.simpl"))
        shell("input keyevent KEYCODE_WAKEUP")
        shell("input keyevent KEYCODE_MEDIA_PAUSE")
        await("headset media pause") { !ReadAloud.state.value.playing }
        shell("input keyevent KEYCODE_MEDIA_PLAY")
        await("headset media play") { ReadAloud.state.value.playing }
        val future = MediaController.Builder(ui.activity, SessionToken(ui.activity, ComponentName(ui.activity, ReadAloudService::class.java))).buildAsync()
        val controller = future.get(10, TimeUnit.SECONDS)
        try {
            ui.runOnUiThread { controller.pause() }
            await("media session pause") { !ReadAloud.state.value.playing }
            ui.runOnUiThread { controller.play() }
            await("media session play") { ReadAloud.state.value.playing }
            val manager = ui.activity.getSystemService(AudioManager::class.java)
            val focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
                .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).build()).build()
            ui.runOnUiThread { manager.requestAudioFocus(focus) }
            await("audio focus interruption") { !ReadAloud.state.value.playing }
            ui.runOnUiThread { manager.abandonAudioFocusRequest(focus) }
            await("audio focus return") { ReadAloud.state.value.playing }
            ui.runOnUiThread { controller.stop() }
            await("media session stop") { !ReadAloud.state.value.active }
        } finally { ui.runOnUiThread { controller.release() } }
    }

    @Test fun passageDoesNotTurnPagesAndRestartRejectsOldEngineCallbacks() {
        val entry = book("<p>${"A long first passage for reading. ".repeat(800)}</p>")
        ui.onNodeWithTag("speechStart").performClick()
        await("first session") { !ReadAloud.state.value.preparing && ReadAloud.state.value.range != null }
        val passage = speechPassage("A selected 😀 passage with several words.")
        ui.runOnUiThread { ReadAloud.start(ui.activity, entry.fingerprint, entry.title, passage, passage = true) }
        await("new passage") { ReadAloud.state.value.chunk?.text?.startsWith("A selected") == true }
        await("passage finishes") { !ReadAloud.state.value.active }
        assertNull(ReadAloud.state.value.range)
        ui.onNodeWithTag("pageLabel").assertTextContains("Page 1 of", substring = true)
    }

    @Test fun nativePlansRetainTheirDocumentAndSkipTextlessPdfPages() {
        val file = File(ui.activity.cacheDir, "p1-plan.epub").apply { writeBytes(readerEpub()) }
        val task = openBook(file.absolutePath)
        await("native opening") { task.status() != LayoutStatus.RUNNING }
        val opened = task.result()!!; task.close()
        val plan = opened.speechPlan(SourcePoint(0u, 0u, 0u)); opened.close()
        plan.use {
            val chunks = generateSequence { it.next() }.toList()
            assertTrue(chunks.any { c -> c.source.section == 1u })
            assertFalse(chunks.any { c -> c.text.contains("supplementary") })
        }
        val pdfFile = File(ui.activity.cacheDir, "p1-blank.pdf")
        val pdf = android.graphics.pdf.PdfDocument()
        try {
            for (number in 1..3) {
                val page = pdf.startPage(android.graphics.pdf.PdfDocument.PageInfo.Builder(300, 200, number).create())
                if (number != 2) page.canvas.drawText("Page $number text", 20f, 80f, android.graphics.Paint().apply { textSize = 18f })
                pdf.finishPage(page)
            }
            pdfFile.outputStream().use(pdf::writeTo)
        } finally { pdf.close() }
        openPdfDocument(pdfFile.absolutePath).use { pdf ->
            pdf.speechPlan(1u).use { speech ->
                assertEquals(1u, speech.next()!!.pdfPage)
                assertEquals(3u, speech.next()!!.pdfPage)
                assertNull(speech.next())
            }
        }
        val restricted = File(ui.activity.cacheDir, "p1-restricted.pdf")
        instrumentation.context.assets.open("copy-restricted.pdf").use { input -> restricted.outputStream().use(input::copyTo) }
        openPdfDocument(restricted.absolutePath).use { document ->
            assertFalse(document.info().canCopy)
            try { document.speechPlan(1u); fail("Restricted PDFs must not expose speech text") }
            catch (expected: CoreException.Failed) { assertTrue(expected.reason.contains("does not permit copying")) }
        }
    }

    @Test fun selectionAndHighlightActionsReadOnlyTheirPassage() {
        val entry = book("<p>${"The harbour has boats and quiet lights. ".repeat(200)}</p>")
        ui.onNodeWithTag("row:0:0", useUnmergedTree = true).performTouchInput { longClick(androidx.compose.ui.geometry.Offset(24f, 12f)) }
        ui.waitUntil(10_000) { ui.onAllNodesWithText("Read selection").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithText("Read selection").performClick()
        await("selection starts") { ReadAloud.state.value.passage && ReadAloud.state.value.chunk?.text == "The" }
        ui.runOnUiThread { ReadAloud.stop() }
        ui.onNodeWithContentDescription("Highlight yellow").performClick()
        ui.waitUntil(10_000) { loadAnnotations(entry.fingerprint).highlights.any { it.quote == "The" } }
        ui.onNodeWithContentDescription("Annotations").performClick()
        ui.onNodeWithText("Highlights").performClick()
        val readHighlight = hasText("Read aloud") and hasAnyAncestor(hasTestTag("annotationList"))
        ui.waitUntil(10_000) { ui.onAllNodes(readHighlight).fetchSemanticsNodes().isNotEmpty() }
        ui.onNode(readHighlight).performClick()
        await("highlight starts") { ReadAloud.state.value.passage && ReadAloud.state.value.chunk?.text == "The" }
    }

    @Test fun pdfSpeechFollowsPhysicalPagesAndReadsSelectionWithoutMovingTheView() {
        val file = File(ui.activity.cacheDir, "p1-speech.pdf").apply { writeBytes(pdfFixture(3, 12, "speech")) }
        val entry = importLibraryBook(file.absolutePath); entries += entry
        openPdfDocument(entry.path).use { it.saveLocation(PdfLocation(1u, 0f, 0f, 1f, true)) }
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].open(entry) }
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("speechStart").fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithTag("speechStart").performClick()
        await("PDF page two") { ReadAloud.state.value.range?.let { it.pdfPage == 2u && it.from.byte > 10u } == true && !ReadAloud.state.value.preparing }
        // Freeze the real audio clock immediately; a Compose idle wait can let
        // this short fixture reach page three before the pause click is delivered.
        // The shared pause button is exercised by the platform TTS test above.
        ui.runOnUiThread { ReadAloud.pause() }
        assertEquals(2u, ReadAloud.state.value.range?.pdfPage)
        ui.waitUntil(30_000) { ui.onAllNodesWithTag("pdfPage").fetchSemanticsNodes().any {
            it.config[SemanticsProperties.StateDescription] == "Reading aloud"
        } }
        ui.onNodeWithTag("pageLabel").assertTextContains("Page 2 of 3", substring = true)
        screenshot("pdf-word")
        ui.onNodeWithTag("speechStop").performClick()
        ui.onNodeWithText("Select page text").performClick()
        ui.onNodeWithText("Read selection").performClick()
        await("PDF passage") { ReadAloud.state.value.passage && ReadAloud.state.value.chunk?.text?.contains("page 2") == true }
        ui.onNodeWithTag("pageLabel").assertTextContains("Page 2 of 3", substring = true)
    }

    @Test fun installedPlatformEngineAlsoSynthesizesPlayableAudio() {
        val engines = ui.activity.packageManager.queryIntentServices(Intent(android.speech.tts.TextToSpeech.Engine.INTENT_ACTION_TTS_SERVICE), 0)
            .map { it.serviceInfo.packageName }.filter { it != "io.github.tikkaaa3.simpl.test" }
        Assume.assumeTrue("A production TTS engine must be installed for this additional smoke test", engines.isNotEmpty())
        val selected = originalEngine.takeIf { it in engines } ?: engines.first()
        shell("settings put secure tts_default_synth $selected")
        book("<p>${"The boats returned to the quiet harbour before dawn. ".repeat(200)}</p>")
        ui.onNodeWithTag("speechStart").performClick()
        await("installed engine audio output", 60_000) { ReadAloud.state.value.range != null && !ReadAloud.state.value.preparing }
        shell("input keyevent KEYCODE_MEDIA_PAUSE")
        await("installed engine headset pause") { !ReadAloud.state.value.playing }
        assertTrue(ReadAloud.state.value.active)
        assertTrue(ReadAloud.state.value.voices.none { it.id == "p1-test-en" })
    }

    private fun screenshot(name: String) {
        ui.waitForIdle()
        val resolver = ui.activity.contentResolver
        val uri = requireNotNull(resolver.insert(android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI, android.content.ContentValues().apply {
            put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, "$name.png")
            put(android.provider.MediaStore.MediaColumns.MIME_TYPE, "image/png")
            put(android.provider.MediaStore.MediaColumns.RELATIVE_PATH, "Pictures/simPl-P1")
        }))
        resolver.openOutputStream(uri)!!.use { ui.onRoot().captureToImage().asAndroidBitmap().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
    }
}
