@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.tikkaaa3.simpl

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.*
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.*
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle

/** One stable row; secondary actions open only when requested. */
@Composable
internal fun ReaderBottomBar(page: UInt, total: UInt, label: String = page.toString(),
    enabled: Boolean, color: Color, text: Color, loading: Boolean = false, numeric: Boolean = true, requestPageFocus: Boolean = false,
    caption: String? = null, progress: Float? = null,
    pageFocusHandled: () -> Unit = {},
    editingChanged: (Boolean) -> Unit = {},
    back: () -> Unit, previous: () -> Unit, next: () -> Unit, jump: (String, () -> Unit) -> Unit,
    contents: (() -> Unit)? = null, tools: () -> Unit) {
    val manager = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    val focus = remember { FocusRequester() }
    var editing by remember { mutableStateOf(false) }
    var value by rememberSaveable(stateSaver = TextFieldValue.Saver) { mutableStateOf(TextFieldValue(page.toString())) }
    fun finish() { manager.clearFocus(); keyboard?.hide() }
    fun submit() { if (value.text.isNotBlank()) jump(value.text.trim(), ::finish) }
    LaunchedEffect(page, editing) { if (!editing) value = TextFieldValue(page.toString()) }
    LaunchedEffect(requestPageFocus, enabled) { if (requestPageFocus && enabled) { focus.requestFocus(); keyboard?.show(); pageFocusHandled() } }
    BackHandler(editing) { finish() }
    Surface(color = color, contentColor = text) {
      Column {
        // Desktop's slim progress track and "Book · Chapter" title, so position is visible at a glance.
        if (progress != null) ThinProgress(progress, Modifier.fillMaxWidth(), height = 2.dp)
        if (!caption.isNullOrBlank() && !loading) Text(caption, Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 8.dp).testTag("readerCaption"),
            style = MaterialTheme.typography.labelMedium, color = text.copy(alpha = .62f), maxLines = 1, overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center)
        BoxWithConstraints(Modifier.fillMaxWidth()) {
            val roomForContents = maxWidth >= 360.dp
            Row(Modifier.fillMaxWidth().navigationBarsPadding().imePadding().height(56.dp)
                .padding(horizontal = 4.dp).testTag("readerToolbar"), verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = { finish(); back() }, modifier = Modifier.size(48.dp)) { Icon(AppIcons.Back, "Back to library") }
                if (loading || total == 0u) {
                    Text(if (loading) "Opening book…" else "Reader", Modifier.weight(1f), style = MaterialTheme.typography.labelMedium)
                } else {
                    // Arrows stay beside the page field, as on desktop, however wide the bar is.
                    Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
                    IconButton(onClick = { finish(); previous() }, modifier = Modifier.size(48.dp), enabled = enabled && page > 1u) { Icon(AppIcons.Previous, "Previous") }
                    Row(Modifier.weight(1f, fill = false).testTag("pageLabel").semantics {
                        stateDescription = if (label == page.toString()) "Page $label of $total" else "Page $label · $page of $total"
                    }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
                        BasicTextField(value, { value = it.copy(text = it.text.take(32)) }, enabled = enabled, singleLine = true,
                            modifier = Modifier.weight(1f, fill = false).widthIn(min = 64.dp, max = 160.dp).heightIn(min = 48.dp).focusRequester(focus)
                                .onFocusChanged { state ->
                                    if (state.isFocused && !editing) {
                                        val current = page.toString()
                                        value = TextFieldValue(current, TextRange(0, current.length))
                                    }
                                    editing = state.isFocused
                                    editingChanged(state.isFocused)
                                }.testTag("jumpPage").semantics { contentDescription = if (numeric) "Page number" else "Page number or label" },
                            textStyle = MaterialTheme.typography.bodyMedium.copy(color = text, textAlign = TextAlign.Center),
                            cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                            keyboardOptions = KeyboardOptions(keyboardType = if (numeric) KeyboardType.Number else KeyboardType.Text, imeAction = ImeAction.Go),
                            keyboardActions = KeyboardActions(onGo = { submit() }),
                            decorationBox = { field -> Box(Modifier.fillMaxSize().padding(vertical = 5.dp)
                                .background(text.copy(alpha = .06f), RoundedCornerShape(4.dp)).padding(horizontal = 4.dp),
                                contentAlignment = Alignment.Center) { field() } })
                        Text("/ $total", Modifier.padding(start = 4.dp), style = MaterialTheme.typography.labelMedium, maxLines = 1)
                    }
                    IconButton(onClick = { finish(); next() }, modifier = Modifier.size(48.dp), enabled = enabled && page < total) { Icon(AppIcons.Next, "Next") }
                    }
                    if (editing && roomForContents) IconButton(onClick = ::submit, modifier = Modifier.size(48.dp), enabled = value.text.isNotBlank()) { Icon(AppIcons.Check, "Go") }
                    else if (!editing && contents != null && roomForContents) IconButton(onClick = contents, modifier = Modifier.size(48.dp), enabled = enabled) { Icon(AppIcons.Contents, "Contents") }
                    IconButton(onClick = { finish(); tools() }, modifier = Modifier.size(48.dp), enabled = enabled) { Icon(AppIcons.More, "Reader tools") }
                }
            }
        }
      }
    }
}

