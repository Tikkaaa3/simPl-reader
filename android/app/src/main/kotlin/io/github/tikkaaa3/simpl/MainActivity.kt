package io.github.tikkaaa3.simpl

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.core.content.IntentCompat
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.getValue

class MainActivity : ComponentActivity() {
    internal var readerKeys: ((android.view.KeyEvent, Boolean) -> Boolean)? = null
    internal var appKeys: ((android.view.KeyEvent) -> Boolean)? = null
    internal var screenOnOwner: Any? = null
    private val consumedKeys = mutableSetOf<Int>()

    private fun handleKeyEvent(event: android.view.KeyEvent): Boolean {
        if (event.action == android.view.KeyEvent.ACTION_UP && consumedKeys.remove(event.keyCode)) return true
        if (event.action == android.view.KeyEvent.ACTION_DOWN) {
            val editing = getSystemService(android.view.inputmethod.InputMethodManager::class.java).isAcceptingText
            if (appKeys?.invoke(event) == true || readerKeys?.invoke(event, editing) == true) {
                consumedKeys.add(event.keyCode); return true
            }
        }
        return false
    }

    override fun onPause() { consumedKeys.clear(); super.onPause() }
    override fun dispatchGenericMotionEvent(event: android.view.MotionEvent): Boolean {
        if (event.action == android.view.MotionEvent.ACTION_SCROLL && (event.metaState and android.view.KeyEvent.META_CTRL_MASK) != 0) {
            val delta = event.getAxisValue(android.view.MotionEvent.AXIS_VSCROLL)
            val editing = getSystemService(android.view.inputmethod.InputMethodManager::class.java).isAcceptingText
            if (delta != 0f && readerKeys?.invoke(android.view.KeyEvent(0, 0, android.view.KeyEvent.ACTION_DOWN,
                    if (delta > 0) android.view.KeyEvent.KEYCODE_PLUS else android.view.KeyEvent.KEYCODE_MINUS, 0, android.view.KeyEvent.META_CTRL_ON), editing) == true) return true
        }
        return super.dispatchGenericMotionEvent(event)
    }
    private val library: LibraryViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        installSplashScreen()
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val original = window.callback
        window.callback = object : android.view.Window.Callback by original {
            override fun dispatchKeyEvent(event: android.view.KeyEvent): Boolean = handleKeyEvent(event) || original.dispatchKeyEvent(event)
        }
        if (savedInstanceState == null) receive(intent)
        setContent {
            val state by library.state.collectAsStateWithLifecycle()
            SimplTheme(state.appearance) { FoldAware { SimplApp(state, library) } }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        receive(intent)
        setIntent(intent)
    }

    private fun receive(intent: Intent) {
        if (intent.action == ReadAloudService.OPEN_READER) {
            ReadAloud.state.value.fingerprint.takeIf { it.isNotBlank() }?.let(library::openSpokenBook)
            return
        }
        val uris = when (intent.action) {
            Intent.ACTION_VIEW -> listOfNotNull(intent.data)
            Intent.ACTION_SEND -> listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
            Intent.ACTION_SEND_MULTIPLE -> IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
            else -> emptyList()
        }.toMutableList()
        if (intent.action in listOf(Intent.ACTION_VIEW, Intent.ACTION_SEND, Intent.ACTION_SEND_MULTIPLE)) {
            intent.clipData?.let { clip -> repeat(clip.itemCount) { index -> clip.getItemAt(index).uri?.let(uris::add) } }
        }
        library.enqueue(uris.distinct(), openAfter = uris.distinct().size == 1)
    }
}
