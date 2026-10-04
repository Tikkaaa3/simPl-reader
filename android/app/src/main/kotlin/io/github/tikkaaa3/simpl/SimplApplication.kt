package io.github.tikkaaa3.simpl

import android.app.Application
import io.github.tikkaaa3.simpl.core.initialize
import java.util.Locale

class SimplApplication : Application() {
    /** Why the Rust core could not start, or null when it is ready. */
    var coreFailure: String? = null
        private set

    override fun onCreate() {
        super.onCreate()
        ReadingControls.initialize(this)
        // Library and reading state live in private files; conversions are disposable.
        coreFailure =
            try {
                initialize(filesDir.absolutePath, cacheDir.absolutePath, Locale.getDefault().toLanguageTag())
                OfflineDictionary.initialize(this)
                null
            } catch (error: Throwable) {
                userError(error, FailureAction.Read)
            }
        OfflineDictionary.coreReady(coreFailure)
    }
}
