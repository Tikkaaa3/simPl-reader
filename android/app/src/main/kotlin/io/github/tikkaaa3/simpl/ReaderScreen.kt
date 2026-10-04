@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.tikkaaa3.simpl

import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.*
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.input.pointer.*
import androidx.compose.ui.platform.*
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.*
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlin.math.roundToInt

@Composable
internal fun ReaderScreen(book: LibraryBook, model: ReaderViewModel, back: () -> Unit, settings: () -> Unit, document: (() -> Unit)? = null) {
    val state by model.state.collectAsStateWithLifecycle()
    LaunchedEffect(book.path) { model.open(book) }
    val lifecycle = LocalLifecycleOwner.current
    val activity = requireNotNull(LocalActivity.current)
    var toolbar by rememberSaveable { mutableStateOf(true) }
    var zoom by rememberSaveable { mutableFloatStateOf(1f) }
    var editing by remember { mutableStateOf<AnnotationEntry?>(null) }
    LaunchedEffect(state.selection) { if (state.selection != null) toolbar = true }
    var panel by rememberSaveable { mutableStateOf<String?>(null) }
    var dictionary by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(state.selection) { if (state.selection == null) dictionary = false }
    val theme = state.themes.firstOrNull { it.id == state.theme }
    val appDark = MaterialTheme.colorScheme.surface.luminance() < 0.5f
    val dark = when (state.paperAppearance) { "light" -> false; "dark" -> true; else -> appDark }
    val palette = if (dark) theme?.dark else theme?.light
    val paper = palette?.let { rgbaColor(it.paper) } ?: MaterialTheme.colorScheme.surface
    val desk = palette?.let { rgbaColor(it.background) } ?: MaterialTheme.colorScheme.background
    val text = palette?.let { rgbaColor(it.text) } ?: MaterialTheme.colorScheme.onSurface
    val accent = palette?.let { rgbaColor(it.accent) } ?: MaterialTheme.colorScheme.primary

    DisposableEffect(lifecycle, model) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_STOP) model.stop() }
        lifecycle.lifecycle.addObserver(observer)
        onDispose { lifecycle.lifecycle.removeObserver(observer); model.stop() }
    }
    DisposableEffect(toolbar, dark) {
        val controller = WindowCompat.getInsetsController(activity.window, activity.window.decorView)
        controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        if (toolbar) controller.show(WindowInsetsCompat.Type.systemBars()) else controller.hide(WindowInsetsCompat.Type.systemBars())
        controller.isAppearanceLightStatusBars = !dark
        controller.isAppearanceLightNavigationBars = !dark
        onDispose { controller.show(WindowInsetsCompat.Type.systemBars()); controller.isAppearanceLightStatusBars = !appDark; controller.isAppearanceLightNavigationBars = !appDark }
    }
    BackHandler {
        when { state.selection != null -> model.select(null); state.note != null -> model.dismissNote(); panel != null -> panel = null;
            state.canReturn -> model.returnFromLink(); !toolbar -> toolbar = true; else -> { model.stop(); back() } }
    }
    Column(Modifier.fillMaxSize().background(desk).testTag("reader")) {
        if (toolbar) TopAppBar(title = { Text(book.title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
            navigationIcon = { IconButton(onClick = { model.stop(); back() }) { Icon(AppIcons.Back, "Back to library") } },
            actions = {
                if (document != null) TextButton(onClick = document) { Text("Document") }
                IconButton(onClick = model::bookmark, enabled = !state.loading) { Text(if (state.annotations.bookmarks.any { it.pageNumber == state.page }) "★" else "☆", Modifier.semantics { contentDescription = "Bookmark page" }) }
                IconButton(onClick = { panel = "annotations" }) { Text("☰", Modifier.semantics { contentDescription = "Annotations" }) }
                IconButton(onClick = settings) { Icon(AppIcons.Settings, "Settings") } },
            colors = TopAppBarDefaults.topAppBarColors(containerColor = paper, titleContentColor = text, navigationIconContentColor = text, actionIconContentColor = text))
        if (toolbar && !state.loading) FlowRow(Modifier.fillMaxWidth().background(paper).padding(horizontal = 8.dp)) {
            TextButton(onClick = { panel = "contents" }) { Text("Contents") }
            ReadAloudControls(book.fingerprint, model::readAloud)
            TextButton(onClick = { panel = "options" }) { Text("Reading options") }
            TextButton(onClick = { zoom = 1f }) { Text("Fit width") }
            if (state.canReturn) IconButton(onClick = model::returnFromLink) { Icon(AppIcons.Back, "Return from link", tint = accent) }
        }
        if (state.loading) Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) { Column(horizontalAlignment = Alignment.CenterHorizontally) {
            CircularProgressIndicator()
            if (document != null) {
                val progress = state.preparation
                Text(if ((progress?.total ?: 0u) > 0u) "Preparing Book: ${progress?.completed} of ${progress?.total} pages" else "Preparing Book…", Modifier.testTag("bookPreparation"))
                TextButton(onClick = { model.cancelOpening(); document() }) { Text("Cancel preparation") }
            }
        } }
        else Box(Modifier.weight(1f).fillMaxWidth()) {
            if (state.pages.isNotEmpty()) ReaderViewport(state, model, zoom, { zoom = it }, { toolbar = !toolbar }, paper, text, accent,
                dictionary = { model.selectWord(it) { dictionary = true } }) { id -> editing = state.annotations.highlights.firstOrNull { it.id == id } }
            state.selection?.let { selection -> Surface(Modifier.align(Alignment.BottomCenter), tonalElevation = 3.dp) {
                key(selection) { Column {
                    SelectionMenu("readerSelection", model::copy, model::highlight, read = { model.readSelection() }, dictionary = { dictionary = true }) { model.select(null) }
                    DictionarySelection(model::copy, selection, dictionary) { dictionary = false }
                } }
            } }
            if (state.adapting) LinearProgressIndicator(Modifier.fillMaxWidth().align(Alignment.TopCenter).testTag("layoutProgress"))
            if (state.pages.isEmpty() && state.error != null) Column(Modifier.padding(24.dp)) {
                Text(state.error!!, color = text); TextButton(onClick = back) { Text("Return to library") }
            }
        }
        if (toolbar && state.total > 0u && !state.loading) Row(Modifier.fillMaxWidth().background(paper).navigationBarsPadding().padding(horizontal = 8.dp),
            horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = { model.turn(-1) }, enabled = state.page > 1u && !state.adapting) { Text("Previous") }
            TextButton(onClick = { panel = "jump" }, modifier = Modifier.weight(1f).testTag("pageLabel")) {
                val label = state.pages.firstOrNull()?.layout?.label ?: state.page.toString()
                Text(if (label == state.page.toString()) "Page $label of ${state.total}" else "Page $label · ${state.page} of ${state.total}", color = text, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            TextButton(onClick = { model.turn(1) }, enabled = state.page < state.total && !state.adapting) { Text("Next") }
        }
    }
    if (panel == "annotations") AnnotationSheet(state.annotations, { panel = null }, model::annotation, model::edit, model::remove, read = { model.readPassage(it) }, book = book)
    editing?.let { entry -> NoteEditor(entry, { editing = null }) { color, note -> model.edit(entry.id, color, note); editing = null } }
    if (panel == "contents") ModalBottomSheet(onDismissRequest = { panel = null }) {
        Text("Contents", Modifier.padding(20.dp), style = MaterialTheme.typography.headlineSmall)
        LazyColumn(Modifier.fillMaxWidth().heightIn(max = 480.dp)) {
            if (state.contents.isEmpty()) item { Text("No contents entries in this book", Modifier.padding(20.dp)) }
            items(state.contents) { entry ->
                TextButton(onClick = { model.go(entry.location); panel = null }, modifier = Modifier.fillMaxWidth().padding(start = (entry.depth.toInt().coerceAtMost(8) * 12).dp)) {
                    Text(entry.label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurface)
                    Text(entry.location.page.toString())
                }
            }
        }
    }
    if (panel == "options") ModalBottomSheet(onDismissRequest = { panel = null }) { ReadingOptions(state, model) }
    if (panel == "jump") JumpDialog(state, model) { panel = null }
    state.note?.let { note ->
        val measurer = rememberTextMeasurer()
        val rows = remember(note, state.options, text, accent) { measureRows(note.section, note.noteRows, state.options, measurer, text, accent) }
        ModalBottomSheet(onDismissRequest = model::dismissNote, containerColor = paper) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                Text("Note", color = text, style = MaterialTheme.typography.titleLarge)
                TextButton(onClick = model::dismissNote) { Text("Return to reading") }
            }
            BoxWithConstraints(Modifier.fillMaxWidth().heightIn(max = 480.dp).verticalScroll(rememberScrollState()).padding(20.dp)) {
                val scale = constraints.maxWidth / (720f - state.options.margin.toInt() * 2)
                Column { rows.forEach { row -> PaperRow(row, scale, text, accent, model::image, model::follow, {}) } }
            }
        }
    }
    if (state.error != null && state.pages.isNotEmpty()) AlertDialog(onDismissRequest = model::dismissError,
        title = { Text("Reader") }, text = { Text(state.error!!) }, confirmButton = { TextButton(onClick = model::dismissError) { Text("OK") } })
}

