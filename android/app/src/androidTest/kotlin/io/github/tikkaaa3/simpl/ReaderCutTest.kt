package io.github.tikkaaa3.simpl

import androidx.activity.compose.setContent
import androidx.compose.material3.Text
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ReaderCutTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()

    @Test fun canonicalCutsCoverEveryComposeLineExactlyOnceAcrossThemes() {
        lateinit var measurer: TextMeasurer
        ui.activityRule.scenario.onActivity { activity -> activity.setContent {
            SimplTheme(Appearance.Light) { val value = rememberTextMeasurer(64); SideEffect { measurer = value }; Text("Measuring reader pages") }
        } }
        ui.waitForIdle()
        readerIllustration(ui.activity.cacheDir)
        val file = File(ui.activity.cacheDir, "M4-cut.html").apply { writeText(readerHtml("Cut coverage")) }
        openBook(file.absolutePath).use { task ->
            waitLayout(task::status)
            requireNotNull(task.result()).use { book ->
                val total = book.readerInfo().total
                assertTrue(total > 5u)
                val layouts = listOf(
                    "default" to LayoutOptions(ReadingFont.THEME, 20u, 48u, 0u),
                    "soft" to LayoutOptions(ReadingFont.SPECTRAL, 26u, 64u, 180u),
                    "clear" to LayoutOptions(ReadingFont.FIRA_SANS, 36u, 96u, 220u),
                    "compact" to LayoutOptions(ReadingFont.LITERATA, 12u, 16u, 110u))
                for ((theme, options) in layouts) book.adapt(theme, options).use { adaptation ->
                    waitLayout(adaptation::status)
                    requireNotNull(adaptation.result()).use { adapted ->
                        val expected = linkedMapOf<Pair<UInt, UInt>, String>()
                        val actual = linkedMapOf<Pair<UInt, UInt>, StringBuilder>()
                        for (page in 1u..total) {
                            val content = adapted.page(page)
                            content.forEach { p -> p.rows.filter { it.text != null }.forEach { row -> expected[p.section to row.index] = row.text!! } }
                            ui.runOnIdle {
                                measurePage(content, options, measurer, Color.Black, Color.Blue).forEach { row ->
                                    row.text?.let { layout ->
                                        val key = row.section to row.row.index
                                        val slice = row.row.text!!.substring(layout.getLineStart(row.firstLine), layout.getLineEnd(row.lastLine - 1))
                                        actual.getOrPut(key) { StringBuilder() }.append(slice)
                                    }
                                }
                            }
                        }
                        expected.forEach { (key, text) -> assertEquals("$theme row $key", text, actual[key].toString()) }
                    }
                }
            }
        }
        assertEquals(2, byteIndex("🛶é", 4u)); assertEquals(3, byteIndex("🛶é", 6u)); assertEquals(0, byteIndex("🛶é", 2u))
    }
}

internal fun waitLayout(status: () -> LayoutStatus) {
    val end = android.os.SystemClock.uptimeMillis() + 30_000
    while (status() == LayoutStatus.RUNNING) { check(android.os.SystemClock.uptimeMillis() < end); Thread.sleep(10) }
    assertEquals(LayoutStatus.COMPLETE, status())
}