internal data class ReaderTool(val label: String, val icon: ImageVector, val action: () -> Unit,
    val enabled: Boolean = true, val selected: Boolean = false)

@Composable
internal fun ReaderToolsSheet(title: String, fingerprint: String, zoom: Float, tools: List<ReaderTool>,
    dismiss: () -> Unit, zoomOut: () -> Unit, zoomIn: () -> Unit, fitPage: (() -> Unit)? = null,
    fitWidth: () -> Unit, read: () -> Unit, canRead: Boolean = true) {
    ModalBottomSheet(onDismissRequest = dismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp)
            .testTag("readerTools")) {
            Row(Modifier.fillMaxWidth().padding(bottom = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(title, Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis,
                    style = MaterialTheme.typography.titleMedium)
                IconButton(onClick = dismiss, modifier = Modifier.size(48.dp)) { Icon(AppIcons.Close, "Close reader tools") }
            }
            // Desktop's zoom group: − 100% + with fit choices, as quiet bordered controls.
            Row(Modifier.fillMaxWidth().padding(bottom = 12.dp), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                val outline = androidx.compose.foundation.BorderStroke(1.dp, MaterialTheme.colorScheme.outline)
                OutlinedIconButton(onClick = zoomOut, shape = ControlShape, border = outline, modifier = Modifier.semantics { contentDescription = "Zoom out" }) {
                    Text("−", style = MaterialTheme.typography.titleLarge)
                }
                Text("${(zoom * 100).toInt()}%", Modifier.widthIn(min = 48.dp), style = MaterialTheme.typography.labelLarge, textAlign = TextAlign.Center)
                OutlinedIconButton(onClick = zoomIn, shape = ControlShape, border = outline, modifier = Modifier.semantics { contentDescription = "Zoom in" }) {
                    Icon(AppIcons.Plus, null)
                }
                Spacer(Modifier.weight(1f))
                if (fitPage != null) OutlinedButton(onClick = { fitPage(); dismiss() }, shape = ControlShape, border = outline, colors = quietButtonColors(),
                    contentPadding = PaddingValues(horizontal = 12.dp)) { Text("Fit page", maxLines = 1) }
                OutlinedButton(onClick = { fitWidth(); dismiss() }, shape = ControlShape, border = outline, colors = quietButtonColors(),
                    contentPadding = PaddingValues(horizontal = 12.dp)) { Text("Fit width", maxLines = 1) }
            }
            HorizontalDivider()
            tools.chunked(2).forEach { pair ->
                Row(Modifier.fillMaxWidth()) {
                    pair.forEach { tool -> TextButton(onClick = { dismiss(); tool.action() }, enabled = tool.enabled, shape = ControlShape, colors = quietButtonColors(),
                        modifier = Modifier.weight(1f).heightIn(min = 52.dp).semantics { if (tool.selected) selected = true }) {
                        Icon(tool.icon, null, Modifier.size(20.dp), tint = if (tool.selected) MaterialTheme.colorScheme.primary
                            else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = if (tool.enabled) 1f else .38f))
                        Spacer(Modifier.width(12.dp))
                        Text(tool.label, Modifier.weight(1f), maxLines = 2, style = MaterialTheme.typography.bodyMedium)
                    } }
                    if (pair.size == 1) Spacer(Modifier.weight(1f))
                }
            }
            HorizontalDivider()
            FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) {
                ReadAloudControls(fingerprint, { read(); dismiss() }, enabled = canRead)
            }
            Spacer(Modifier.height(16.dp))
        }
    }
}

@Composable
internal fun ReaderSpeechBar(fingerprint: String) {
    val state by ReadAloud.state.collectAsStateWithLifecycle()
    if (state.active && state.fingerprint == fingerprint) Surface(tonalElevation = 1.dp) {
        Row(Modifier.fillMaxWidth().height(48.dp).padding(start = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(if (state.playing) "Reading aloud" else "Reading paused", Modifier.weight(1f), style = MaterialTheme.typography.labelMedium)
            IconButton(onClick = { if (state.playing) ReadAloud.pause() else ReadAloud.resume() }, modifier = Modifier.testTag("speechPause")) {
                Icon(if (state.playing) AppIcons.Pause else AppIcons.Play, if (state.playing) "Pause" else "Resume")
            }
            IconButton(onClick = ReadAloud::stop, modifier = Modifier.testTag("speechStop")) { Icon(AppIcons.Stop, "Stop reading") }
        }
    }
}
