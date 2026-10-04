@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.tikkaaa3.simpl

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.*
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.input.pointer.*
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlin.math.*

/** Build a bounded spatial index once; pointer movement does not scan every glyph. */
internal class PdfTextGeometry(private val layer: PdfPageText) {
    private val rows = Array(128) { ArrayList<Int>() }
    private val tall = ArrayList<Int>()
    init {
        layer.glyphs.forEachIndexed { index, glyph -> glyph.bounds?.let { r ->
            val first = (r.top * 128).toInt().coerceIn(0, 127)
            val last = (r.bottom * 128).toInt().coerceIn(first, 127)
            if (last - first > 4) tall += index else for (row in first..last) rows[row] += index
        } }
    }
    fun closest(point: Offset, width: Float, height: Float, distance: Float = Float.POSITIVE_INFINITY): Int? {
        val x = point.x / width; val y = point.y / height
        var best: Int? = null; var score = Float.POSITIVE_INFINITY
        fun consider(index: Int) {
            val r = layer.glyphs[index].bounds!!
            val dx = max(max(r.left - x, x - r.right), 0f) * width
            val dy = max(max(r.top - y, y - r.bottom), 0f) * height
            val candidate = dx * dx + dy * dy * 4
            if (candidate < score && dx <= distance && dy <= distance) { best = index; score = candidate }
        }
        tall.forEach(::consider)
        val center = (y * 128).toInt().coerceIn(0, 127)
        for (step in 0..127) {
            var searched = false
            for (row in if (step == 0) listOf(center) else listOf(center - step, center + step)) {
                if (row !in 0..127) continue
                val dy = max(max(row / 128f - y, y - (row + 1) / 128f), 0f) * height * 2
                if (dy * dy <= score) { rows[row].forEach(::consider); searched = true }
            }
            if (!searched) break
        }
        return best
    }
    fun word(index: Int): IntRange {
        val bytes = layer.text.toByteArray(Charsets.UTF_8)
        fun isWord(i: Int): Boolean {
            val g = layer.glyphs[i]
            return String(bytes, g.start.toInt(), (g.end - g.start).toInt(), Charsets.UTF_8).any { it.isLetterOrDigit() || it == '\'' || it == '’' }
        }
        var first = index; var last = index
        if (isWord(index)) {
            while (first > 0 && isWord(first - 1)) first--
            while (last < layer.glyphs.lastIndex && isWord(last + 1)) last++
        }
        return first..last
    }
}

