@file:OptIn(androidx.benchmark.macro.ExperimentalMetricApi::class)

package io.github.tikkaaa3.simpl.baselineprofile

import android.content.Intent
import android.net.Uri
import androidx.benchmark.macro.*
import androidx.benchmark.macro.junit4.BaselineProfileRule
import androidx.benchmark.macro.junit4.MacrobenchmarkRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

private const val PACKAGE = "io.github.tikkaaa3.simpl"
private val BOOK = Uri.parse("content://io.github.tikkaaa3.simpl.benchmark.books/reader")

private fun bookIntent(): Intent {
    InstrumentationRegistry.getInstrumentation().context.grantUriPermission(PACKAGE, BOOK, Intent.FLAG_GRANT_READ_URI_PERMISSION)
    return Intent(Intent.ACTION_VIEW).setDataAndType(BOOK, "text/html").setClassName(PACKAGE, "$PACKAGE.MainActivity")
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION)
}

private fun device() = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())

private fun awaitPage(): String {
    val device = device()
    check(device.wait(Until.hasObject(By.textStartsWith("Page ")), 30_000)) { "Reader did not finish loading" }
    return device.findObject(By.textStartsWith("Page ")).text
}

private fun nextPage() {
    val before = awaitPage()
    device().findObject(By.text("Next")).click()
    check(device().wait(Until.gone(By.text(before)), 10_000)) { "Page did not turn" }
    awaitPage()
    device().waitForIdle()
}

private fun resetPage() {
    val label = awaitPage()
    if (label.startsWith("Page 1 of ")) return
    device().findObject(By.text(label)).click()
    check(device().wait(Until.hasObject(By.clazz("android.widget.EditText")), 5_000))
    device().findObject(By.clazz("android.widget.EditText")).text = "1"
    device().findObject(By.text("Go")).click()
    check(device().wait(Until.hasObject(By.textStartsWith("Page 1 of ")), 10_000))
}

@RunWith(AndroidJUnit4::class)
class BaselineProfileGenerator {
    @get:Rule val rule = BaselineProfileRule()

    @Test fun startup() = rule.collect(PACKAGE, includeInStartupProfile = true) {
        pressHome()
        startActivityAndWait()
        check(device().wait(Until.hasObject(By.desc("Settings")), 10_000))
    }

    @Test fun readerAndSettings() = rule.collect(PACKAGE) {
        startActivityAndWait(bookIntent())
        resetPage()
        repeat(3) { nextPage() }
        device().findObject(By.desc("Bookmark page")).click()
        device().findObject(By.desc("Annotations")).click()
        check(device().wait(Until.hasObject(By.text("Bookmarks")), 10_000))
        device().pressBack()
        device().findObject(By.desc("Settings")).click()
        check(device().wait(Until.hasObject(By.text("Appearance")), 10_000))
        device().pressBack()
    }
}

@RunWith(AndroidJUnit4::class)
class ReaderBenchmarks {
    @get:Rule val rule = MacrobenchmarkRule()

    @Test fun coldStartup() = rule.measureRepeated(
        packageName = PACKAGE, metrics = listOf(StartupTimingMetric()), iterations = 5,
        startupMode = StartupMode.COLD, compilationMode = CompilationMode.Partial(BaselineProfileMode.Require),
        setupBlock = { pressHome() },
    ) { startActivityAndWait() }

    @Test fun pageTurn() = rule.measureRepeated(
        packageName = PACKAGE, metrics = listOf(FrameTimingMetric(), TraceSectionMetric("simPl.pageContent")), iterations = 5,
        compilationMode = CompilationMode.Partial(BaselineProfileMode.Require),
        setupBlock = { startActivityAndWait(bookIntent()); resetPage() },
    ) { nextPage() }
}
