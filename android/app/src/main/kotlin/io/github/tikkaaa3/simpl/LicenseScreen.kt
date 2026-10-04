@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.tikkaaa3.simpl

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray

internal data class LicenseNotice(val title: String, val category: String, val path: String)

@Composable
internal fun LicenseScreen(back: () -> Unit) {
    val assets = LocalContext.current.assets
    val notices by produceState<List<LicenseNotice>?>(null, assets) {
        value = withContext(Dispatchers.IO) {
            val index = JSONArray(assets.open("licenses/index.json").bufferedReader().use { it.readText() })
            List(index.length()) { index.getJSONObject(it).let { entry ->
                LicenseNotice(entry.getString("title"), entry.getString("category"), entry.getString("path"))
            } }
        }
    }
    var query by rememberSaveable { mutableStateOf("") }
    var selected by rememberSaveable { mutableStateOf<String?>(null) }
    val notice = notices?.find { it.path == selected }
    val fullText by produceState<String?>(null, notice) {
        value = notice?.let { withContext(Dispatchers.IO) { assets.open(it.path).bufferedReader().use { reader -> reader.readText() } } }
    }
    Column(Modifier.fillMaxSize().testTag("licenses")) {
        TopAppBar(title = { Text("Licenses") }, navigationIcon = { IconButton(onClick = back,
            modifier = Modifier.sizeIn(minWidth = 48.dp, minHeight = 48.dp)) { Icon(AppIcons.Back, "Back") } })
        Text("simPl ${BuildConfig.VERSION_NAME}", Modifier.padding(horizontal = 20.dp), style = MaterialTheme.typography.titleMedium)
        Text("Source and release terms, third-party licenses and attribution.", Modifier.padding(20.dp))
        OutlinedTextField(query, { query = it }, label = { Text("Search licenses") }, singleLine = true,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).testTag("licenseSearch"))
        if (notices == null) LinearProgressIndicator(Modifier.fillMaxWidth())
        LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(20.dp)) {
            items(notices.orEmpty().filter { (it.title + " " + it.category).contains(query.trim(), ignoreCase = true) }, key = { it.path }) { item ->
                TextButton(onClick = { selected = item.path }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
                    Column(Modifier.fillMaxWidth()) {
                        Text(item.title)
                        Text(item.category, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
        }
    }
    if (notice != null) ModalBottomSheet(onDismissRequest = { selected = null }) {
        Text(notice.title, Modifier.padding(20.dp), style = MaterialTheme.typography.titleLarge)
        SelectionContainer {
            Text(fullText ?: "Loading license…", Modifier.fillMaxWidth().heightIn(max = 520.dp)
                .verticalScroll(rememberScrollState()).padding(20.dp).testTag("licenseText"), style = MaterialTheme.typography.bodySmall)
        }
    }
}
