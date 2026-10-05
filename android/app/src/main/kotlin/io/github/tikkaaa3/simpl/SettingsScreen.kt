@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.tikkaaa3.simpl

import android.content.Intent
import android.content.res.Configuration
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import io.github.tikkaaa3.simpl.core.LibraryShelf

@Composable
internal fun SettingsScreen(state: LibraryState, model: LibraryViewModel, back: () -> Unit,
    dictionaries: () -> Unit, backups: () -> Unit, licenses: () -> Unit,
    createShelf: () -> Unit, editShelf: (LibraryShelf) -> Unit, deleteShelf: (LibraryShelf) -> Unit) {
    val context = LocalContext.current
    val controls by ReadingControls.state.collectAsState()
    val theme by ReadingThemes.selected.collectAsState()
    val dark = isAppDark(state.appearance)
    // Shortcuts only matter with a hardware keyboard attached.
    val configuration = LocalConfiguration.current
    val keyboard = configuration.keyboard == Configuration.KEYBOARD_QWERTY && configuration.hardKeyboardHidden == Configuration.HARDKEYBOARDHIDDEN_NO
    var keyboardHelp by rememberSaveable { mutableStateOf(false) }
    Column(Modifier.fillMaxSize().testTag("settings")) {
        TopAppBar(title = { Text("Settings") }, navigationIcon = { IconButton(onClick = back) { Icon(AppIcons.Back, "Back") } },
            colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background))
        LazyColumn(Modifier.weight(1f).testTag("settingsList"), contentPadding = PaddingValues(bottom = 32.dp)) {
            item { SectionTitle("Appearance", divider = false) }
            item {
                ChoiceRow(listOf(Appearance.System to "Device", Appearance.Light to "Light", Appearance.Dark to "Dark"), state.appearance,
                    Modifier.fillMaxWidth().padding(horizontal = 20.dp).testTag("appearance"), model::appearance)
            }
            item {
                Text("Reading theme", Modifier.padding(start = 20.dp, top = 20.dp, bottom = 8.dp), style = MaterialTheme.typography.titleSmall)
                ReadingThemePicker(theme, dark, Modifier.fillMaxWidth()) { ReadingThemes.select(context, it) }
                Text("A theme sets the reading font, spacing and colors. Page numbers stay the same in every theme.",
                    Modifier.padding(horizontal = 20.dp, vertical = 8.dp), style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            item { SectionTitle("Reading") }
            item {
                SettingSwitch("Volume keys turn pages", "While reading: up goes back, down goes forward", controls.volumeTurns, "volumeTurns") {
                    ReadingControls.update(context, volumeTurns = it)
                }
            }
            item {
                SettingSwitch("Keep screen on while reading", null, controls.keepScreenOn, "keepScreenOn") {
                    ReadingControls.update(context, keepScreenOn = it)
                }
            }
            if (keyboard) item { SettingLink("Keyboard shortcuts", null) { keyboardHelp = true } }
            item { SectionTitle("Shelves") }
            items(state.shelves, key = { it.id.toString() }) { shelf ->
                var menu by rememberSaveable { mutableStateOf(false) }
                ListItem(headlineContent = { Text(shelf.name) },
                    supportingContent = { Text("${state.books.count { it.fingerprint in shelf.books }} books") },
                    trailingContent = { Box {
                        IconButton(onClick = { menu = true }) { Icon(AppIcons.More, "Manage ${shelf.name}") }
                        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                            DropdownMenuItem(text = { Text("Rename shelf") }, onClick = { menu = false; editShelf(shelf) })
                            DropdownMenuItem(text = { Text("Delete shelf") }, onClick = { menu = false; deleteShelf(shelf) })
                        }
                    } }, colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background))
            }
            item { SettingLink("New shelf", null, AppIcons.Plus, onClick = createShelf) }
            item { SectionTitle("Library data") }
            item { SettingLink("Offline dictionaries", "Download word translations for offline use", onClick = dictionaries) }
            item { SettingLink("Backup and export", "Save books, notes and progress to a file", onClick = backups) }
            item { SectionTitle("About") }
            item { SettingLink("Licenses", null, onClick = licenses) }
            item { SettingLink("Privacy policy", "Opens in your browser") {
                context.startActivity(Intent(Intent.ACTION_VIEW, "https://github.com/Tikkaaa3/simPl-reader/blob/main/docs/privacy.md".toUri()))
            } }
            item {
                Column(Modifier.padding(horizontal = 20.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text("Your books stay on this device", style = MaterialTheme.typography.titleSmall)
                    Text("Imports are copied into private storage. Removing a book never deletes its original file.",
                        style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text("simPl ${BuildConfig.VERSION_NAME}", Modifier.padding(top = 8.dp), style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
    if (keyboardHelp) KeyboardHelp { keyboardHelp = false }
}

@Composable
private fun SectionTitle(text: String, divider: Boolean = true) {
    Column {
        if (divider) HorizontalDivider(Modifier.padding(top = 12.dp))
        Text(text, Modifier.padding(start = 20.dp, top = 20.dp, bottom = 8.dp), style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.primary)
    }
}

@Composable
private fun SettingSwitch(title: String, detail: String?, checked: Boolean, tag: String, change: (Boolean) -> Unit) {
    ListItem(headlineContent = { Text(title) }, supportingContent = detail?.let { { Text(it) } },
        trailingContent = { Switch(checked, change, Modifier.testTag(tag)) },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
        modifier = Modifier.clickable { change(!checked) })
}

@Composable
private fun SettingLink(title: String, detail: String?, icon: ImageVector = AppIcons.ChevronRight, onClick: () -> Unit) {
    ListItem(headlineContent = { Text(title) }, supportingContent = detail?.let { { Text(it) } },
        trailingContent = { Icon(icon, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
        modifier = Modifier.clickable(onClick = onClick))
}
