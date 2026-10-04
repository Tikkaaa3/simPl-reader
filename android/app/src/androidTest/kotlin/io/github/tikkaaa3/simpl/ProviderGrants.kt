package io.github.tikkaaa3.simpl

import android.content.ComponentName
import android.content.Intent
import android.provider.DocumentsContract
import androidx.test.platform.app.InstrumentationRegistry

internal fun grantFixtureDocuments() {
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    instrumentation.targetContext.startActivity(Intent().apply {
        component = ComponentName("io.github.tikkaaa3.simpl.test", "io.github.tikkaaa3.simpl.GrantFixtureActivity")
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    })
    val uri = DocumentsContract.buildDocumentUri("io.github.tikkaaa3.simpl.test.documents", "Notes.txt")
    val deadline = android.os.SystemClock.uptimeMillis() + 10_000
    while (true) {
        if (runCatching { instrumentation.targetContext.contentResolver.query(uri, null, null, null, null)?.use { it.moveToFirst() } }.getOrDefault(false) == true) break
        check(android.os.SystemClock.uptimeMillis() < deadline) { "Test picker did not grant access" }
        Thread.sleep(20)
    }
    instrumentation.waitForIdleSync()
}
