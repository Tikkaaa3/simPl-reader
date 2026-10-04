@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.tikkaaa3.simpl

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.gestures.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.*
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.*
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.delay
import kotlin.math.roundToInt

internal val emptyAnnotations get() = AnnotationCollection(emptyList(), emptyList())
internal fun AnnotationColor.tint(): Color = when (this) {
    AnnotationColor.YELLOW -> Color(0xfff5c40f).copy(alpha = .42f)
    AnnotationColor.GREEN -> Color(0xff3fc06b).copy(alpha = .38f)
    AnnotationColor.BLUE -> Color(0xff4a9ef5).copy(alpha = .38f)
    AnnotationColor.PINK -> Color(0xfff05ca8).copy(alpha = .36f)
}
internal fun AnnotationColor.label() = name.lowercase().replaceFirstChar { it.uppercase() }

/** Mirror annotation_logic::coalesce_colored_ranges for platform text geometry.
 * Each merged component keeps its oldest paint order, including disjoint spans
 * of a color on either side of another color's layer. */
internal fun coloredRanges(ranges: List<Pair<IntRange, AnnotationColor>>): List<Pair<IntRange, AnnotationColor>> {
    data class Painted(val range: IntRange, val color: AnnotationColor, val order: Int)
    val merged = ranges.map { it.second }.distinct().flatMap { color ->
        val result = mutableListOf<Painted>()
        ranges.withIndex().filter { it.value.second == color && !it.value.first.isEmpty() }.sortedBy { it.value.first.first }.forEach { (order, value) ->
            val range = value.first; val last = result.lastOrNull()
            if (last != null && range.first.toLong() <= last.range.last.toLong() + 1)
                result[result.lastIndex] = Painted(last.range.first..maxOf(last.range.last, range.last), color, minOf(last.order, order))
            else result += Painted(range, color, order)
        }
        result
    }
    return merged.sortedBy { it.order }.map { it.range to it.color }
}

@Composable
internal fun SelectionMenu(tag: String, copy: ((String) -> Unit) -> Unit, highlight: (AnnotationColor, String?) -> Unit, read: (() -> Unit)? = null, dictionary: (() -> Unit)? = null, clear: () -> Unit) {
    val context = LocalContext.current
    var copied by remember(tag) { mutableStateOf(false) }
    var note by rememberSaveable { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().testTag(tag)) {
        FlowRow(Modifier.fillMaxWidth()) {
            Text("Text selected", Modifier.padding(start = 12.dp, top = 14.dp), style = MaterialTheme.typography.labelLarge)
            TextButton(onClick = { copy { text ->
                (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Book text", text)); copied = true
            } }) { Text(if (copied) "Copied" else "Copy") }
            TextButton(onClick = { copy { text -> context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply {
                type = "text/plain"; putExtra(Intent.EXTRA_TEXT, text)
            }, "Share text")) } }) { Text("Share") }
            if (read != null) TextButton(onClick = read) { Text("Read selection") }
            if (dictionary != null) TextButton(onClick = dictionary) { Text("Dictionary") }
            TextButton(onClick = clear) { Text("Clear") }
        }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            AnnotationColor.entries.forEach { color -> ColorButton(color, false) { highlight(color, null) } }
            TextButton(onClick = { note = true }) { Text("Add note") }
        }
    }
    if (note) NoteEditor(null, { note = false }, { color, text -> highlight(color, text); note = false })
}

@Composable
private fun ColorButton(color: AnnotationColor, selected: Boolean, click: () -> Unit) {
    IconButton(onClick = click, modifier = Modifier.semantics { contentDescription = "Highlight ${color.label().lowercase()}"; this.selected = selected }) {
        Box(Modifier.size(if (selected) 30.dp else 24.dp).background(color.tint().copy(alpha = 1f), CircleShape))
    }
}

