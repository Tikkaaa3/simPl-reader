package io.github.tikkaaa3.simpl

import android.content.Context
import android.view.KeyEvent
import android.view.WindowManager
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.*
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.core.content.edit
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

internal data class ReaderPreferences(val volumeTurns: Boolean = false, val keepScreenOn: Boolean = false)
internal object ReadingControls {
    private val mutable = MutableStateFlow(ReaderPreferences())
    val state = mutable.asStateFlow()
    fun initialize(context: Context) {
        val prefs = context.getSharedPreferences("controls", 0)
        mutable.value = ReaderPreferences(prefs.getBoolean("volumeTurns", false), prefs.getBoolean("keepScreenOn", false))
    }
    fun update(context: Context, volumeTurns: Boolean = mutable.value.volumeTurns, keepScreenOn: Boolean = mutable.value.keepScreenOn) {
        context.getSharedPreferences("controls", 0).edit { putBoolean("volumeTurns", volumeTurns); putBoolean("keepScreenOn", keepScreenOn) }
        mutable.value = ReaderPreferences(volumeTurns, keepScreenOn)
    }
}
internal enum class ReaderCommand { Previous, Next, ScrollUp, ScrollDown, First, Last, Find, Jump, ZoomIn, ZoomOut, ResetZoom, Fit, Contents, PreviousChapter, NextChapter, Return, Copy, SelectAll, Speech, Highlight, Bookmark, Panel, Toolbar, Help, Close, Escape }
internal fun readerCommand(event: KeyEvent, editing: Boolean, volume: Boolean): ReaderCommand? {
    val key = event.keyCode
    if (key == KeyEvent.KEYCODE_ESCAPE) return ReaderCommand.Escape
    if (event.isCtrlPressed && key == KeyEvent.KEYCODE_F && !event.isShiftPressed) return ReaderCommand.Find
    if (editing) return null
    if (event.isCtrlPressed) return when (key) {
        KeyEvent.KEYCODE_MOVE_HOME -> ReaderCommand.First
        KeyEvent.KEYCODE_MOVE_END -> ReaderCommand.Last
        KeyEvent.KEYCODE_L -> ReaderCommand.Jump
        KeyEvent.KEYCODE_EQUALS, KeyEvent.KEYCODE_PLUS, KeyEvent.KEYCODE_NUMPAD_ADD -> ReaderCommand.ZoomIn
        KeyEvent.KEYCODE_MINUS, KeyEvent.KEYCODE_NUMPAD_SUBTRACT -> ReaderCommand.ZoomOut
        KeyEvent.KEYCODE_0, KeyEvent.KEYCODE_NUMPAD_0 -> ReaderCommand.ResetZoom
        KeyEvent.KEYCODE_F -> if (event.isShiftPressed) ReaderCommand.Fit else ReaderCommand.Find
        KeyEvent.KEYCODE_T -> ReaderCommand.Contents
        KeyEvent.KEYCODE_PAGE_UP -> ReaderCommand.PreviousChapter
        KeyEvent.KEYCODE_PAGE_DOWN -> ReaderCommand.NextChapter
        KeyEvent.KEYCODE_C -> ReaderCommand.Copy
        KeyEvent.KEYCODE_A -> ReaderCommand.SelectAll
        KeyEvent.KEYCODE_U -> if (event.isShiftPressed) ReaderCommand.Speech else null
        KeyEvent.KEYCODE_H -> ReaderCommand.Highlight
        KeyEvent.KEYCODE_D -> ReaderCommand.Bookmark
        KeyEvent.KEYCODE_B -> ReaderCommand.Panel
        KeyEvent.KEYCODE_W -> ReaderCommand.Close
        else -> null
    }
    if (event.isAltPressed) return if (key == KeyEvent.KEYCODE_DPAD_LEFT) ReaderCommand.Return else null
    return when (key) {
        KeyEvent.KEYCODE_DPAD_LEFT -> ReaderCommand.Previous
        KeyEvent.KEYCODE_DPAD_RIGHT -> ReaderCommand.Next
        KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_PAGE_UP -> ReaderCommand.ScrollUp
        KeyEvent.KEYCODE_DPAD_DOWN, KeyEvent.KEYCODE_PAGE_DOWN -> ReaderCommand.ScrollDown
        KeyEvent.KEYCODE_VOLUME_UP -> if (volume) ReaderCommand.Previous else null
        KeyEvent.KEYCODE_VOLUME_DOWN -> if (volume) ReaderCommand.Next else null
        KeyEvent.KEYCODE_F8, KeyEvent.KEYCODE_F11 -> ReaderCommand.Toolbar
        KeyEvent.KEYCODE_F1 -> ReaderCommand.Help
        else -> null
    }
}
internal val LocalQuickSwitch = staticCompositionLocalOf<() -> Unit> { {} }
internal val LocalLibraryFocus = staticCompositionLocalOf<(Boolean) -> Unit> { {} }
internal val LocalReaderScroll = staticCompositionLocalOf<Channel<Int>?> { null }

