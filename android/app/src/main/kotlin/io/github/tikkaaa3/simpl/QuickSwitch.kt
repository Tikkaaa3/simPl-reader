package io.github.tikkaaa3.simpl

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import io.github.tikkaaa3.simpl.core.LibraryBook

@Composable
internal fun QuickSwitch(books: List<LibraryBook>, recent: Boolean, dismiss: () -> Unit, open: (LibraryBook) -> Unit) {
    var query by rememberSaveable { mutableStateOf("") }
    val matches = remember(books, query, recent) { books.asSequence()
        .filter { (!recent || it.openedAt > 0uL) && (it.title.contains(query.trim(), true) || it.author?.contains(query.trim(), true) == true) }
        .sortedByDescending { it.openedAt }.take(100).toList() }
    AlertDialog(onDismissRequest = dismiss, modifier = Modifier.testTag("quickSwitch"),
        title = { Text(if (recent) "Recent books" else "Switch book") },
        text = {
            val focus = remember { FocusRequester() }
            LaunchedEffect(Unit) { focus.requestFocus() }
            Column {
                OutlinedTextField(query, { query = it.take(256) }, singleLine = true, label = { Text("Title or author") },
                    modifier = Modifier.fillMaxWidth().focusRequester(focus).testTag("quickQuery"),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Go),
                    keyboardActions = KeyboardActions(onGo = { matches.firstOrNull()?.let(open) }))
                LazyColumn(Modifier.fillMaxWidth().heightIn(max = 360.dp).testTag("quickResults")) {
                    if (matches.isEmpty()) item { Text("No matching books", Modifier.padding(12.dp)) }
                    items(matches, key = { it.fingerprint }) { book -> TextButton(onClick = { open(book) }, modifier = Modifier.fillMaxWidth().testTag("quick:${book.fingerprint}")) {
                        Column(Modifier.fillMaxWidth()) {
                            Text(book.title, maxLines = 2)
                            Text(if (book.missing) "Locate the missing file" else book.author ?: formatLabel(book.format), style = MaterialTheme.typography.labelMedium)
                        }
                    } }
                }
                Text("Up to 100 books, most recently opened first", style = MaterialTheme.typography.labelSmall)
            }
        }, confirmButton = { TextButton(onClick = dismiss) { Text("Cancel") } })
}