@Composable
internal fun NoteEditor(entry: AnnotationEntry?, dismiss: () -> Unit, save: (AnnotationColor, String) -> Unit) {
    var color by remember(entry?.id) { mutableStateOf(entry?.color ?: AnnotationColor.YELLOW) }
    var note by remember(entry?.id) { mutableStateOf(entry?.note.orEmpty()) }
    AlertDialog(onDismissRequest = dismiss, title = { Text(if (entry == null) "Add note" else "Edit highlight") }, text = {
        Column {
            if (entry != null) Text(entry.quote.take(240), Modifier.padding(bottom = 12.dp), style = MaterialTheme.typography.bodySmall)
            Row { AnnotationColor.entries.forEach { option -> ColorButton(option, color == option) { color = option } } }
            OutlinedTextField(note, { note = it }, label = { Text("Note") }, modifier = Modifier.testTag("annotationNote"), maxLines = 6,
                supportingText = { Text("Leave blank to remove the note") })
        }
    }, confirmButton = { TextButton(onClick = { save(color, note) }) { Text("Save") } }, dismissButton = { TextButton(onClick = dismiss) { Text("Cancel") } })
}

@Composable
internal fun AnnotationSheet(data: AnnotationCollection, dismiss: () -> Unit, go: (ULong, Boolean) -> Unit,
    edit: (ULong, AnnotationColor, String) -> Unit, remove: (ULong, Boolean) -> Unit, read: ((String) -> Unit)? = null, book: LibraryBook? = null) {
    ModalBottomSheet(onDismissRequest = dismiss) { AnnotationPanel(data, dismiss, go, edit, remove, read, book) }
}

@Composable
internal fun AnnotationPanel(data: AnnotationCollection, dismiss: () -> Unit, go: (ULong, Boolean) -> Unit,
    edit: (ULong, AnnotationColor, String) -> Unit, remove: (ULong, Boolean) -> Unit, read: ((String) -> Unit)? = null, book: LibraryBook? = null) {
    var tab by rememberSaveable { mutableIntStateOf(0) }
    var editing by remember { mutableStateOf<AnnotationEntry?>(null) }
    var deleting by remember { mutableStateOf<Pair<AnnotationEntry, Boolean>?>(null) }
    Column(Modifier.fillMaxWidth().testTag("annotationsPanel")) {
        Text("Annotations", Modifier.padding(horizontal = 20.dp, vertical = 12.dp), style = MaterialTheme.typography.headlineSmall)
        if (book != null) NotesExport(book)
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            listOf("Bookmarks", "Highlights", "Notes").forEachIndexed { index, title -> FilterChip(tab == index, onClick = { tab = index }, label = { Text(title) }) }
        }
        val entries = if (tab == 0) data.bookmarks else data.highlights.filter { tab == 1 || !it.note.isNullOrBlank() }
        LazyColumn(Modifier.fillMaxWidth().heightIn(max = 440.dp).testTag("annotationList")) {
            if (entries.isEmpty()) item { Text("No ${listOf("bookmarks", "highlights", "notes")[tab]} yet", Modifier.padding(20.dp)) }
            items(entries, key = { it.id.toString() }) { entry ->
                Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp).testTag("annotation:${entry.id}")) {
                    TextButton(onClick = { go(entry.id, tab == 0); dismiss() }, modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.weight(1f), horizontalAlignment = Alignment.Start) {
                            Text("Page ${entry.page}", style = MaterialTheme.typography.labelLarge)
                            Text(entry.quote.ifBlank { "Bookmark" }, maxLines = 3, color = MaterialTheme.colorScheme.onSurface)
                            entry.note?.let { Text(it, maxLines = 3, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                        }
                        entry.color?.let { Box(Modifier.padding(start = 8.dp).size(16.dp).background(it.tint().copy(alpha = 1f), CircleShape)) }
                    }
                    Row(Modifier.align(Alignment.End)) {
                        if (tab != 0 && read != null) TextButton(onClick = { read(entry.quote); dismiss() }) { Text("Read aloud") }
                        if (tab != 0) TextButton(onClick = { editing = entry }) { Text("Edit") }
                        TextButton(onClick = { deleting = entry to (tab == 0) }) { Text("Delete") }
                    }
                    HorizontalDivider()
                }
            }
        }
        Spacer(Modifier.height(20.dp))
    }
    editing?.let { entry -> NoteEditor(entry, { editing = null }) { color, note -> edit(entry.id, color, note); editing = null } }
    deleting?.let { (entry, bookmark) -> AlertDialog(onDismissRequest = { deleting = null }, title = { Text("Delete ${if (bookmark) "bookmark" else "highlight"}?") },
        text = { Text(entry.note ?: entry.quote.take(200)) }, confirmButton = { TextButton(onClick = { remove(entry.id, bookmark); deleting = null }, modifier = Modifier.testTag("annotationDeleteConfirm")) { Text("Delete") } },
        dismissButton = { TextButton(onClick = { deleting = null }) { Text("Cancel") } }) }
}

