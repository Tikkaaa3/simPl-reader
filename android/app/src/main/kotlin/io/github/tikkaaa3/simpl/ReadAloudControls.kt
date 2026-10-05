@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.tikkaaa3.simpl

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle

@Composable
internal fun ReadAloudControls(fingerprint: String, start: () -> Unit, enabled: Boolean = true) {
    val state by ReadAloud.state.collectAsStateWithLifecycle()
    val active = state.active && state.fingerprint == fingerprint
    var options by remember { mutableStateOf(false) }
    val context = LocalContext.current
    if (active) {
        TextButton(onClick = { if (state.playing) ReadAloud.pause() else ReadAloud.resume() }, modifier = Modifier.testTag("speechPause")) { Text(if (state.playing) "Pause" else "Resume") }
        TextButton(onClick = ReadAloud::stop, modifier = Modifier.testTag("speechStop")) { Text("Stop reading") }
    } else FilledTonalButton(onClick = start, enabled = enabled, shape = ControlShape, modifier = Modifier.padding(end = 8.dp).testTag("speechStart")) {
        Icon(AppIcons.Speak, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Read aloud")
    }
    TextButton(onClick = { ReadAloud.connect(context); options = true }, colors = quietButtonColors(), shape = ControlShape) { Text("Voice & speed") }
    if (options) ModalBottomSheet(onDismissRequest = { options = false }) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp)) {
            Text("Read aloud", style = MaterialTheme.typography.headlineSmall)
            Text("Speed: %.2f×".format(state.rate))
            Slider(state.rate, { ReadAloud.options(rate = it) }, valueRange = .5f..2f, steps = 14, modifier = Modifier.testTag("speechRate"))
            Text("Voices marked network may send text to the selected speech engine's service. Word highlighting depends on engine timing support.", style = MaterialTheme.typography.bodySmall)
        }
        LazyColumn(Modifier.fillMaxWidth().heightIn(max = 420.dp)) {
            item { ListItem(headlineContent = { Text("Automatic (matches the text)") }, trailingContent = {
                RadioButton(state.voice.isEmpty(), { ReadAloud.options(voice = "") })
            }) }
            items(state.voices, key = { it.id }) { voice -> ListItem(headlineContent = { Text(voice.label) }, trailingContent = {
                RadioButton(state.voice == voice.id, { ReadAloud.options(voice = voice.id) })
            }) }
            if (state.voices.isEmpty()) item { Text("No voices available yet. Check Android text-to-speech settings.", Modifier.padding(20.dp)) }
        }
    }
}

@Composable
internal fun ReadAloudError() {
    val state by ReadAloud.state.collectAsStateWithLifecycle()
    state.error?.let { error -> AlertDialog(onDismissRequest = ReadAloud::dismissError, title = { Text("Read aloud") },
        text = { Text(error) }, confirmButton = { TextButton(onClick = ReadAloud::dismissError) { Text("OK") } }) }
}