@Composable
private fun ReaderViewport(state: ReaderState, model: ReaderViewModel, zoom: Float, zoomTo: (Float) -> Unit, toggle: () -> Unit,
    paper: Color, text: Color, accent: Color, dictionary: (SourcePoint) -> Unit, editMark: (ULong) -> Unit) {
    val measurer = rememberTextMeasurer(cacheSize = 64)
    val measured = remember(state.pages, state.options, text, accent) { measurePage(state.pages, state.options, measurer, text, accent) }
    val spoken = state.spoken
    val spokenRow = state.speechRow
    val spokenLayout = remember(spokenRow, state.options, text, accent) {
        spokenRow?.let { measureRows(0u, listOf(it), state.options, measurer, text, accent).firstOrNull()?.text }
    }
    LaunchedEffect(spoken, spokenLayout, state.revision, state.adapting) {
        if (spoken != null && spokenRow?.index == spoken.from.row && spokenLayout != null) {
            val line = spokenLayout.getLineForOffset(byteIndex(spokenRow.text.orEmpty(), spoken.from.byte))
            model.speechFollow(spoken.from, line.toUInt(), spokenLayout.lineCount.toUInt())
        }
    }
    val density = LocalDensity.current.density
    val vertical = rememberScrollState()
    val horizontal = rememberScrollState()
    val currentToggle by rememberUpdatedState(toggle)
    val currentZoom by rememberUpdatedState(zoom)
    val currentZoomTo by rememberUpdatedState(zoomTo)
    BoxWithConstraints(Modifier.fillMaxSize().testTag("paperViewport").semantics { stateDescription = "Zoom $zoom; page ${state.page}" }
        .pointerInput(state.page, zoom <= 1.01f, state.selection != null) {
            if (zoom <= 1.01f && state.selection == null) {
                var distance = 0f
                detectHorizontalDragGestures(onDragStart = { distance = 0f }, onDragEnd = {
                    if (kotlin.math.abs(distance) > 48 * density) model.turn(if (distance < 0) 1 else -1)
                }) { change, amount -> change.consume(); distance += amount }
            }
        }
        .pointerInput(Unit) {
            awaitEachGesture {
                awaitFirstDown(requireUnconsumed = false)
                var gestureZoom = currentZoom
                do {
                    val event = awaitPointerEvent()
                    if (event.changes.count { it.pressed } >= 2) {
                        gestureZoom = (gestureZoom * event.calculateZoom()).coerceIn(1f, 3f)
                        currentZoomTo(gestureZoom)
                        event.changes.forEach { it.consume() }
                    }
                } while (event.changes.any { it.pressed })
            }
        }) {
        val scale = constraints.maxWidth / 720f * zoom
        val viewportWidth = constraints.maxWidth
        val paperWidth = (720 * scale / density).dp
        val heights = remember(measured) {
            val starts = mutableListOf(42f)
            measured.forEach { starts += starts.last() + it.height }
            starts
        }
        fun hit(point: Offset): SourcePoint? {
            val y = (point.y + vertical.value) / scale
            val index = (heights.indexOfLast { it <= y }).coerceIn(0, measured.lastIndex.coerceAtLeast(0))
            val row = measured.getOrNull(index) ?: return null
            val layout = row.text ?: return null
            val source = Offset((point.x + horizontal.value) / scale - state.options.margin.toInt() - row.row.presentation.left,
                (y - heights[index] - row.top + row.textTop).coerceIn(row.textTop, row.textBottom - .01f))
            val offset = layout.getOffsetForPosition(source).coerceIn(layout.getLineStart(row.firstLine), layout.getLineEnd(row.lastLine - 1))
            return SourcePoint(row.section, row.row.index, sourceByte(row.row.text.orEmpty(), offset))
        }
        fun handle(point: SourcePoint?): Offset? {
            val index = measured.indexOfFirst { it.section == point?.section && it.row.index == point.row }
            if (index < 0) return null
            val row = measured[index]; val layout = row.text ?: return null
            val offset = byteIndex(row.row.text.orEmpty(), point!!.byte)
            val line = layout.getLineForOffset(offset)
            if (line !in row.firstLine until row.lastLine) return null
            val rect = layout.getCursorRect(offset)
            val result = Offset((state.options.margin.toInt() + row.row.presentation.left + rect.left) * scale - horizontal.value,
                (heights[index] + row.top - row.textTop + rect.bottom) * scale - vertical.value)
            return result.takeIf { it.y in 0f..constraints.maxHeight.toFloat() }
        }
        val handles = handle(state.selection?.from) to handle(state.selection?.to)
        fun lift(point: SourcePoint?): Float {
            val row = measured.firstOrNull { it.section == point?.section && it.row.index == point.row } ?: return 0f
            val layout = row.text ?: return 0f
            val line = layout.getLineForOffset(byteIndex(row.row.text.orEmpty(), point!!.byte))
            return (layout.getLineBottom(line) - layout.getLineTop(line)) * scale / 2
        }
        var dragging by remember { mutableStateOf<Offset?>(null) }
        var draggingStart by remember { mutableStateOf(false) }
        val gestures = selectionGesture(handles, { point ->
            hit(point)?.let { source -> model.selectWord(source); true } ?: false
        }, { point, start -> draggingStart = start; hit(point)?.let { model.extend(it, start) } }, { dragging = it }, lift(state.selection?.from) to lift(state.selection?.to))
        SelectionEdgeDrag(dragging, constraints.maxHeight.toFloat(), { vertical.scrollBy(it) }, { model.selectionTurn(it, draggingStart) }) { point -> hit(point)?.let { model.extend(it, draggingStart) } }
        var positionReady by remember(state.revision) { mutableStateOf(false) }
        var previousScale by remember { mutableFloatStateOf(scale) }
        LaunchedEffect(state.revision) {
            withFrameNanos { }
            horizontal.scrollTo(0)
            val location = state.location
            val index = measured.indexOfFirst { it.section == location?.section && it.row.index == location.row }
            val target = if (index < 0) 0f else {
                val row = measured[index]
                val part = ((location!!.within - row.startFraction) / (row.endFraction - row.startFraction).coerceAtLeast(0.001f)).coerceIn(0f, 1f)
                (heights[index] + row.height * part) * scale
            }
            vertical.scrollTo(target.roundToInt())
            state.selectionEdge?.let { start ->
                hit(Offset(constraints.maxWidth * .5f, if (start) 0f else constraints.maxHeight.toFloat()))?.let(model::completeSelectionEdge)
            }
            previousScale = scale
            positionReady = true
        }
        LaunchedEffect(scale) {
            if (positionReady) {
                val target = vertical.value * scale / previousScale
                withFrameNanos { }
                vertical.scrollTo(target.roundToInt())
                previousScale = scale
            }
        }
        LaunchedEffect(measured, state.revision, scale) {
            snapshotFlow { vertical.value }.distinctUntilChanged().collect { offset ->
                if (positionReady && measured.isNotEmpty()) {
                    val y = offset / scale
                    val index = (heights.indexOfLast { it <= y }).coerceIn(0, measured.lastIndex)
                    val row = measured[index]
                    val part = ((y - heights[index]) / row.height.coerceAtLeast(1f)).coerceIn(0f, 1f)
                    model.record(ReaderLocation(state.page, row.section, row.row.index, row.startFraction + part * (row.endFraction - row.startFraction)))
                }
            }
        }
        LaunchedEffect(state.spoken, measured, scale) {
            val word = state.spoken ?: return@LaunchedEffect
            val index = measured.indexOfFirst { it.section == word.from.section && it.row.index == word.from.row }
            val row = measured.getOrNull(index) ?: return@LaunchedEffect
            val layout = row.text ?: return@LaunchedEffect
            val line = layout.getLineForOffset(byteIndex(row.row.text.orEmpty(), word.from.byte))
            if (line !in row.firstLine until row.lastLine) return@LaunchedEffect
            val top = (heights[index] + row.top - row.textTop + layout.getLineTop(line)) * scale
            val bottom = top + row.row.presentation.lineHeight * scale
            if (top < vertical.value || bottom > vertical.value + constraints.maxHeight) {
                vertical.scrollTo((top - constraints.maxHeight * .25f).roundToInt().coerceAtLeast(0))
            }
        }
        val tap: (Offset) -> Unit = { point ->
            // Row offsets are within the text column; margins count as edge zones too.
            val x = point.x + state.options.margin.toInt() * scale - horizontal.value
            when { state.selection != null -> model.select(null); x < constraints.maxWidth * 0.18f -> model.turn(-1);
                x > constraints.maxWidth * 0.82f -> model.turn(1); else -> currentToggle() }
        }
        Box(Modifier.fillMaxSize().then(gestures)) {
          Box(Modifier.fillMaxSize().verticalScroll(vertical, enabled = dragging == null).horizontalScroll(horizontal, enabled = zoom > 1.01f && dragging == null)) {
            Column(Modifier.requiredWidth(paperWidth).heightIn(min = (720f * 1.414f * scale / density).dp)
                .background(paper).pointerInput(scale) { detectTapGestures { point ->
                    when { state.selection != null -> model.select(null); point.x - horizontal.value < viewportWidth * 0.18f -> model.turn(-1);
                        point.x - horizontal.value > viewportWidth * 0.82f -> model.turn(1); else -> currentToggle() }
                } }
                .padding(horizontal = (state.options.margin.toInt() * scale / density).dp, vertical = (42f * scale / density).dp)) {
                measured.forEach { row -> key(row.section, row.row.index) { PaperRow(row, scale, text, accent, model::image, model::follow, tap,
                    selection = state.selection, spoken = state.spoken, marks = state.marks.filter { it.section == row.section && it.row == row.row.index }, editMark = editMark,
                    extend = { model.extend(it, false) }, dictionary = dictionary) } }
            }
          }
          SelectionHandle(handles.first, true)
          SelectionHandle(handles.second, false)
        }
    }
}