/** Registration follows the navigation entry's lifecycle, so settings keep normal keys. */
@Composable
internal fun ReaderKeys(enabled: Boolean, action: (ReaderCommand) -> Unit) {
    val activity = LocalActivity.current as MainActivity
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val prefs by ReadingControls.state.collectAsStateWithLifecycle()
    val callback by rememberUpdatedState(action)
    val ready by rememberUpdatedState(enabled)
    val volume by rememberUpdatedState(prefs.volumeTurns)
    DisposableEffect(activity, lifecycle) {
        val handler: (KeyEvent, Boolean) -> Boolean = { event, editing ->
            val command = readerCommand(event, editing, volume)
            if (ready && command != null) {
                if (event.repeatCount == 0 || command in listOf(ReaderCommand.Previous, ReaderCommand.Next, ReaderCommand.ScrollUp, ReaderCommand.ScrollDown)) callback(command)
                true
            } else false
        }
        val observer = LifecycleEventObserver { _, _ ->
            if (lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) activity.readerKeys = handler
            else if (activity.readerKeys === handler) activity.readerKeys = null
        }
        lifecycle.addObserver(observer)
        if (lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) activity.readerKeys = handler
        onDispose { lifecycle.removeObserver(observer); if (activity.readerKeys === handler) activity.readerKeys = null }
    }
    DisposableEffect(activity, lifecycle, prefs.keepScreenOn) {
        val owner = Any()
        fun clear() {
            if (activity.screenOnOwner === owner) {
                activity.screenOnOwner = null
                activity.window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            }
        }
        fun apply() {
            if (prefs.keepScreenOn && lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) {
                activity.screenOnOwner = owner
                activity.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            } else clear()
        }
        val observer = LifecycleEventObserver { _, _ -> apply() }
        lifecycle.addObserver(observer); apply()
        onDispose { lifecycle.removeObserver(observer); clear() }
    }
}

@Composable
internal fun rememberFindState(): FindState = rememberSaveable(saver = listSaver(
    save = { listOf(it.query, it.selected, it.previousQuery) }, restore = { FindState(it[0] as String, it[1] as Int, it[2] as String) })) { FindState() }

/** Owned by the screen, so moving between a sheet and a side pane retains search. */
internal class FindState(query: String = "", selected: Int = 0, previousQuery: String = "") {
    var query by mutableStateOf(query)
    var selected by mutableIntStateOf(selected)
    var previousQuery by mutableStateOf(previousQuery)
}