/** The viewport owns the gesture, so selection survives replacement of a page. */
@Composable
internal fun selectionGesture(handles: Pair<Offset?, Offset?>, begin: (Offset) -> Boolean, move: (Offset, Boolean) -> Unit,
    dragging: (Offset?) -> Unit, handleLift: Pair<Float, Float> = 0f to 0f): Modifier {
    val handlesNow by rememberUpdatedState(handles)
    val beginNow by rememberUpdatedState(begin)
    val moveNow by rememberUpdatedState(move)
    val draggingNow by rememberUpdatedState(dragging)
    val liftNow by rememberUpdatedState(handleLift)
    val density = LocalDensity.current.density
    return Modifier.pointerInput(Unit) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            val startDistance = handlesNow.first?.let { (it - down.position).getDistance() } ?: Float.POSITIVE_INFINITY
            val endDistance = handlesNow.second?.let { (it - down.position).getDistance() } ?: Float.POSITIVE_INFINITY
            val start = startDistance <= 28 * density && startDistance < endDistance
            val end = endDistance <= 28 * density && !start
            val lift = if (start) liftNow.first else if (end) liftNow.second else 0f
            val held = if (start || end) down else {
                // Let the child's tap recognizer receive this down first.
                awaitPointerEvent(PointerEventPass.Main)
                awaitLongPressOrCancellation(down.id)
            }
            if (held != null && (start || end || beginNow(held.position))) {
                held.consume()
                try {
                    // Claim motion before the nested page scrollers. A handle
                    // drag can start immediately and must never become a pan.
                    while (true) {
                        val event = awaitPointerEvent(PointerEventPass.Initial)
                        if (event.changes.count { it.pressed } > 1) break
                        val change = event.changes.firstOrNull { it.id == held.id } ?: break
                        if (!change.pressed) { change.consume(); break }
                        if (change.position != change.previousPosition) {
                            moveNow(change.position - Offset(0f, lift), start); draggingNow(change.position)
                        }
                        change.consume()
                    }
                } finally { draggingNow(null) }
            }
        }
    }
}

@Composable
internal fun BoxScope.SelectionHandle(point: Offset?, start: Boolean) {
    if (point == null) return
    val density = LocalDensity.current.density
    val color = MaterialTheme.colorScheme.primary
    Canvas(Modifier.offset { IntOffset((point.x - 24 * density).roundToInt(), (point.y - 24 * density).roundToInt()) }
        .size(48.dp).testTag(if (start) "selectionStart" else "selectionEnd").semantics { contentDescription = if (start) "Selection start handle" else "Selection end handle" }) {
        drawCircle(color, 8 * density)
    }
}

/** Scroll while held at the top/bottom, then continue onto the adjacent page. */
@Composable
internal fun SelectionEdgeDrag(point: Offset?, height: Float, scroll: suspend (Float) -> Float, turn: (Int) -> Unit, move: (Offset) -> Unit) {
    val pointNow by rememberUpdatedState(point)
    val turnNow by rememberUpdatedState(turn)
    val moveNow by rememberUpdatedState(move)
    val density = LocalDensity.current.density
    LaunchedEffect(point != null, height) {
        var ticks = 0
        while (pointNow != null) {
            delay(100)
            val p = pointNow ?: break
            val direction = when { p.y < 36 * density -> -1; p.y > height - 36 * density -> 1; else -> 0 }
            if (direction == 0) { ticks = 0; continue }
            val amount = direction * 28 * density
            if (kotlin.math.abs(scroll(amount)) < 1f && ++ticks >= 6) { turnNow(direction); ticks = 0; delay(180) }
            moveNow(p)
        }
    }
}