@Composable
private fun ReadingOptions(state: ReaderState, model: ReaderViewModel) {
    val options = state.options
    Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Reading options", style = MaterialTheme.typography.headlineSmall)
        LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            items(state.themes) { theme -> FilterChip(state.theme == theme.id, onClick = { model.change(theme = theme.id) }, label = { Text(theme.name) }) }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("auto" to "Device", "light" to "Light paper", "dark" to "Dark paper").forEach { (value, label) ->
                FilterChip(state.paperAppearance == value, onClick = { model.paperAppearance(value) }, label = { Text(label) })
            }
        }
        Text("Font", style = MaterialTheme.typography.titleSmall)
        LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            items(ReadingFont.entries) { font -> FilterChip(options.font == font, onClick = { model.change(options.copy(font = font)) },
                label = { Text(when (font) { ReadingFont.THEME -> "Theme font"; ReadingFont.LITERATA -> "Literata"; ReadingFont.SPECTRAL -> "Spectral"; ReadingFont.FIRA_SANS -> "Fira Sans" }) }) }
        }
        OptionStepper("Font size", options.size.toInt().toString(), "font size", options.size.toInt() > 12, options.size.toInt() < 36,
            { model.change(options.copy(size = (options.size.toInt() - 2).coerceAtLeast(12).toUShort())) },
            { model.change(options.copy(size = (options.size.toInt() + 2).coerceAtMost(36).toUShort())) })
        OptionStepper("Line spacing", if (options.spacing == 0.toUShort()) "Theme" else "${options.spacing}%", "line spacing", options.spacing.toInt() != 110, options.spacing.toInt() < 220,
            { model.change(options.copy(spacing = ((if (options.spacing == 0.toUShort()) 160 else options.spacing.toInt()) - 10).coerceAtLeast(110).toUShort())) },
            { model.change(options.copy(spacing = ((if (options.spacing == 0.toUShort()) 160 else options.spacing.toInt()) + 10).coerceAtMost(220).toUShort())) })
        OptionStepper("Margins", options.margin.toInt().toString(), "margins", options.margin.toInt() > 16, options.margin.toInt() < 96,
            { model.change(options.copy(margin = (options.margin.toInt() - 8).coerceAtLeast(16).toUShort())) },
            { model.change(options.copy(margin = (options.margin.toInt() + 8).coerceAtMost(96).toUShort())) })
        TextButton(onClick = { model.change(LayoutOptions(ReadingFont.THEME, 20u, 48u, 0u)) }) { Text("Reset typography") }
        Text("Pinch to zoom. Swipe or tap a page edge to turn. Tap the center to hide controls.", style = MaterialTheme.typography.bodySmall)
        if (state.warnings.isNotEmpty()) Text(state.warnings.joinToString("\n"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.height(16.dp))
    }
}

@Composable
private fun OptionStepper(title: String, value: String, label: String, decrease: Boolean, increase: Boolean, less: () -> Unit, more: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(title, Modifier.weight(1f)); IconButton(onClick = less, enabled = decrease, modifier = Modifier.semantics { contentDescription = "Decrease $label" }) { Text("−") }
        Text(value, Modifier.widthIn(min = 48.dp)); IconButton(onClick = more, enabled = increase, modifier = Modifier.semantics { contentDescription = "Increase $label" }) { Text("+") }
    }
}

@Composable
private fun JumpDialog(state: ReaderState, model: ReaderViewModel, dismiss: () -> Unit) {
    var value by rememberSaveable { mutableStateOf("") }
    AlertDialog(onDismissRequest = dismiss, title = { Text("Go to page") }, text = {
        OutlinedTextField(value, { value = it }, label = { Text("Page number or label") }, singleLine = true, modifier = Modifier.testTag("jumpPage"),
            supportingText = { Text("${state.total} pages") })
    }, confirmButton = { TextButton(onClick = { model.jump(value, dismiss) }, enabled = value.isNotBlank()) { Text("Go") } }, dismissButton = { TextButton(onClick = dismiss) { Text("Cancel") } })
}