@Composable
internal fun PdfScreen(book: LibraryBook, model: PdfViewModel, back: () -> Unit, settings: () -> Unit) {
    val state by model.state.collectAsStateWithLifecycle()
    LaunchedEffect(book.path) { model.open(book) }
    var toolbar by rememberSaveable { mutableStateOf(true) }
    var jump by rememberSaveable { mutableStateOf(false) }
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(state.selection, state.location.page) { copied = false }
    val activity = requireNotNull(LocalActivity.current)
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current
    val dark = MaterialTheme.colorScheme.surface.luminance() < .5f
    DisposableEffect(lifecycle, model) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_STOP) model.stop() }
        lifecycle.lifecycle.addObserver(observer)
        onDispose { lifecycle.lifecycle.removeObserver(observer); model.stop() }
    }
    DisposableEffect(toolbar, dark) {
        val controller = WindowCompat.getInsetsController(activity.window, activity.window.decorView)
        controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        if (toolbar) controller.show(WindowInsetsCompat.Type.systemBars()) else controller.hide(WindowInsetsCompat.Type.systemBars())
        controller.isAppearanceLightStatusBars = !dark; controller.isAppearanceLightNavigationBars = !dark
        onDispose { controller.show(WindowInsetsCompat.Type.systemBars()) }
    }
    BackHandler {
        when { state.selection != null -> model.select(null); jump -> jump = false;
            !toolbar -> toolbar = true; else -> { model.stop(); back() } }
    }
    Column(Modifier.fillMaxSize().testTag("reader")) {
        if (toolbar) TopAppBar(title = { Text(book.title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
            navigationIcon = { IconButton(onClick = { model.stop(); back() }) { Icon(AppIcons.Back, "Back to library") } },
            actions = { IconButton(onClick = settings) { Icon(AppIcons.Settings, "Settings") } })
        if (toolbar && !state.loading && state.info != null) Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("PDF", style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(horizontal = 8.dp))
            TextButton(onClick = model::fit) { Text("Fit width") }
            TextButton(onClick = model::selectAll, enabled = !state.text?.glyphs.isNullOrEmpty()) { Text("Select page text") }
        }
        if (state.selection != null) Row(Modifier.fillMaxWidth().testTag("pdfSelection"), verticalAlignment = Alignment.CenterVertically) {
            Text("Text selected", Modifier.weight(1f).padding(start = 16.dp), style = MaterialTheme.typography.labelLarge)
            TextButton(onClick = { model.copy { text ->
                (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("PDF text", text))
                copied = true
            } }, enabled = state.info?.canCopy == true) { Text(if (copied) "Copied" else "Copy") }
            TextButton(onClick = { model.select(null); copied = false }) { Text("Clear") }
        }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            if (state.loading) CircularProgressIndicator(Modifier.align(Alignment.Center))
            else if (state.info != null) PdfViewport(state, model) { toolbar = !toolbar; copied = false }
            else Column(Modifier.padding(24.dp)) { Text(state.error ?: "Could not open this PDF"); TextButton(onClick = back) { Text("Return to library") } }
            if (state.rendering) LinearProgressIndicator(Modifier.fillMaxWidth().align(Alignment.TopCenter).testTag("pdfRendering"))
        }
        if (toolbar && state.info != null) Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
            TextButton(onClick = { model.turn(-1) }, enabled = state.location.page > 1u) { Text("Previous") }
            TextButton(onClick = { jump = true }, modifier = Modifier.testTag("pageLabel")) { Text("Page ${state.location.page} of ${state.info!!.pages.size}") }
            TextButton(onClick = { model.turn(1) }, enabled = state.location.page < state.info!!.pages.size.toUInt()) { Text("Next") }
        }
    }
    if (jump) {
        var value by rememberSaveable { mutableStateOf("") }
        AlertDialog(onDismissRequest = { jump = false }, title = { Text("Go to page") },
            text = { OutlinedTextField(value, { value = it }, singleLine = true, label = { Text("Page number") }, modifier = Modifier.testTag("jumpPage")) },
            confirmButton = { TextButton(onClick = { if (model.jump(value)) jump = false }, enabled = value.isNotBlank()) { Text("Go") } },
            dismissButton = { TextButton(onClick = { jump = false }) { Text("Cancel") } })
    }
    if (state.error != null && state.info != null) AlertDialog(onDismissRequest = model::dismissError, title = { Text("PDF reader") },
        text = { Text(state.error!!) }, confirmButton = { TextButton(onClick = model::dismissError) { Text("OK") } })
}

