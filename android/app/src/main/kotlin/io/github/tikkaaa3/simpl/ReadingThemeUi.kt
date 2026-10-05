package io.github.tikkaaa3.simpl

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Each theme's reading family, matching `reader-layout`'s bundled themes. */
internal fun themeFamily(id: String): FontFamily = when (id) { "soft" -> spectral; "clear" -> fira; else -> literata }

/** Desktop's theme cards: a live "Aa" sample on the theme's own paper. */
@Composable
internal fun ReadingThemePicker(selected: String, dark: Boolean, modifier: Modifier = Modifier, edge: androidx.compose.ui.unit.Dp = 20.dp, select: (String) -> Unit) {
    LazyRow(modifier.testTag("themePicker"), contentPadding = PaddingValues(horizontal = edge), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        items(ReadingThemes.all, key = { it.id }) { theme ->
            val palette = if (dark) theme.dark else theme.light
            val chosen = theme.id == selected
            Surface(onClick = { select(theme.id) }, shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainer,
                border = BorderStroke(if (chosen) 2.dp else 1.dp, if (chosen) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline),
                modifier = Modifier.width(148.dp).semantics { role = Role.RadioButton; this.selected = chosen }) {
                Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(theme.name, style = MaterialTheme.typography.labelLarge)
                    Text(theme.summary, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
                    Column(Modifier.padding(top = 6.dp).fillMaxWidth().background(rgbaColor(palette.paper), RoundedCornerShape(4.dp))
                        .border(1.dp, rgbaColor(palette.border), RoundedCornerShape(4.dp))
                        .padding(horizontal = 10.dp, vertical = 8.dp)) {
                        Text("Aa", color = rgbaColor(palette.text), fontFamily = themeFamily(theme.id), fontSize = 24.sp)
                        Text("The quiet hours of reading.", color = rgbaColor(palette.text), fontFamily = themeFamily(theme.id),
                            fontSize = 11.sp, lineHeight = 14.sp, maxLines = 1)
                    }
                }
            }
        }
    }
}

/** A row of mutually exclusive choices drawn like desktop's segmented toggles. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun <T> ChoiceRow(choices: List<Pair<T, String>>, selected: T, modifier: Modifier = Modifier, select: (T) -> Unit) {
    SingleChoiceSegmentedButtonRow(modifier) {
        choices.forEachIndexed { index, (value, label) ->
            SegmentedButton(selected = value == selected, onClick = { select(value) },
                shape = SegmentedButtonDefaults.itemShape(index, choices.size, RoundedCornerShape(4.dp)),
                colors = SegmentedButtonDefaults.colors(activeContainerColor = MaterialTheme.colorScheme.secondaryContainer,
                    activeContentColor = MaterialTheme.colorScheme.onSecondaryContainer, activeBorderColor = MaterialTheme.colorScheme.outline,
                    inactiveBorderColor = MaterialTheme.colorScheme.outline),
                icon = {}, label = { Text(label, maxLines = 1) })
        }
    }
}