@Composable
internal fun FindPanel(state: FindState, start: suspend (String) -> FindTask, go: (SearchHit) -> Unit, dismiss: () -> Unit) {
    val query = state.query
    var results by remember { mutableStateOf<SearchResults?>(null) }
    var busy by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }
    val startNow by rememberUpdatedState(start)
    val goNow by rememberUpdatedState(go)
    val focus = remember { FocusRequester() }
    val manager = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    LaunchedEffect(Unit) { focus.requestFocus() }
    LaunchedEffect(query) {
        results = null; failure = null; busy = false
        if (state.previousQuery != query) { state.selected = 0; state.previousQuery = query }
        if (query.isBlank()) return@LaunchedEffect
        delay(250); busy = true
        var task: FindTask? = null
        try {
            // Keep ownership on cancellation even if the JNI call has already returned.
            withContext(Dispatchers.IO) { task = startNow(query) }
            while (withContext(Dispatchers.IO) { task!!.status() } == LayoutStatus.RUNNING) delay(50)
            val found = withContext(Dispatchers.IO) { task!!.result() }
            results = found
            if (!found?.hits.isNullOrEmpty()) {
                state.selected = state.selected.coerceIn(0, found.hits.lastIndex)
                goNow(found.hits[state.selected])
            }
        } catch (error: CancellationException) { throw error }
        catch (error: Exception) { failure = userError(error, FailureAction.Read) }
        finally { task?.cancel(); task?.close(); busy = false }
    }
    fun choose(index: Int) {
        val hits = results?.hits.orEmpty()
        if (hits.isEmpty()) return
        state.selected = (index + hits.size) % hits.size
        manager.clearFocus(); keyboard?.hide(); goNow(hits[state.selected])
    }
    Column(Modifier.fillMaxWidth().testTag("findPanel")) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text("Find in book", Modifier.padding(12.dp), style = MaterialTheme.typography.headlineSmall)
            TextButton(onClick = dismiss, colors = quietButtonColors(), shape = ControlShape) { Text("Close search") }
        }
        OutlinedTextField(query, { state.query = it }, singleLine = true, label = { Text("Search text") },
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp).focusRequester(focus).testTag("findQuery"),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search), keyboardActions = KeyboardActions(onSearch = { choose(state.selected) }))
        if (busy) LinearProgressIndicator(Modifier.fillMaxWidth().testTag("findProgress"))
        failure?.let { Text(it, Modifier.padding(12.dp), color = MaterialTheme.colorScheme.error) }
        results?.let { found ->
            Text(if (found.hits.isEmpty()) "No matches" else "${state.selected + 1} of ${found.hits.size}${if (found.limited) "+" else ""} matches", Modifier.padding(12.dp).testTag("findCount"))
            Row {
                TextButton(onClick = { choose(state.selected - 1) }, enabled = found.hits.isNotEmpty(), colors = quietButtonColors(), shape = ControlShape) { Text("Previous match") }
                TextButton(onClick = { choose(state.selected + 1) }, enabled = found.hits.isNotEmpty(), colors = quietButtonColors(), shape = ControlShape) { Text("Next match") }
            }
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = 300.dp).testTag("findResults")) {
                itemsIndexed(found.hits) { index, hit -> TextButton(onClick = { choose(index) }, modifier = Modifier.fillMaxWidth().testTag("findHit:$index"),
                    colors = quietButtonColors(), shape = ControlShape) {
                    Column(Modifier.fillMaxWidth()) {
                        Text("Page ${hit.page}", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Text(hit.excerpt, maxLines = 3, style = MaterialTheme.typography.bodyMedium)
                    }
                } }
            }
        }
    }
}

@Composable
internal fun KeyboardHelp(dismiss: () -> Unit) {
    AlertDialog(onDismissRequest = dismiss, title = { Text("Keyboard shortcuts") }, text = { LazyColumn {
        itemsIndexed(listOf("Ctrl+O — Import books", "Ctrl+K / Ctrl+R — Switch book / recent books", "Ctrl+W — Library", "Space — Resume in library (no control focused)", "Left / Right — Previous / next page", "Up / Down / Page Up / Page Down — Scroll", "Ctrl+Home / End — First / last page", "Ctrl+F — Find in book", "Ctrl+L — Go to page", "Ctrl+T — Contents", "Ctrl+Page Up / Down — EPUB chapter", "Alt+Left — Return from link", "Ctrl+Plus / Minus / 0 — Zoom / reset", "Ctrl+mouse wheel — Zoom", "Ctrl+Shift+F — Fit width / previous zoom", "Ctrl+A / C — Select page text / copy", "Ctrl+Shift+U — Read aloud / stop", "Ctrl+H — Highlight selection", "Ctrl+D — Bookmark page", "Ctrl+B — Annotations panel", "F8 / F11 — Hide or show controls and system bars", "F1 — This help", "Escape — Dismiss / return", "Tab / Shift+Tab — Move focus")) { _, text -> Text(text, Modifier.padding(vertical = 4.dp)) }
    } }, confirmButton = { TextButton(onClick = dismiss) { Text("Done") } })
}