@Composable
private fun PdfViewport(state: PdfState, model: PdfViewModel, toggle: () -> Unit) {
    val density = LocalDensity.current.density
    val vertical = rememberScrollState(); val horizontal = rememberScrollState()
    val location = state.location
    val pageSize = state.info!!.pages[location.page.toInt() - 1]
    val toggleNow by rememberUpdatedState(toggle)
    val locationNow by rememberUpdatedState(location)
    val selectionNow by rememberUpdatedState(state.selection)
    val geometry = remember(state.text) { state.text?.let(::PdfTextGeometry) }
    val geometryNow by rememberUpdatedState(geometry)
    val background = MaterialTheme.colorScheme.surfaceContainer
    BoxWithConstraints(Modifier.fillMaxSize().background(background).testTag("pdfViewport")
        .pointerInput(location.page, location.fitWidth, state.selection != null) {
            if (location.fitWidth && state.selection == null) {
                var distance = 0f
                detectHorizontalDragGestures(onDragStart = { distance = 0f }, onDragEnd = {
                    if (abs(distance) > 48 * density) model.turn(if (distance < 0) 1 else -1)
                }) { change, amount -> change.consume(); distance += amount }
            }
        }) {
        val fitScale = constraints.maxWidth / density / pageSize.width / (4f / 3f)
        val zoom = if (location.fitWidth) 1f else location.zoom / fitScale
        val width = constraints.maxWidth * zoom
        val height = width * pageSize.height / pageSize.width
        val zoomNow by rememberUpdatedState(zoom)
        LaunchedEffect(location.page, width) { model.render(width) }
        var ready by remember(state.revision) { mutableStateOf(false) }
        LaunchedEffect(state.revision, width, height) {
            ready = false
            withFrameNanos { }
            vertical.scrollTo((locationNow.within * height).roundToInt())
            horizontal.scrollTo((locationNow.horizontal * horizontal.maxValue).roundToInt())
            ready = true
        }
        LaunchedEffect(state.revision, width, height) {
            snapshotFlow { vertical.value to horizontal.value }.distinctUntilChanged().collect { (y, x) ->
                if (ready) model.record(locationNow.copy(within = (y / height).coerceIn(0f, 1f),
                    horizontal = if (horizontal.maxValue > 0) (x.toFloat() / horizontal.maxValue).coerceIn(0f, 1f) else 0f))
            }
        }
        Box(Modifier.fillMaxSize().semantics { stateDescription = "Zoom $zoom; page ${location.page}" }
            .pointerInput(fitScale) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    var scale = zoomNow
                    do {
                        val event = awaitPointerEvent()
                        if (event.changes.count { it.pressed } >= 2) {
                            scale = (scale * event.calculateZoom()).coerceIn(.5f, 4f)
                            model.zoom(scale * fitScale)
                            event.changes.forEach { it.consume() }
                        }
                    } while (event.changes.any { it.pressed })
                }
            }.verticalScroll(vertical).horizontalScroll(horizontal, enabled = !location.fitWidth)) {
            val image = remember(state.raster) { state.raster?.asImageBitmap() }
            Canvas(Modifier.requiredWidth((width / density).dp).requiredHeight((height / density).dp).background(Color.White).testTag("pdfPage")
                .semantics {
                    contentDescription = "PDF page ${location.page}"
                    stateDescription = if (image == null) "Loading page" else "Page ready"
                    if (state.text != null) text = androidx.compose.ui.text.AnnotatedString(state.text.text)
                    if (!state.text?.glyphs.isNullOrEmpty()) customActions = listOf(CustomAccessibilityAction("Select page text") { model.selectAll(); true })
                }
                .pointerInput(location.page) {
                    // One recognizer owns taps and selection, so lifting a long
                    // press cannot also toggle the controls or turn a page.
                    awaitEachGesture {
                        val down = awaitFirstDown(requireUnconsumed = false)
                        val held = awaitLongPressOrCancellation(down.id)
                        if (held != null) {
                            val geometry = geometryNow
                            val index = geometry?.closest(held.position, size.width.toFloat(), size.height.toFloat(), 24 * density)
                            if (index != null) {
                                val word = geometry.word(index); model.select(word)
                                held.consume()
                                drag(held.id) { change ->
                                    geometry.closest(change.position, size.width.toFloat(), size.height.toFloat())?.let { focus ->
                                        model.select(min(word.first, focus)..max(word.first, focus))
                                    }
                                    change.consume()
                                }
                                currentEvent.changes.forEach { it.consume() }
                            }
                        } else {
                            val up = currentEvent.changes.firstOrNull { it.id == down.id }
                            if (up != null && !up.pressed && !up.isConsumed && (up.position - down.position).getDistance() <= viewConfiguration.touchSlop &&
                                up.uptimeMillis - down.uptimeMillis < viewConfiguration.longPressTimeoutMillis) {
                                if (selectionNow != null) model.select(null)
                                else if (locationNow.fitWidth && up.position.x < size.width * .18f) model.turn(-1)
                                else if (locationNow.fitWidth && up.position.x > size.width * .82f) model.turn(1)
                                else toggleNow()
                                up.consume()
                            }
                        }
                    }
                }) {
                image?.let { drawImage(it, dstSize = IntSize(size.width.roundToInt(), size.height.roundToInt())) }
                state.selection?.let { range -> range.forEach { index -> state.text?.glyphs?.get(index)?.bounds?.let { r ->
                    drawRect(Color(0x6657a8ef), Offset(r.left * size.width, r.top * size.height),
                        Size((r.right - r.left) * size.width, (r.bottom - r.top) * size.height))
                } } }
            }
        }
        if (state.raster == null && state.error == null) CircularProgressIndicator(Modifier.align(Alignment.Center))
    }
}
